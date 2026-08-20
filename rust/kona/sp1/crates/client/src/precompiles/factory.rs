//! [`EvmFactory`] implementation for the EVM in the ZKVM environment.

use super::OpZkvmPrecompiles;
use alloc::collections::BTreeMap;
use alloy_evm::{Database, EvmEnv, EvmFactory};
use alloy_op_evm::{
    OpEvm, OpEvmContext, OpTx, OpTxError,
    post_exec::{PostExecEvmFactoryHooks, PostExecExecutedTx, PostExecTxContext, WarmingState},
};
use hsk_h20_config::H20Config;
use op_revm::{L1BlockInfo, OpBuilder, OpHaltReason, OpSpecId, OpTransaction};
use revm::{
    Context, Inspector, MainContext,
    context::{BlockEnv, CfgEnv, DBErrorMarker, TxEnv, result::EVMError},
    inspector::NoOpInspector,
};

/// Factory producing [`OpEvm`]s with FPVM-accelerated precompile overrides enabled.
#[derive(Debug, Clone, Default)]
pub struct ZkvmOpEvmFactory {
    h20_configs: BTreeMap<u64, H20Config>,
}

impl ZkvmOpEvmFactory {
    /// Creates a ZKVM factory with H20 disabled unless a chain configuration is installed.
    pub const fn new() -> Self {
        Self { h20_configs: BTreeMap::new() }
    }

    /// Installs one chain's validated H20 consensus configuration.
    pub fn with_h20_config(mut self, chain_id: u64, config: H20Config) -> Self {
        self.h20_configs.insert(chain_id, config);
        self
    }

    /// Installs all chain configurations used by an interop proof.
    pub fn with_h20_configs(
        mut self,
        configs: impl IntoIterator<Item = (u64, H20Config)>,
    ) -> Self {
        self.h20_configs.extend(configs);
        self
    }

    fn h20_config(&self, chain_id: u64) -> H20Config {
        self.h20_configs.get(&chain_id).copied().unwrap_or(H20Config::DISABLED)
    }
}

impl PostExecEvmFactoryHooks for ZkvmOpEvmFactory {
    type Snapshot = WarmingState;

    fn begin_post_exec_tx<DB, I>(evm: &mut Self::Evm<DB, I>, ctx: PostExecTxContext)
    where
        DB: Database,
        I: Inspector<Self::Context<DB>>,
    {
        evm.begin_post_exec_tx(ctx);
    }

    fn take_last_post_exec_tx_result<DB, I>(evm: &mut Self::Evm<DB, I>) -> PostExecExecutedTx
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

impl EvmFactory for ZkvmOpEvmFactory {
    type Evm<DB: Database, I: Inspector<OpEvmContext<DB>>> = OpEvm<DB, I, OpZkvmPrecompiles, OpTx>;
    type Context<DB: Database> = OpEvmContext<DB>;
    type Tx = OpTx;
    type Error<DBError: DBErrorMarker> = EVMError<DBError, OpTxError>;
    type HaltReason = OpHaltReason;
    type Spec = OpSpecId;
    type Precompiles = OpZkvmPrecompiles;
    type BlockEnv = BlockEnv;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        input: EvmEnv<OpSpecId>,
    ) -> Self::Evm<DB, NoOpInspector> {
        let spec_id = *input.spec_id();
        let chain_id = input.cfg_env.chain_id;
        let timestamp = input.block_env.timestamp.saturating_to::<u64>();
        let revm_evm = Context::mainnet()
            .with_tx(OpTx(OpTransaction::<TxEnv>::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(NoOpInspector {})
            .with_precompiles(
                OpZkvmPrecompiles::new_with_spec(spec_id)
                    .with_h20(self.h20_config(chain_id), timestamp),
            );

        OpEvm::new(revm_evm, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<OpSpecId>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        let spec_id = *input.spec_id();
        let chain_id = input.cfg_env.chain_id;
        let timestamp = input.block_env.timestamp.saturating_to::<u64>();
        let revm_evm = Context::mainnet()
            .with_tx(OpTx(OpTransaction::<TxEnv>::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(inspector)
            .with_precompiles(
                OpZkvmPrecompiles::new_with_spec(spec_id)
                    .with_h20(self.h20_config(chain_id), timestamp),
            );

        OpEvm::new(revm_evm, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_evm::Evm;
    use alloy_op_evm::H20OpEvmFactory;
    use alloy_primitives::{Address, B256, Bytes, TxKind, U256, address};
    use alloy_sol_types::SolCall;
    use alloy_trie::{TrieAccount, root};
    use hsk_h20_precompiles::{
        ActivationFeature, ActivationRegistryStorage, H20FactoryStorage, H20Variant,
        IActivationRegistry, PolicyRegistryStorage,
    };
    use kona_protocol::OutputRoot;
    use revm::{
        context::{BlockEnv, CfgEnv, TxEnv},
        database::InMemoryDB,
        handler::PrecompileProvider,
        state::{AccountInfo, EvmState},
    };

    fn state_root(state: EvmState) -> B256 {
        let accounts = state.into_iter().filter_map(|(address, account)| {
            if !account.is_touched() || account.is_selfdestructed() {
                return None;
            }
            let storage_root = root::storage_root_unhashed(account.storage.into_iter().map(
                |(slot, value)| (B256::from(slot.to_be_bytes()), value.present_value),
            ));
            Some((
                address,
                TrieAccount {
                    nonce: account.info.nonce,
                    balance: account.info.balance,
                    storage_root,
                    code_hash: account.info.code_hash,
                },
            ))
        });
        root::state_root_unhashed(accounts)
    }

    #[test]
    fn zkvm_and_op_reth_h20_execution_produce_the_same_state_root() {
        let admin = Address::repeat_byte(0x11);
        let config = H20Config::new(Some(100), Some(admin)).unwrap();
        let chain_id = 133;
        let env = EvmEnv::new(
            CfgEnv::new_with_spec(OpSpecId::JOVIAN).with_chain_id(chain_id),
            BlockEnv {
                timestamp: U256::from(100),
                gas_limit: 30_000_000,
                ..Default::default()
            },
        );
        let calldata = IActivationRegistry::activateCall {
            feature: ActivationFeature::H20Asset.id(),
        }
        .abi_encode();
        let tx = OpTx(
            OpTransaction::builder()
                .base(
                    TxEnv::builder()
                        .caller(admin)
                        .chain_id(Some(chain_id))
                        .kind(TxKind::Call(ActivationRegistryStorage::ADDRESS))
                        .data(Bytes::from(calldata))
                        .gas_limit(1_000_000)
                        .gas_price(0),
                )
                .enveloped_tx(Some(Bytes::new()))
                .build_fill(),
        );
        let database = || {
            let mut db = InMemoryDB::default();
            db.insert_account_info(
                admin,
                AccountInfo { balance: U256::MAX, ..Default::default() },
            );
            db
        };

        let mut op_reth =
            H20OpEvmFactory::<OpTx>::new(config).create_evm(database(), env.clone());
        let op_reth_result =
            op_reth.transact_raw(tx.clone()).expect("op-reth H20 activation succeeds");

        let mut zkvm = ZkvmOpEvmFactory::new()
            .with_h20_config(chain_id, config)
            .create_evm(database(), env);

        let dynamic = H20Variant::Asset.compute_address(admin, [0x22; 32].into()).0;
        assert_eq!(dynamic, address!("0177000000000000000000f4f69ba108f6504dc5"));
        for address in [
            H20FactoryStorage::ADDRESS,
            ActivationRegistryStorage::ADDRESS,
            PolicyRegistryStorage::ADDRESS,
            dynamic,
        ] {
            assert!(op_reth.precompiles().get(&address).is_some());
            assert!(<OpZkvmPrecompiles as PrecompileProvider<OpEvmContext<InMemoryDB>>>::contains(
                zkvm.precompiles(),
                &address,
            ));
        }

        let zkvm_result = zkvm.transact_raw(tx).expect("ZKVM H20 activation succeeds");

        assert_eq!(op_reth_result.result, zkvm_result.result);
        assert_eq!(op_reth_result.state, zkvm_result.state);
        let op_reth_state_root = state_root(op_reth_result.state);
        let zkvm_state_root = state_root(zkvm_result.state);
        assert_eq!(op_reth_state_root, zkvm_state_root);
        assert_ne!(op_reth_state_root, alloy_trie::EMPTY_ROOT_HASH);

        let bridge_storage_root = B256::repeat_byte(0x22);
        let block_hash = B256::repeat_byte(0x33);
        assert_eq!(
            OutputRoot::from_parts(op_reth_state_root, bridge_storage_root, block_hash).hash(),
            OutputRoot::from_parts(zkvm_state_root, bridge_storage_root, block_hash).hash(),
        );
    }
}
