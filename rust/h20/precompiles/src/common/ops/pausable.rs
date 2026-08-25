use alloc::vec::Vec;

use alloy_primitives::{Address, U256};
use alloy_sol_types::SolEvent;
use h20_precompile_storage::{H20PrecompileError, Result};

use crate::{H20Guards, H20PausableFeature, H20TokenRole, IH20, Token, TokenAccounting};

/// Pause and unpause operations.
///
/// All methods have default implementations that go through [`Token::accounting`].
/// Implement this trait with an empty body to opt in.
pub trait Pausable: Token {
    /// Returns whether the given pause `feature` is currently set.
    fn is_paused(&self, feature: IH20::PausableFeature) -> Result<bool> {
        H20PausableFeature::ensure_valid(feature)?;
        Ok((self.accounting().paused()? & H20PausableFeature::mask(feature)) != U256::ZERO)
    }

    /// Returns all currently paused features.
    fn paused_features(&self) -> Result<Vec<IH20::PausableFeature>> {
        let paused = self.accounting().paused()?;
        let mut features = Vec::new();
        for feature in [
            IH20::PausableFeature::TRANSFER,
            IH20::PausableFeature::MINT,
            IH20::PausableFeature::BURN,
        ] {
            if (paused & H20PausableFeature::mask(feature)) != U256::ZERO {
                features.push(feature);
            }
        }
        Ok(features)
    }

    /// ORs `features` into the current paused bitmask.
    fn pause(
        &mut self,
        caller: Address,
        features: Vec<IH20::PausableFeature>,
        privileged: bool,
    ) -> Result<()> {
        for feature in &features {
            H20PausableFeature::ensure_valid(*feature)?;
        }
        if !privileged {
            H20Guards::ensure_token_role::<Self>(self, caller, H20TokenRole::Pause)?;
        }
        if features.is_empty() {
            return Err(H20PrecompileError::revert(IH20::EmptyFeatureSet {}));
        }
        let current = self.accounting().paused()?;
        let mut next = current;
        for feature in &features {
            next |= H20PausableFeature::mask(*feature);
        }
        self.accounting_mut().set_paused(next)?;
        self.accounting_mut()
            .emit_event(IH20::Paused { updater: caller, features }.encode_log_data())
    }

    /// Clears `features` from the current paused bitmask.
    fn unpause(
        &mut self,
        caller: Address,
        features: Vec<IH20::PausableFeature>,
        privileged: bool,
    ) -> Result<()> {
        for feature in &features {
            H20PausableFeature::ensure_valid(*feature)?;
        }
        if !privileged {
            H20Guards::ensure_token_role::<Self>(self, caller, H20TokenRole::Unpause)?;
        }
        if features.is_empty() {
            return Err(H20PrecompileError::revert(IH20::EmptyFeatureSet {}));
        }
        let mut next = self.accounting().paused()?;
        for feature in &features {
            next &= !H20PausableFeature::mask(*feature);
        }
        self.accounting_mut().set_paused(next)?;
        self.accounting_mut()
            .emit_event(IH20::Unpaused { updater: caller, features }.encode_log_data())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use alloy_primitives::Address;
    use h20_precompile_storage::H20PrecompileError;

    use crate::{
        FakePolicyAccounting, H20PausableFeature, H20TokenRole, IH20, InMemoryTokenAccounting,
        Pausable, TestToken, Token,
    };

    const CALLER: Address = Address::repeat_byte(0xaa);
    const TOKEN_ADDR: Address = Address::repeat_byte(1);

    fn make_token() -> TestToken {
        TestToken::with_storage_and_policy(
            InMemoryTokenAccounting::new(TOKEN_ADDR),
            FakePolicyAccounting::new(),
        )
    }

    fn token_with_role(role: H20TokenRole, account: Address) -> TestToken {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.roles.insert((role.id(), account), true);
        TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new())
    }

    #[test]
    fn pause_sets_feature_and_emits_event() {
        let mut token = make_token();

        token.pause(CALLER, vec![IH20::PausableFeature::TRANSFER], true).unwrap();

        assert!(token.is_paused(IH20::PausableFeature::TRANSFER).unwrap());
        assert_eq!(token.accounting().events.len(), 1);
    }

    #[test]
    fn pause_ors_multiple_features_into_existing_bitmask() {
        let mut token = make_token();

        token.pause(CALLER, vec![IH20::PausableFeature::TRANSFER], true).unwrap();
        token
            .pause(CALLER, vec![IH20::PausableFeature::MINT, IH20::PausableFeature::BURN], true)
            .unwrap();

        assert!(token.is_paused(IH20::PausableFeature::TRANSFER).unwrap());
        assert!(token.is_paused(IH20::PausableFeature::MINT).unwrap());
        assert!(token.is_paused(IH20::PausableFeature::BURN).unwrap());
    }

    #[test]
    fn unpause_clears_selected_feature_and_leaves_others_paused() {
        let mut token = make_token();

        token
            .pause(CALLER, vec![IH20::PausableFeature::TRANSFER, IH20::PausableFeature::MINT], true)
            .unwrap();
        token.unpause(CALLER, vec![IH20::PausableFeature::MINT], true).unwrap();

        assert!(token.is_paused(IH20::PausableFeature::TRANSFER).unwrap());
        assert!(!token.is_paused(IH20::PausableFeature::MINT).unwrap());
    }

    #[test]
    fn paused_features_returns_active_features_in_abi_order() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::TRANSFER) |
            H20PausableFeature::mask(IH20::PausableFeature::BURN);
        let token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.paused_features().unwrap(),
            vec![IH20::PausableFeature::TRANSFER, IH20::PausableFeature::BURN]
        );
    }

    #[test]
    fn pause_empty_feature_set_reverts() {
        let mut token = make_token();

        assert_eq!(
            token.pause(CALLER, vec![], true).unwrap_err(),
            H20PrecompileError::revert(IH20::EmptyFeatureSet {})
        );
    }

    #[test]
    fn unpause_empty_feature_set_reverts() {
        let mut token = make_token();

        assert_eq!(
            token.unpause(CALLER, vec![], true).unwrap_err(),
            H20PrecompileError::revert(IH20::EmptyFeatureSet {})
        );
    }

    #[test]
    fn non_privileged_pause_without_role_reverts() {
        let mut token = make_token();

        assert_eq!(
            token.pause(CALLER, vec![IH20::PausableFeature::TRANSFER], false).unwrap_err(),
            H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
                account: CALLER,
                neededRole: H20TokenRole::Pause.id(),
            })
        );
    }

    #[test]
    fn non_privileged_pause_with_role_succeeds() {
        let mut token = token_with_role(H20TokenRole::Pause, CALLER);

        token.pause(CALLER, vec![IH20::PausableFeature::TRANSFER], false).unwrap();

        assert!(token.is_paused(IH20::PausableFeature::TRANSFER).unwrap());
    }

    #[test]
    fn non_privileged_unpause_without_role_reverts() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::TRANSFER);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.unpause(CALLER, vec![IH20::PausableFeature::TRANSFER], false).unwrap_err(),
            H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
                account: CALLER,
                neededRole: H20TokenRole::Unpause.id(),
            })
        );
    }

    #[test]
    fn non_privileged_unpause_with_role_succeeds() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::TRANSFER);
        accounting.roles.insert((H20TokenRole::Unpause.id(), CALLER), true);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        token.unpause(CALLER, vec![IH20::PausableFeature::TRANSFER], false).unwrap();

        assert!(!token.is_paused(IH20::PausableFeature::TRANSFER).unwrap());
    }
}
