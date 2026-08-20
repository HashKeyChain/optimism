//! Version 1 of the B-20 token factory precompile logic, activated at Beryl.

use alloc::{string::ToString, vec::Vec};

use crate::H20Spec;
use alloy_primitives::{Address, B256, Bytes, U256};
use alloy_sol_types::{SolCall, SolEvent, SolValue};
use h20_precompile_storage::{BasePrecompileError, ContractStorage, Result};
use revm::state::Bytecode;

use crate::{
    ActivationRegistryStorage, AssetVersion, H20AssetInit, H20AssetStorage, H20AssetToken,
    H20FactoryStorage, H20StablecoinInit, H20StablecoinStorage, H20StablecoinToken, H20TokenRole,
    H20Variant, Factory, IH20Factory, NoopPrecompileCallObserver, PolicyRegistryStorage,
    PolicyVersions, StablecoinVersion, Token,
};

/// Version byte for `H20StablecoinEventParams` inside `H20Created.variantParams`.
const H20_STABLECOIN_EVENT_PARAMS_VERSION: u8 = 1;

/// ABI-encodes stablecoin-specific `variantParams` for `H20Created`.
/// DEFAULT and ASSET call sites use `Bytes::new()` directly.
fn encode_stablecoin_variant_params(currency: &str) -> Bytes {
    IH20Factory::H20StablecoinEventParams {
        version: H20_STABLECOIN_EVENT_PARAMS_VERSION,
        currency: currency.to_string(),
    }
    .abi_encode()
    .into()
}

/// Initial multiplier storage value. Reads treat zero as WAD precision (1:1).
const INITIAL_MULTIPLIER: U256 = U256::ZERO;

/// First B-20 token factory logic implementation. Frozen as of its activation at Beryl.
#[derive(Debug, Default, Clone, Copy)]
pub struct FactoryV1;

impl FactoryV1 {
    fn init_stablecoin(
        &self,
        storage: &H20FactoryStorage<'_>,
        token_address: Address,
        common: CommonParams,
        init: H20StablecoinInit,
        init_calls: Vec<Bytes>,
        upgrade: H20Spec,
    ) -> Result<()> {
        let policy_version = PolicyVersions::from_spec(upgrade)
            .ok_or_else(|| BasePrecompileError::Revert(Bytes::new()))?;
        let mut token = H20StablecoinToken::with_storage_and_policy(
            H20StablecoinStorage::from_address(token_address, storage.storage()),
            PolicyRegistryStorage::new(storage.storage()),
            policy_version,
        );
        let (name, symbol, currency) =
            (init.name.clone(), init.symbol.clone(), init.currency.clone());
        token.accounting_mut().initialize(init)?;

        storage.storage().emit_event(
            storage.address(),
            IH20Factory::H20Created {
                token: token_address,
                variant: H20Variant::Stablecoin.abi(),
                name,
                symbol,
                decimals: H20Variant::Stablecoin
                    .decimals()
                    .expect("stablecoin has fixed 6-decimal precision"),
                variantParams: encode_stablecoin_variant_params(&currency),
            }
            .encode_log_data(),
        )?;

        if !common.initial_admin.is_zero() {
            token.grant_role_unchecked(
                H20TokenRole::DefaultAdmin.id(),
                common.initial_admin,
                H20FactoryStorage::ADDRESS,
            )?;
        }

        // Pinned to V1: factory-init setup calls run before any later fork can be relevant.
        storage.storage().with_caller(H20FactoryStorage::ADDRESS, || {
            for (index, calldata) in init_calls.into_iter().enumerate() {
                token
                    .route(
                        storage.storage(),
                        &calldata,
                        StablecoinVersion::V1,
                        true,
                        NoopPrecompileCallObserver,
                    )
                    .map_err(|err| Self::map_init_call_error(index, err))?;
            }
            Ok::<(), BasePrecompileError>(())
        })?;
        Ok(())
    }

    fn init_asset_token(
        &self,
        storage: &H20FactoryStorage<'_>,
        token_address: Address,
        common: CommonParams,
        init: H20AssetInit,
        init_calls: Vec<Bytes>,
        upgrade: H20Spec,
    ) -> Result<()> {
        let policy_version = PolicyVersions::from_spec(upgrade)
            .ok_or_else(|| BasePrecompileError::Revert(Bytes::new()))?;
        let mut token = H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(token_address, storage.storage()),
            PolicyRegistryStorage::new(storage.storage()),
            policy_version,
        );
        let (name, symbol, decimals) = (init.name.clone(), init.symbol.clone(), init.decimals);
        token.accounting_mut().initialize(init)?;

        storage.storage().emit_event(
            storage.address(),
            IH20Factory::H20Created {
                token: token_address,
                variant: H20Variant::Asset.abi(),
                name,
                symbol,
                decimals,
                variantParams: Bytes::new(),
            }
            .encode_log_data(),
        )?;

        if !common.initial_admin.is_zero() {
            token.grant_role_unchecked(
                H20TokenRole::DefaultAdmin.id(),
                common.initial_admin,
                H20FactoryStorage::ADDRESS,
            )?;
        }

        // Pinned to V1: factory-init setup calls run before any later fork can be relevant.
        storage.storage().with_caller(H20FactoryStorage::ADDRESS, || {
            for (index, calldata) in init_calls.into_iter().enumerate() {
                token
                    .route(
                        storage.storage(),
                        &calldata,
                        AssetVersion::V1,
                        true,
                        NoopPrecompileCallObserver,
                    )
                    .map_err(|err| Self::map_init_call_error(index, err))?;
            }
            Ok::<(), BasePrecompileError>(())
        })?;
        Ok(())
    }

    fn check_version(version: u8, variant: H20Variant) -> Result<()> {
        if version != variant.supported_version() {
            return Err(BasePrecompileError::revert(IH20Factory::UnsupportedVersion {
                version,
                variant: variant.abi(),
            }));
        }
        Ok(())
    }

    fn map_init_call_error(index: usize, err: BasePrecompileError) -> BasePrecompileError {
        match err {
            BasePrecompileError::Revert(bytes) if !bytes.is_empty() => {
                BasePrecompileError::Revert(bytes)
            }
            err if err.is_system_error() => err,
            _ => BasePrecompileError::revert(IH20Factory::InitCallFailed {
                index: U256::from(index),
            }),
        }
    }
}

impl Factory for FactoryV1 {
    fn create_h20(
        &self,
        storage: &mut H20FactoryStorage<'_>,
        call: IH20Factory::createH20Call,
        address_hash: B256,
        upgrade: H20Spec,
    ) -> Result<Address> {
        let variant = H20Variant::from_abi(call.variant)
            .ok_or_else(|| BasePrecompileError::revert(IH20Factory::InvalidVariant {}))?;
        ActivationRegistryStorage::new(storage.storage())
            .ensure_activated(variant.activation_feature().id())?;
        let params = TokenCreateParams::decode(variant, &call.params)?;
        Self::check_version(params.version(), variant)?;
        params.validate()?;
        let (token_address, _) = variant.compute_address_from_hash(address_hash);

        let already_deployed = storage
            .storage()
            .with_account_info(token_address, |info| Ok(!info.is_empty_code_hash()))?;
        if already_deployed {
            return Err(BasePrecompileError::revert(IH20Factory::TokenAlreadyExists {
                token: token_address,
            }));
        }

        let checkpoint = storage.storage().checkpoint();
        let stub = Bytecode::new_legacy(Bytes::from_static(&[0xef]));
        storage.storage().set_code(token_address, stub)?;

        let init_calls = call.initCalls;
        match params {
            TokenCreateParams::Stablecoin { common, init } => {
                self.init_stablecoin(storage, token_address, common, init, init_calls, upgrade)?;
            }
            TokenCreateParams::Asset { common, init } => {
                self.init_asset_token(storage, token_address, common, init, init_calls, upgrade)?;
            }
        }

        checkpoint.commit();
        Ok(token_address)
    }
}

/// Control-flow fields shared by every token variant (not written to storage).
#[derive(Debug)]
pub struct CommonParams {
    /// Token creation parameter version.
    version: u8,
    /// Initial default admin granted after token initialization.
    initial_admin: Address,
}

/// Decoded creation parameters typed per token variant.
///
/// Each arm carries a typed `init` struct that maps 1-to-1 to its storage
/// `initialize()` call, plus the shared control-flow fields in `common`.
#[derive(Debug)]
pub enum TokenCreateParams {
    /// Stablecoin B-20 token creation parameters.
    Stablecoin {
        /// Shared control-flow fields.
        common: CommonParams,
        /// Stablecoin initialization fields.
        init: H20StablecoinInit,
    },
    /// Asset B-20 token creation parameters.
    Asset {
        /// Shared control-flow fields.
        common: CommonParams,
        /// Asset-token initialization fields.
        init: H20AssetInit,
    },
}

impl TokenCreateParams {
    /// Decodes ABI-encoded creation parameters for `variant`.
    pub fn decode(variant: H20Variant, params: &Bytes) -> Result<Self> {
        match variant {
            H20Variant::Stablecoin => {
                let p = IH20Factory::H20StablecoinCreateParams::abi_decode_validate(params)
                    .map_err(Self::invalid_params)?;
                Ok(Self::Stablecoin {
                    common: CommonParams { version: p.version, initial_admin: p.initialAdmin },
                    init: H20StablecoinInit {
                        name: p.name,
                        symbol: p.symbol,
                        supply_cap: H20FactoryStorage::DEFAULT_SUPPLY_CAP,
                        currency: p.currency,
                    },
                })
            }
            H20Variant::Asset => {
                let p = IH20Factory::H20AssetCreateParams::abi_decode_validate(params)
                    .map_err(Self::invalid_params)?;
                Ok(Self::Asset {
                    common: CommonParams { version: p.version, initial_admin: p.initialAdmin },
                    init: H20AssetInit {
                        name: p.name,
                        symbol: p.symbol,
                        supply_cap: H20FactoryStorage::DEFAULT_SUPPLY_CAP,
                        multiplier: INITIAL_MULTIPLIER,
                        decimals: p.decimals,
                    },
                })
            }
        }
    }

    /// Returns the shared token creation parameter version.
    pub const fn version(&self) -> u8 {
        match self {
            Self::Stablecoin { common, .. } | Self::Asset { common, .. } => common.version,
        }
    }

    /// Validates variant-specific invariants after the shared version check.
    ///
    /// Each arm owns its own rules. Version is checked first by the caller (`check_version`)
    /// so that version errors always take precedence over field-level errors.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Stablecoin { init, .. } => Self::validate_stablecoin(init),
            Self::Asset { init, .. } => Self::validate_asset(init),
        }
    }

    /// Validates stablecoin initialization fields.
    pub const fn validate_stablecoin(_init: &H20StablecoinInit) -> Result<()> {
        // Currency validation is delegated to `H20StablecoinStorage::initialize`, which rejects
        // empty values with `MissingRequiredField` and non-A-Z values with `InvalidCurrency`.
        Ok(())
    }

    /// Validates asset-token initialization fields.
    pub fn validate_asset(init: &H20AssetInit) -> Result<()> {
        if init.decimals < H20AssetStorage::MIN_DECIMALS
            || init.decimals > H20AssetStorage::MAX_DECIMALS
        {
            return Err(BasePrecompileError::revert(IH20Factory::InvalidDecimals {
                decimals: init.decimals,
            }));
        }
        Ok(())
    }

    /// Maps an ABI parameter decoding error into the factory error surface.
    pub fn invalid_params(error: impl core::fmt::Display) -> BasePrecompileError {
        BasePrecompileError::AbiDecodeFailed {
            selector: IH20Factory::createH20Call::SELECTOR,
            error: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use crate::H20Spec;
    use alloy_primitives::{Address, B256, U256, address};
    use alloy_sol_types::{SolCall, SolValue};
    use h20_precompile_storage::{Handler, HashMapStorageProvider, StorageCtx};

    use crate::{
        ActivationAdminConfig, ActivationFeature, ActivationRegistryStorage, Asset,
        AssetAccounting, AssetV1, H20_MAX_SUPPLY_CAP, H20AssetStorage, H20AssetToken,
        H20FactoryStorage, H20TokenRole, H20Variant, IH20, IH20Factory, PolicyRegistryStorage,
        PolicyVersion, Token, TokenAccounting,
    };

    const ACTIVATION_ADMIN: Address = address!("0xcb00000000000000000000000000000000000000");
    const ACTIVATION_ADMIN_CONFIG: ActivationAdminConfig =
        ActivationAdminConfig::static_fallback(Some(ACTIVATION_ADMIN));

    fn activate_precompiles(storage: &mut HashMapStorageProvider) {
        storage.set_caller(ACTIVATION_ADMIN);
        for key in [ActivationFeature::H20Stablecoin.id(), ActivationFeature::H20Asset.id()] {
            StorageCtx::enter(storage, |ctx| {
                ActivationRegistryStorage::new(ctx).activate(key, ACTIVATION_ADMIN_CONFIG).unwrap()
            });
        }
    }

    fn token_params(name: &str, symbol: &str) -> IH20Factory::H20AssetCreateParams {
        IH20Factory::H20AssetCreateParams {
            version: H20Variant::Asset.supported_version(),
            name: name.to_string(),
            symbol: symbol.to_string(),
            initialAdmin: Address::repeat_byte(0xAB),
            decimals: H20AssetStorage::MIN_DECIMALS,
        }
    }

    fn create_call(
        variant: IH20Factory::H20Variant,
        params: IH20Factory::H20AssetCreateParams,
        salt: B256,
    ) -> IH20Factory::createH20Call {
        IH20Factory::createH20Call {
            variant,
            salt,
            params: params.abi_encode().into(),
            initCalls: Vec::new(),
        }
    }

    fn h20_call(salt: B256) -> IH20Factory::createH20Call {
        create_call(IH20Factory::H20Variant::ASSET, token_params("Test", "TST"), salt)
    }

    fn token_at<'a>(
        addr: Address,
        ctx: StorageCtx<'a>,
    ) -> H20AssetToken<H20AssetStorage<'a>, PolicyRegistryStorage<'a>> {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(addr, ctx),
            PolicyRegistryStorage::new(ctx),
            PolicyVersion::V1,
        )
    }

    #[test]
    fn test_create_token_deploys_ef_stub() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xAA);
        let (expected_addr, _) = H20Variant::Asset.compute_address(caller, salt);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token = factory.create_h20(caller, h20_call(salt), H20Spec::Beryl).unwrap();

            assert_eq!(token, expected_addr);
            assert!(ctx.has_bytecode(expected_addr).unwrap());
        });
    }

    #[test]
    fn test_create_token_stores_metadata_and_decimals() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xBB);
        let call =
            create_call(IH20Factory::H20Variant::ASSET, token_params("My Token", "MYT"), salt);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call, H20Spec::Beryl).unwrap();
            let token = H20AssetStorage::from_address(token_addr, ctx);

            assert_eq!(token.h20.name.read().unwrap(), "My Token");
            assert_eq!(token.h20.symbol.read().unwrap(), "MYT");
            assert_eq!(AssetAccounting::decimals(&token).unwrap(), H20AssetStorage::MIN_DECIMALS);
            assert_eq!(H20FactoryStorage::DEFAULT_SUPPLY_CAP, H20_MAX_SUPPLY_CAP);
            assert_eq!(token.supply_cap().unwrap(), H20_MAX_SUPPLY_CAP);
        });
    }

    #[test]
    fn test_create_token_init_calls_can_mint_supply() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xCC);
        let recipient = Address::repeat_byte(0xCD);
        let supply = U256::from(5_000u64);
        let mut call =
            create_call(IH20Factory::H20Variant::ASSET, token_params("Supply Token", "SUP"), salt);
        call.initCalls.push(IH20::mintCall { to: recipient, amount: supply }.abi_encode().into());

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call, H20Spec::Beryl).unwrap();
            let token = H20AssetStorage::from_address(token_addr, ctx);

            assert_eq!(token.h20.total_supply.read().unwrap(), supply);
            assert_eq!(token.balance_of(recipient).unwrap(), supply);
        });
    }

    #[test]
    fn test_create_token_init_calls_use_factory_caller_and_restore_creator() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let creator = Address::repeat_byte(0x55);
        let spender = Address::repeat_byte(0x77);
        let salt = B256::repeat_byte(0xCE);
        let allowance = U256::from(123u64);
        let mut call =
            create_call(IH20Factory::H20Variant::ASSET, token_params("Caller Token", "CALL"), salt);
        call.initCalls.push(IH20::approveCall { spender, amount: allowance }.abi_encode().into());
        storage.set_caller(creator);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(creator, call, H20Spec::Beryl).unwrap();
            let token = H20AssetStorage::from_address(token_addr, ctx);

            assert_eq!(ctx.caller(), creator);
            assert_eq!(token.allowance(H20FactoryStorage::ADDRESS, spender).unwrap(), allowance);
            assert_eq!(token.allowance(creator, spender).unwrap(), U256::ZERO);
        });
    }

    #[test]
    fn test_create_token_reverts_if_salt_reused() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xEE);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            factory.create_h20(caller, h20_call(salt), H20Spec::Beryl).unwrap();
            let result = factory.create_h20(caller, h20_call(salt), H20Spec::Beryl);
            assert!(result.is_err());
        });
    }

    /// A prefunded token address (balance > 0, no code) must not block `create_h20`.
    /// The factory collision check rejects only accounts that already have code, so a
    /// prefunded address is a valid deployment target and `set_code` must succeed.
    #[test]
    fn test_create_token_at_prefunded_address_succeeds() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);

        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xF0);
        let (token_addr, _) = H20Variant::Asset.compute_address(caller, salt);

        storage.set_balance(token_addr, U256::from(1u64));

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            factory.create_h20(caller, h20_call(salt), H20Spec::Beryl).unwrap();
        });
    }

    #[test]
    fn test_create_token_reverts_for_invalid_version_and_variant() {
        let mut storage = HashMapStorageProvider::new(1);
        let caller = Address::repeat_byte(0x55);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);

            let mut bad_params = token_params("Bad Version", "BAD");
            bad_params.version = H20Variant::Asset.supported_version() + 1;
            let bad_version =
                create_call(IH20Factory::H20Variant::ASSET, bad_params, B256::repeat_byte(0x01));
            assert!(factory.create_h20(caller, bad_version, H20Spec::Beryl).is_err());

            let bad_variant = IH20Factory::createH20Call {
                variant: IH20Factory::H20Variant::__Invalid,
                salt: B256::repeat_byte(0x02),
                params: token_params("Bad Variant", "BAD").abi_encode().into(),
                initCalls: Vec::new(),
            };
            assert!(factory.create_h20(caller, bad_variant, H20Spec::Beryl).is_err());
        });
    }

    #[test]
    fn test_create_token_allows_empty_default_name_and_symbol() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0x05);
        let call = create_call(IH20Factory::H20Variant::ASSET, token_params("", ""), salt);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call, H20Spec::Beryl).unwrap();
            let token = H20AssetStorage::from_address(token_addr, ctx);

            assert_eq!(token.h20.name.read().unwrap(), "");
            assert_eq!(token.h20.symbol.read().unwrap(), "");
        });
    }

    #[test]
    fn test_post_create_calls_execute_against_token() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0xDD);
        let mut call = h20_call(salt);
        call.initCalls
            .push(IH20::updateNameCall { newName: "Configured".to_string() }.abi_encode().into());

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call, H20Spec::Beryl).unwrap();
            let token = H20AssetStorage::from_address(token_addr, ctx);

            assert_eq!(token.h20.name.read().unwrap(), "Configured");
        });
    }

    #[test]
    fn test_is_h20_and_variant_prefix_before_and_after_create() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let salt = B256::repeat_byte(0x11);
        let (addr, _) = H20Variant::Asset.compute_address(caller, salt);

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            assert!(factory.is_h20(addr).unwrap());

            let token = factory.create_h20(caller, h20_call(salt), H20Spec::Beryl).unwrap();
            assert!(factory.is_h20(token).unwrap());
            assert_eq!(H20Variant::from_address(token), Some(H20Variant::Asset));
        });
    }

    #[test]
    fn test_transfer_and_mint_lifecycle() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let params = token_params("Lifecycle", "LIFE");
            let token_addr = factory
                .create_h20(
                    Address::repeat_byte(0xCA),
                    create_call(IH20Factory::H20Variant::ASSET, params, B256::repeat_byte(0x12)),
                    H20Spec::Beryl,
                )
                .unwrap();

            let alice = Address::repeat_byte(0xCD);
            let bob = Address::repeat_byte(0xBB);
            let mut token = token_at(token_addr, ctx);

            AssetV1.mint(&mut token, alice, alice, U256::from(1_000u64), true).unwrap();
            AssetV1.transfer(&mut token, alice, bob, U256::from(300u64), false).unwrap();
            AssetV1.mint(&mut token, alice, alice, U256::from(200u64), true).unwrap();

            assert_eq!(token.accounting().balance_of(alice).unwrap(), U256::from(900u64));
            assert_eq!(token.accounting().balance_of(bob).unwrap(), U256::from(300u64));
            assert_eq!(token.accounting().total_supply().unwrap(), U256::from(1_200u64));
        });
    }

    #[test]
    fn test_token_identity_uses_dynamic_address() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let first = factory
                .create_h20(
                    Address::repeat_byte(0xCA),
                    h20_call(B256::repeat_byte(0x07)),
                    H20Spec::Beryl,
                )
                .unwrap();
            let second = factory
                .create_h20(
                    Address::repeat_byte(0xCA),
                    h20_call(B256::repeat_byte(0x08)),
                    H20Spec::Beryl,
                )
                .unwrap();

            assert_ne!(first, second);

            let first_token = token_at(first, ctx);
            let second_token = token_at(second, ctx);

            assert_eq!(first_token.token_address(), first);
            assert_eq!(second_token.token_address(), second);

            let (_, _, _, _, first_domain_address, _, _) =
                AssetV1.eip712_domain(&first_token, ctx.chain_id()).unwrap();
            let (_, _, _, _, second_domain_address, _, _) =
                AssetV1.eip712_domain(&second_token, ctx.chain_id()).unwrap();

            assert_eq!(first_domain_address, first);
            assert_eq!(second_domain_address, second);
            assert_ne!(
                AssetV1.domain_separator(&first_token, ctx.chain_id()).unwrap(),
                AssetV1.domain_separator(&second_token, ctx.chain_id()).unwrap()
            );
        });
    }

    #[test]
    fn test_create_asset_token_grants_default_admin_role() {
        let mut storage = HashMapStorageProvider::new(1);
        activate_precompiles(&mut storage);
        let caller = Address::repeat_byte(0x55);
        let initial_admin = Address::repeat_byte(0xAB);

        let params = IH20Factory::H20AssetCreateParams {
            version: H20Variant::Asset.supported_version(),
            name: "Asset Token".to_string(),
            symbol: "AST".to_string(),
            initialAdmin: initial_admin,
            decimals: 6,
        };
        let call = IH20Factory::createH20Call {
            variant: IH20Factory::H20Variant::ASSET,
            salt: B256::repeat_byte(0x50),
            params: params.abi_encode().into(),
            initCalls: Vec::new(),
        };

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call, H20Spec::Beryl).unwrap();

            let token = H20AssetToken::with_storage_and_policy(
                H20AssetStorage::from_address(token_addr, ctx),
                PolicyRegistryStorage::new(ctx),
                PolicyVersion::V1,
            );
            assert!(
                token
                    .accounting()
                    .has_role(H20TokenRole::DefaultAdmin.id(), initial_admin)
                    .unwrap()
            );
            assert!(
                !token
                    .accounting()
                    .has_role(H20TokenRole::DefaultAdmin.id(), Address::ZERO)
                    .unwrap()
            );
        });

        // Zero initialAdmin grants no role.
        let params_no_admin = IH20Factory::H20AssetCreateParams {
            version: H20Variant::Asset.supported_version(),
            name: "No Admin".to_string(),
            symbol: "NA".to_string(),
            initialAdmin: Address::ZERO,
            decimals: 6,
        };
        let call_no_admin = IH20Factory::createH20Call {
            variant: IH20Factory::H20Variant::ASSET,
            salt: B256::repeat_byte(0x51),
            params: params_no_admin.abi_encode().into(),
            initCalls: Vec::new(),
        };

        StorageCtx::enter(&mut storage, |ctx| {
            let mut factory = H20FactoryStorage::new(ctx);
            let token_addr = factory.create_h20(caller, call_no_admin, H20Spec::Beryl).unwrap();

            let token = H20AssetToken::with_storage_and_policy(
                H20AssetStorage::from_address(token_addr, ctx),
                PolicyRegistryStorage::new(ctx),
                PolicyVersion::V1,
            );
            assert!(
                !token
                    .accounting()
                    .has_role(H20TokenRole::DefaultAdmin.id(), initial_admin)
                    .unwrap()
            );
            assert!(
                !token
                    .accounting()
                    .has_role(H20TokenRole::DefaultAdmin.id(), Address::ZERO)
                    .unwrap()
            );
        });
    }
}
