//! HSK H20 precompile provider and OP EVM factory.

use alloy_evm::{Database, EvmEnv, EvmFactory, IntoTxEnv, precompiles::PrecompilesMap};
use alloy_primitives::{Address, map::AddressSet};
use core::{fmt::Debug, marker::PhantomData};
use hsk_h20_config::H20Config;
use hsk_h20_precompiles::{
    ActivationRegistry, BerylLookup, H20Factory, H20Spec, NoopPrecompileCallObserver,
    PolicyRegistryPrecompile,
};
use op_revm::{
    L1BlockInfo, OpBuilder, OpHaltReason, OpSpecId, OpTransaction, precompiles::OpPrecompiles,
};
use revm::{
    Context, Inspector, MainContext,
    context::{BlockEnv, CfgEnv, DBErrorMarker, TxEnv},
    context_interface::result::EVMError,
    handler::PrecompileProvider,
    inspector::NoOpInspector,
    interpreter::{CallInputs, InterpreterResult},
};

use crate::{OpEvm, OpEvmContext, OpTx, OpTxError, post_exec};

/// OP precompile provider extended with HSK H20 v1.
#[derive(Debug)]
pub struct H20OpPrecompiles {
    /// Canonical OP precompile provider being wrapped.
    op: OpPrecompiles,
    /// OP EVM spec used by the wrapped provider.
    spec: OpSpecId,
    /// Installed OP and optional H20 precompile map.
    installed: PrecompilesMap,
    /// Static consensus configuration.
    config: H20Config,
    /// Timestamp used to select H20 activation for this EVM.
    timestamp: u64,
    /// Statically installed precompile addresses to warm at transaction start.
    warm_addresses: AddressSet,
}

impl H20OpPrecompiles {
    /// Creates an OP provider and installs H20 when active at `timestamp`.
    pub fn new(spec: OpSpecId, config: H20Config, timestamp: u64) -> Self {
        let op = OpPrecompiles::new_with_spec(spec);
        let installed = Self::install(op.precompiles(), config, timestamp);
        let warm_addresses = installed.addresses().copied().collect();
        Self { op, spec, installed, config, timestamp, warm_addresses }
    }

    fn install(
        op_precompiles: &'static revm::precompile::Precompiles,
        config: H20Config,
        timestamp: u64,
    ) -> PrecompilesMap {
        let mut installed = PrecompilesMap::from_static(op_precompiles);
        if config.is_active_at(timestamp) {
            H20Factory::install_with_observer(
                &mut installed,
                H20Spec::Beryl,
                NoopPrecompileCallObserver,
            );
            PolicyRegistryPrecompile::install(&mut installed, H20Spec::Beryl);
            ActivationRegistry::install(&mut installed, config.activation_admin());
            BerylLookup::install(&mut installed);
        }
        installed
    }

    /// Returns the wrapped canonical OP provider.
    pub const fn op(&self) -> &OpPrecompiles {
        &self.op
    }

    /// Returns the H20 consensus configuration.
    pub const fn config(&self) -> H20Config {
        self.config
    }

    /// Returns the timestamp used for H20 activation selection.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Consumes this wrapper and returns the canonical map expected by reth's EVM interfaces.
    pub fn into_map(self) -> PrecompilesMap {
        self.installed
    }

    /// Builds the canonical OP+H20 precompile map for an EVM environment.
    pub fn build(spec: OpSpecId, config: H20Config, timestamp: u64) -> PrecompilesMap {
        Self::new(spec, config, timestamp).into_map()
    }
}

impl Clone for H20OpPrecompiles {
    fn clone(&self) -> Self {
        Self::new(self.spec, self.config, self.timestamp)
    }
}

impl<DB> PrecompileProvider<OpEvmContext<DB>> for H20OpPrecompiles
where
    DB: Database,
{
    type Output = InterpreterResult;

    fn set_spec(&mut self, spec: OpSpecId) -> bool {
        if <OpPrecompiles as PrecompileProvider<OpEvmContext<DB>>>::set_spec(&mut self.op, spec) {
            self.spec = spec;
            self.installed = Self::install(self.op.precompiles(), self.config, self.timestamp);
            self.warm_addresses = self.installed.addresses().copied().collect();
            true
        } else {
            false
        }
    }

    fn run(
        &mut self,
        context: &mut OpEvmContext<DB>,
        inputs: &CallInputs,
    ) -> Result<Option<Self::Output>, alloc::string::String> {
        <PrecompilesMap as PrecompileProvider<OpEvmContext<DB>>>::run(
            &mut self.installed,
            context,
            inputs,
        )
    }

    fn warm_addresses(&self) -> &AddressSet {
        &self.warm_addresses
    }

    fn contains(&self, address: &Address) -> bool {
        self.installed.get(address).is_some()
    }
}

/// EVM factory that selects H20 precompiles from the block timestamp.
#[derive(Debug, Clone, Copy)]
pub struct H20OpEvmFactory<Tx = OpTx> {
    config: H20Config,
    _tx: PhantomData<Tx>,
}

impl<Tx> H20OpEvmFactory<Tx> {
    /// Creates a H20-aware OP EVM factory.
    pub const fn new(config: H20Config) -> Self {
        Self { config, _tx: PhantomData }
    }

    /// Returns the H20 consensus configuration.
    pub const fn config(&self) -> H20Config {
        self.config
    }
}

impl<Tx> Default for H20OpEvmFactory<Tx> {
    fn default() -> Self {
        Self::new(H20Config::DISABLED)
    }
}

impl<Tx> EvmFactory for H20OpEvmFactory<Tx>
where
    Tx: IntoTxEnv<Tx> + Into<OpTransaction<TxEnv>> + Default + Clone + Debug,
{
    type Evm<DB: Database, I: Inspector<OpEvmContext<DB>>> = OpEvm<DB, I, Self::Precompiles, Tx>;
    type Context<DB: Database> = OpEvmContext<DB>;
    type Tx = Tx;
    type Error<DBError: DBErrorMarker> = EVMError<DBError, OpTxError>;
    type HaltReason = OpHaltReason;
    type Spec = OpSpecId;
    type BlockEnv = BlockEnv;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        input: EvmEnv<OpSpecId, BlockEnv>,
    ) -> Self::Evm<DB, NoOpInspector> {
        let spec = input.cfg_env.spec;
        let timestamp = input.block_env.timestamp.saturating_to::<u64>();
        let inner = Context::mainnet()
            .with_tx(OpTx(OpTransaction::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(NoOpInspector {})
            .with_precompiles(H20OpPrecompiles::build(spec, self.config, timestamp));

        OpEvm::new(inner, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<OpSpecId, BlockEnv>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        let spec = input.cfg_env.spec;
        let timestamp = input.block_env.timestamp.saturating_to::<u64>();
        let inner = Context::mainnet()
            .with_tx(OpTx(OpTransaction::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(inspector)
            .with_precompiles(H20OpPrecompiles::build(spec, self.config, timestamp));

        OpEvm::new(inner, true)
    }
}

impl<Tx> post_exec::PostExecEvmFactoryHooks for H20OpEvmFactory<Tx>
where
    Tx: IntoTxEnv<Tx> + Into<OpTransaction<TxEnv>> + Default + Clone + Debug,
{
    type Snapshot = post_exec::WarmingState;

    fn begin_post_exec_tx<DB, I>(evm: &mut Self::Evm<DB, I>, ctx: post_exec::PostExecTxContext)
    where
        DB: Database,
        I: Inspector<Self::Context<DB>>,
    {
        evm.begin_post_exec_tx(ctx);
    }

    fn take_last_post_exec_tx_result<DB, I>(
        evm: &mut Self::Evm<DB, I>,
    ) -> post_exec::PostExecExecutedTx
    where
        DB: Database,
        I: Inspector<Self::Context<DB>>,
    {
        evm.take_last_post_exec_tx_result()
    }

    fn refund_snapshot<DB, I>(evm: &Self::Evm<DB, I>) -> Self::Snapshot
    where
        DB: Database,
        I: Inspector<Self::Context<DB>>,
    {
        evm.refund_snapshot()
    }

    fn seed_refund_snapshot<DB, I>(evm: &mut Self::Evm<DB, I>, state: Self::Snapshot)
    where
        DB: Database,
        I: Inspector<Self::Context<DB>>,
    {
        evm.seed_refund_snapshot(state);
    }
}

#[cfg(test)]
mod tests {
    use alloy_evm::{Evm, EvmEnv, EvmFactory};
    use alloy_primitives::{Address, U256, address};
    use hsk_h20_precompiles::{
        ActivationRegistryStorage, H20FactoryStorage, H20Variant, PolicyRegistryStorage,
    };
    use op_revm::OpSpecId;
    use revm::{
        context::{BlockEnv, CfgEnv},
        database::EmptyDB,
        handler::PrecompileProvider,
        inspector::NoOpInspector,
    };

    use super::{H20Config, H20OpEvmFactory, H20OpPrecompiles};
    use crate::{OpEvmContext, OpTx};

    const ADMIN: Address = Address::new([0x11; 20]);

    fn config() -> H20Config {
        H20Config::new(Some(100), Some(ADMIN)).unwrap()
    }

    #[test]
    fn provider_does_not_match_h20_before_activation() {
        let provider = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 99);
        assert!(!<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &provider,
            &H20FactoryStorage::ADDRESS,
        ));
    }

    #[test]
    fn provider_installs_singletons_at_activation_timestamp() {
        let provider = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 100);
        for address in [
            H20FactoryStorage::ADDRESS,
            PolicyRegistryStorage::ADDRESS,
            ActivationRegistryStorage::ADDRESS,
        ] {
            assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
                &provider, &address,
            ));
        }
    }

    #[test]
    fn provider_dynamically_matches_h20_addresses_only_after_activation() {
        let asset = H20Variant::Asset.compute_address(ADMIN, [0x22; 32].into()).0;
        let stablecoin = H20Variant::Stablecoin.compute_address(ADMIN, [0x33; 32].into()).0;
        let before = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 99);
        let after = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 100);

        assert!(!<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &before, &asset,
        ));
        assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &after, &asset,
        ));
        assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &after,
            &stablecoin,
        ));
        assert!(!<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &after,
            &Address::repeat_byte(0x22),
        ));
    }

    #[test]
    fn provider_matches_frozen_h20_address_vectors() {
        let salt = [0x22; 32].into();
        let asset = H20Variant::Asset.compute_address(ADMIN, salt).0;
        let stablecoin = H20Variant::Stablecoin.compute_address(ADMIN, salt).0;

        assert_eq!(asset, address!("0177000000000000000000f4f69ba108f6504dc5"));
        assert_eq!(stablecoin, address!("0177000000000000000001f4f69ba108f6504dc5"));

        let provider = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 100);
        for address in [asset, stablecoin] {
            assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
                &provider, &address,
            ));
        }
    }

    #[test]
    fn provider_preserves_op_precompiles_and_keeps_dynamic_addresses_cold() {
        let dynamic = H20Variant::Asset.compute_address(ADMIN, [0x44; 32].into()).0;
        let provider = H20OpPrecompiles::new(OpSpecId::JOVIAN, config(), 100);
        let eth_ecrecover =
            Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

        assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &provider,
            &eth_ecrecover,
        ));
        assert!(<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::contains(
            &provider, &dynamic,
        ));
        assert!(
            !<H20OpPrecompiles as PrecompileProvider<OpEvmContext<EmptyDB>>>::warm_addresses(
                &provider,
            )
            .contains(&dynamic)
        );
    }

    #[test]
    fn evm_factory_selects_provider_at_inclusive_fork_boundary() {
        let factory = H20OpEvmFactory::<OpTx>::new(config());
        let create = |timestamp| {
            factory.create_evm(
                EmptyDB::default(),
                EvmEnv::new(
                    CfgEnv::new_with_spec(OpSpecId::JOVIAN),
                    BlockEnv { timestamp: U256::from(timestamp), ..Default::default() },
                ),
            )
        };

        let before = create(99);
        let active = create(100);
        assert!(before.precompiles().get(&H20FactoryStorage::ADDRESS).is_none());
        assert!(active.precompiles().get(&H20FactoryStorage::ADDRESS).is_some());
    }

    #[test]
    fn inspected_evm_uses_the_same_h20_map() {
        let factory = H20OpEvmFactory::<OpTx>::new(config());
        let dynamic = H20Variant::Stablecoin.compute_address(ADMIN, [0x55; 32].into()).0;
        let evm = factory.create_evm_with_inspector(
            EmptyDB::default(),
            EvmEnv::new(
                CfgEnv::new_with_spec(OpSpecId::JOVIAN),
                BlockEnv { timestamp: U256::from(100), ..Default::default() },
            ),
            NoOpInspector {},
        );

        assert!(evm.precompiles().get(&H20FactoryStorage::ADDRESS).is_some());
        assert!(evm.precompiles().get(&dynamic).is_some());
    }
}
