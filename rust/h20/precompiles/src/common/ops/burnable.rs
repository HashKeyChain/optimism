use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::SolEvent;
use h20_precompile_storage::{H20PrecompileError, Result};

use crate::{H20Guards, H20TokenRole, IH20, Token, TokenAccounting};

/// Token burn operations.
///
/// All methods have default implementations that go through [`Token::accounting`].
/// Implement this trait with an empty body to opt in.
pub trait Burnable: Token {
    /// Destroys `amount` tokens from `from`. Emits `Transfer(from, 0x0, amount)`.
    fn burn(
        &mut self,
        caller: Address,
        from: Address,
        amount: U256,
        privileged: bool,
    ) -> Result<()> {
        H20Guards::ensure_not_paused::<Self>(self, IH20::PausableFeature::BURN)?;
        if !privileged {
            H20Guards::ensure_token_role::<Self>(self, caller, H20TokenRole::Burn)?;
        }
        self.burn_inner(from, amount)
    }

    /// Internal burn implementation that skips pause and role checks.
    ///
    /// Called by [`Self::burn`] after guards, and by [`Self::burn_blocked`]
    /// which does its own pause and role checks first.
    fn burn_inner(&mut self, from: Address, amount: U256) -> Result<()> {
        let balance = self.accounting().balance_of(from)?;
        if balance < amount {
            return Err(H20PrecompileError::revert(IH20::InsufficientBalance {
                sender: from,
                balance,
                needed: amount,
            }));
        }
        self.accounting_mut().set_balance(from, balance - amount)?;
        let supply = self.accounting().total_supply()?;
        let new_supply =
            supply.checked_sub(amount).ok_or_else(H20PrecompileError::under_overflow)?;
        self.accounting_mut().set_total_supply(new_supply)?;
        self.accounting_mut()
            .emit_event(IH20::Transfer { from, to: Address::ZERO, amount }.encode_log_data())
    }

    /// [`Self::burn`] followed by a `Memo` event.
    fn burn_with_memo(
        &mut self,
        caller: Address,
        from: Address,
        amount: U256,
        memo: B256,
        privileged: bool,
    ) -> Result<()> {
        self.burn(caller, from, amount, privileged)?;
        self.accounting_mut().emit_event(IH20::Memo { caller, memo }.encode_log_data())
    }

    /// Destroys `amount` from a policy-blocked account. Emits `Transfer` and `BurnedBlocked`.
    fn burn_blocked(
        &mut self,
        caller: Address,
        from: Address,
        amount: U256,
        privileged: bool,
    ) -> Result<()> {
        H20Guards::ensure_not_paused::<Self>(self, IH20::PausableFeature::BURN)?;
        if !privileged {
            H20Guards::ensure_token_role::<Self>(self, caller, H20TokenRole::BurnBlocked)?;
        }
        H20Guards::ensure_blocked::<Self>(self, from)?;
        self.burn_inner(from, amount)?;
        self.accounting_mut()
            .emit_event(IH20::BurnedBlocked { caller, from, amount }.encode_log_data())
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, U256};
    use h20_precompile_storage::H20PrecompileError;
    use rstest::rstest;

    use crate::{
        Burnable, FakePolicyAccounting, H20PausableFeature, H20PolicyType, H20TokenRole, IH20,
        InMemoryTokenAccounting, PolicyRegistryStorage, TestToken, Token, TokenAccounting,
    };

    const CALLER: Address = Address::repeat_byte(0xcc);
    const ALICE: Address = Address::repeat_byte(0xaa);
    const TOKEN_ADDR: Address = Address::repeat_byte(1);

    fn token_with_balance(balance: U256) -> TestToken {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, balance);
        accounting.total_supply = balance;
        TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new())
    }

    fn token_with_role(role: H20TokenRole, account: Address, balance: U256) -> TestToken {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, balance);
        accounting.total_supply = balance;
        accounting.roles.insert((role.id(), account), true);
        TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new())
    }

    #[test]
    fn burn_decreases_balance_and_supply() {
        let mut token = token_with_balance(U256::from(100u64));

        token.burn(CALLER, ALICE, U256::from(40u64), true).unwrap();

        assert_eq!(token.accounting().balance_of(ALICE).unwrap(), U256::from(60u64));
        assert_eq!(token.accounting().total_supply().unwrap(), U256::from(60u64));
        assert_eq!(token.accounting().events.len(), 1);
    }

    #[test]
    fn burn_insufficient_balance_reverts() {
        let mut token = token_with_balance(U256::from(10u64));

        assert_eq!(
            token.burn(CALLER, ALICE, U256::from(11u64), true).unwrap_err(),
            H20PrecompileError::revert(IH20::InsufficientBalance {
                sender: ALICE,
                balance: U256::from(10u64),
                needed: U256::from(11u64),
            })
        );
    }

    #[test]
    fn non_privileged_burn_with_role_succeeds() {
        let mut token = token_with_role(H20TokenRole::Burn, CALLER, U256::from(10u64));

        token.burn(CALLER, ALICE, U256::from(4u64), false).unwrap();

        assert_eq!(token.accounting().balance_of(ALICE).unwrap(), U256::from(6u64));
    }

    #[test]
    fn burn_reverts_when_burn_feature_paused() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, U256::from(10u64));
        accounting.total_supply = U256::from(10u64);
        accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::BURN);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.burn(CALLER, ALICE, U256::ONE, true).unwrap_err(),
            H20PrecompileError::revert(IH20::ContractPaused {
                feature: IH20::PausableFeature::BURN,
            })
        );
    }

    #[test]
    fn burn_blocked_paused_gets_pause_error_not_blocked_error() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::BURN);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.burn_blocked(CALLER, ALICE, U256::ONE, true).unwrap_err(),
            H20PrecompileError::revert(IH20::ContractPaused {
                feature: IH20::PausableFeature::BURN,
            })
        );
    }

    #[test]
    fn burn_blocked_reverts_when_account_is_not_blocked() {
        let mut token = token_with_balance(U256::from(10u64));

        assert_eq!(
            token.burn_blocked(CALLER, ALICE, U256::ONE, true).unwrap_err(),
            H20PrecompileError::revert(IH20::AccountNotBlocked { account: ALICE })
        );
    }

    #[test]
    fn burn_blocked_burns_blocked_account_and_emits_events() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, U256::from(100u64));
        accounting.total_supply = U256::from(100u64);
        accounting
            .policy_ids
            .insert(H20PolicyType::TransferSender.id(), PolicyRegistryStorage::ALWAYS_BLOCK_ID);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        token.burn_blocked(CALLER, ALICE, U256::from(25u64), true).unwrap();

        assert_eq!(token.accounting().balance_of(ALICE).unwrap(), U256::from(75u64));
        assert_eq!(token.accounting().total_supply().unwrap(), U256::from(75u64));
        assert_eq!(token.accounting().events.len(), 2);
    }

    #[test]
    fn non_privileged_burn_blocked_without_role_reverts() {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, U256::from(10u64));
        accounting.total_supply = U256::from(10u64);
        accounting
            .policy_ids
            .insert(H20PolicyType::TransferSender.id(), PolicyRegistryStorage::ALWAYS_BLOCK_ID);
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.burn_blocked(CALLER, ALICE, U256::ONE, false).unwrap_err(),
            H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
                account: CALLER,
                neededRole: H20TokenRole::BurnBlocked.id(),
            })
        );
    }

    // ---- Guard ordering tests ----

    #[rstest]
    #[case::paused_without_role_gets_pause_error(
        true,  // paused
        false, // has_role
        false, // privileged
        H20PrecompileError::revert(IH20::ContractPaused { feature: IH20::PausableFeature::BURN })
    )]
    #[case::paused_privileged_still_gets_pause_error(
        true,  // paused
        true,  // has_role
        true,  // privileged
        H20PrecompileError::revert(IH20::ContractPaused { feature: IH20::PausableFeature::BURN })
    )]
    #[case::role_before_balance_for_non_privileged(
        false, // paused
        false, // has_role
        false, // privileged
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: CALLER,
            neededRole: H20TokenRole::Burn.id(),
        })
    )]
    fn burn_guard_ordering(
        #[case] paused: bool,
        #[case] has_role: bool,
        #[case] privileged: bool,
        #[case] expected_error: H20PrecompileError,
    ) {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, U256::from(10u64));
        accounting.total_supply = U256::from(10u64);
        if paused {
            accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::BURN);
        }
        if has_role {
            accounting.roles.insert((H20TokenRole::Burn.id(), CALLER), true);
        }
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(token.burn(CALLER, ALICE, U256::ONE, privileged).unwrap_err(), expected_error);
    }

    #[rstest]
    #[case::paused_without_role_gets_pause_error(
        true,  // paused
        false, // has_role
        false, // is_blocked
        false, // privileged
        H20PrecompileError::revert(IH20::ContractPaused { feature: IH20::PausableFeature::BURN })
    )]
    #[case::paused_privileged_still_gets_pause_error(
        true,  // paused
        true,  // has_role
        true,  // is_blocked
        true,  // privileged
        H20PrecompileError::revert(IH20::ContractPaused { feature: IH20::PausableFeature::BURN })
    )]
    #[case::role_before_blocked_check(
        false, // paused
        false, // has_role
        true,  // is_blocked
        false, // privileged
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: CALLER,
            neededRole: H20TokenRole::BurnBlocked.id(),
        })
    )]
    #[case::blocked_check_before_burn_for_privileged(
        false, // paused
        true,  // has_role (not used when privileged, but keep consistent)
        false, // is_blocked
        true,  // privileged
        H20PrecompileError::revert(IH20::AccountNotBlocked { account: ALICE })
    )]
    fn burn_blocked_guard_ordering(
        #[case] paused: bool,
        #[case] has_role: bool,
        #[case] is_blocked: bool,
        #[case] privileged: bool,
        #[case] expected_error: H20PrecompileError,
    ) {
        let mut accounting = InMemoryTokenAccounting::new(TOKEN_ADDR);
        accounting.balances.insert(ALICE, U256::from(10u64));
        accounting.total_supply = U256::from(10u64);
        if paused {
            accounting.paused = H20PausableFeature::mask(IH20::PausableFeature::BURN);
        }
        if has_role {
            accounting.roles.insert((H20TokenRole::BurnBlocked.id(), CALLER), true);
        }
        if is_blocked {
            accounting
                .policy_ids
                .insert(H20PolicyType::TransferSender.id(), PolicyRegistryStorage::ALWAYS_BLOCK_ID);
        }
        let mut token = TestToken::with_storage_and_policy(accounting, FakePolicyAccounting::new());

        assert_eq!(
            token.burn_blocked(CALLER, ALICE, U256::ONE, privileged).unwrap_err(),
            expected_error
        );
    }
}
