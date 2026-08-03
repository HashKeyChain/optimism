//! ABI dispatch for the activation registry.

use alloy_primitives::Bytes;
use alloy_sol_types::SolCall;
use b20_precompile_storage::StorageCtx;
use revm::precompile::PrecompileResult;

use crate::{
    ActivationAdminConfig, ActivationRegistryStorage, BerylCallRecorder, BerylMetricLabels,
    IActivationRegistry::{self, IActivationRegistryCalls as C},
    NoopPrecompileCallObserver, PrecompileCallObserver,
    macros::decode_precompile_call,
};

impl ActivationRegistryStorage<'_> {
    /// ABI-dispatches activation registry calldata.
    pub fn dispatch(
        &mut self,
        ctx: StorageCtx<'_>,
        calldata: &[u8],
        admin_config: ActivationAdminConfig,
    ) -> PrecompileResult {
        self.dispatch_with_observer(ctx, calldata, admin_config, NoopPrecompileCallObserver)
    }

    /// ABI-dispatches activation registry calldata with an observer.
    pub fn dispatch_with_observer<O>(
        &mut self,
        ctx: StorageCtx<'_>,
        calldata: &[u8],
        admin_config: ActivationAdminConfig,
        observer: O,
    ) -> PrecompileResult
    where
        O: PrecompileCallObserver,
    {
        let mut recorder =
            BerylCallRecorder::start(observer, BerylMetricLabels::activation_call(calldata));
        if let Err(error) = recorder.deduct_calldata_gas(ctx, calldata) {
            return recorder.record_base_error_result(ctx, error);
        }
        recorder.record_base_result(ctx, self.inner(calldata, admin_config), |output| output)
    }

    fn inner(
        &mut self,
        calldata: &[u8],
        admin_config: ActivationAdminConfig,
    ) -> b20_precompile_storage::Result<Bytes> {
        match decode_precompile_call!(calldata, IActivationRegistry::IActivationRegistryCalls) {
            C::isActivated(call) => {
                let activated = self.is_activated(call.feature)?;
                Ok(IActivationRegistry::isActivatedCall::abi_encode_returns(&activated).into())
            }
            C::checkActivated(call) => {
                self.ensure_activated(call.feature)?;
                Ok(Bytes::new())
            }
            C::activate(call) => {
                self.activate(call.feature, admin_config)?;
                Ok(Bytes::new())
            }
            C::deactivate(call) => {
                self.deactivate(call.feature, admin_config)?;
                Ok(Bytes::new())
            }
            C::admin(_) => {
                Ok(IActivationRegistry::adminCall::abi_encode_returns(&self.admin(admin_config)?)
                    .into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, Bytes, address};
    use b20_precompile_storage::{HashMapStorageProvider, StorageCtx};

    use crate::{ActivationAdminConfig, ActivationRegistryStorage};

    const ADMIN: Address = address!("0xcb00000000000000000000000000000000000000");
    const SET_ADMIN_SELECTOR: [u8; 4] = [0x70, 0x4b, 0x6c, 0x02];
    const STATIC_ADMIN_CONFIG: ActivationAdminConfig =
        ActivationAdminConfig::static_fallback(Some(ADMIN));

    #[test]
    fn dispatch_treats_set_admin_as_unknown_in_beryl() {
        let malformed = Bytes::copy_from_slice(&SET_ADMIN_SELECTOR);
        let mut valid = SET_ADMIN_SELECTOR.to_vec();
        valid.extend_from_slice(&[0_u8; 32]);
        let valid = Bytes::from(valid);

        for calldata in [malformed, valid] {
            let mut storage = HashMapStorageProvider::new(1);
            storage.set_caller(ADMIN);

            let output = StorageCtx::enter(&mut storage, |ctx| {
                ActivationRegistryStorage::new(ctx).dispatch(ctx, &calldata, STATIC_ADMIN_CONFIG)
            })
            .expect("unknown selector must be returned as a revert");

            assert!(output.is_revert(), "setAdmin must revert under Beryl");
            assert_eq!(
                output.bytes,
                Bytes::copy_from_slice(&SET_ADMIN_SELECTOR),
                "Beryl setAdmin must preserve the unknown-selector output"
            );
        }
    }
}
