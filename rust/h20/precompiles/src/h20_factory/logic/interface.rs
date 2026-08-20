//! Append-only business-logic interface for the B-20 token factory precompile.

use crate::H20Spec;
use alloy_primitives::{Address, B256};
use h20_precompile_storage::Result;

use crate::{H20FactoryStorage, IH20Factory};

/// The B-20 token factory logic interface.
///
/// This trait is append-only: new versions add methods, never remove or change the
/// signature of an existing one.
pub trait Factory {
    /// Creates a token at a deterministic address derived from `(caller, variant, salt)`.
    ///
    /// `address_hash` must be `keccak256(abi_encode(caller, call.salt))`. Computing (and
    /// metering) that hash is the dispatcher's responsibility; this method only consumes
    /// the result. `upgrade` selects the policy-logic version the created token is bound to.
    fn create_h20(
        &self,
        storage: &mut H20FactoryStorage<'_>,
        call: IH20Factory::createH20Call,
        address_hash: B256,
        upgrade: H20Spec,
    ) -> Result<Address>;

    // --- version-invariant reads: default pass-throughs to `H20FactoryStorage` ---

    /// Returns whether `token` has the structural B-20 prefix.
    fn is_h20(&self, storage: &H20FactoryStorage<'_>, token: Address) -> Result<bool> {
        storage.is_h20(token)
    }

    /// Returns whether `token` is a B-20 address that has been initialized by this factory.
    fn is_h20_initialized(&self, storage: &H20FactoryStorage<'_>, token: Address) -> Result<bool> {
        storage.is_h20_initialized(token)
    }
}
