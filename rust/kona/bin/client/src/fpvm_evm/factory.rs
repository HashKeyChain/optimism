//! [`EvmFactory`] implementation for the EVM in the FPVM environment.

use super::{precompiles::OpFpvmPrecompiles, tx::FpvmOpTx};
use alloc::collections::BTreeMap;
use alloy_evm::{Database, EvmEnv, EvmFactory};
use alloy_op_evm::{
    OpEvm, OpEvmContext, OpTx, OpTxError,
    post_exec::{PostExecEvmFactoryHooks, PostExecExecutedTx, PostExecTxContext, WarmingState},
};
use hsk_h20_config::H20Config;
use kona_preimage::{HintWriterClient, PreimageOracleClient};
use op_revm::{L1BlockInfo, OpBuilder, OpHaltReason, OpSpecId, OpTransaction};
use revm::{
    Context, Inspector, MainContext,
    context::{BlockEnv, CfgEnv, DBErrorMarker, result::EVMError},
    inspector::NoOpInspector,
};

/// Factory producing [`OpEvm`]s with FPVM-accelerated precompile overrides enabled.
#[derive(Debug, Clone)]
pub struct FpvmOpEvmFactory<H, O> {
    /// The hint writer.
    hint_writer: H,
    /// The oracle reader.
    oracle_reader: O,
    /// Per-L2-chain H20 consensus configurations loaded from proof boot information.
    h20_configs: BTreeMap<u64, H20Config>,
}

impl<H, O> FpvmOpEvmFactory<H, O>
where
    H: HintWriterClient + Clone + Send + Sync,
    O: PreimageOracleClient + Clone + Send + Sync,
{
    /// Creates a new [`FpvmOpEvmFactory`].
    pub fn new(hint_writer: H, oracle_reader: O) -> Self {
        Self { hint_writer, oracle_reader, h20_configs: BTreeMap::new() }
    }

    /// Installs the validated H20 configuration for an L2 chain ID.
    pub fn with_h20_config(mut self, chain_id: u64, config: H20Config) -> Self {
        self.h20_configs.insert(chain_id, config);
        self
    }

    /// Installs all validated H20 configurations used by an interop proof.
    pub fn with_h20_configs(mut self, configs: impl IntoIterator<Item = (u64, H20Config)>) -> Self {
        self.h20_configs.extend(configs);
        self
    }

    fn h20_config(&self, chain_id: u64) -> H20Config {
        self.h20_configs.get(&chain_id).copied().unwrap_or(H20Config::DISABLED)
    }

    /// Returns a reference to the inner [`HintWriterClient`].
    pub fn hint_writer(&self) -> &H {
        &self.hint_writer
    }

    /// Returns a reference to the inner [`PreimageOracleClient`].
    pub fn oracle_reader(&self) -> &O {
        &self.oracle_reader
    }
}

impl<H, O> PostExecEvmFactoryHooks for FpvmOpEvmFactory<H, O>
where
    H: HintWriterClient + Clone + Send + Sync + 'static,
    O: PreimageOracleClient + Clone + Send + Sync + 'static,
{
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

impl<H, O> EvmFactory for FpvmOpEvmFactory<H, O>
where
    H: HintWriterClient + Clone + Send + Sync + 'static,
    O: PreimageOracleClient + Clone + Send + Sync + 'static,
{
    type Evm<DB: Database, I: Inspector<OpEvmContext<DB>>> =
        OpEvm<DB, I, OpFpvmPrecompiles<H, O>, FpvmOpTx>;
    type Context<DB: Database> = OpEvmContext<DB>;
    type Tx = FpvmOpTx;
    type Error<DBError: DBErrorMarker> = EVMError<DBError, OpTxError>;
    type HaltReason = OpHaltReason;
    type Spec = OpSpecId;
    type Precompiles = OpFpvmPrecompiles<H, O>;
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
            .with_tx(OpTx(OpTransaction::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(NoOpInspector {})
            .with_precompiles(
                OpFpvmPrecompiles::new_with_spec(
                    spec_id,
                    self.hint_writer.clone(),
                    self.oracle_reader.clone(),
                )
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
            .with_tx(OpTx(OpTransaction::builder().build_fill()))
            .with_cfg(CfgEnv::new_with_spec(OpSpecId::BEDROCK))
            .with_chain(L1BlockInfo::default())
            .with_db(db)
            .with_block(input.block_env)
            .with_cfg(input.cfg_env)
            .build_op_with_inspector(inspector)
            .with_precompiles(
                OpFpvmPrecompiles::new_with_spec(
                    spec_id,
                    self.hint_writer.clone(),
                    self.oracle_reader.clone(),
                )
                .with_h20(self.h20_config(chain_id), timestamp),
            );

        OpEvm::new(revm_evm, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_evm::Evm;
    use alloy_op_evm::{H20OpEvmFactory, OpEvmContext, post_exec::PostExecEvmFactoryAdapter};
    use alloy_consensus::{Header, Sealable};
    use alloy_eips::Encodable2718;
    use alloy_primitives::{Address, B256, Bytes, TxKind, U256, address};
    use alloy_rpc_types_engine::PayloadAttributes;
    use alloy_sol_types::SolCall;
    use alloy_trie::{TrieAccount, root};
    use hsk_h20_precompiles::{
        ActivationFeature, ActivationRegistryStorage, H20FactoryStorage, H20Variant,
        IActivationRegistry, PolicyRegistryStorage,
    };
    use kona_preimage::{BidirectionalChannel, HintWriter, OracleReader};
    use kona_executor::{NoopTrieDBProvider, StatelessL2Builder};
    use kona_genesis::RollupConfig;
    use kona_mpt::NoopTrieHinter;
    use kona_protocol::OutputRoot;
    use op_alloy_consensus::{OpTxEnvelope, TxDeposit};
    use op_alloy_rpc_types_engine::OpPayloadAttributes;
    use revm::{
        context::{BlockEnv, CfgEnv, TxEnv},
        database::InMemoryDB,
        handler::PrecompileProvider,
        state::{AccountInfo, EvmState},
    };

    fn state_root(state: EvmState) -> B256 {
        let accounts =
            state.into_iter().filter_map(|(address, account)| {
                if !account.is_touched() || account.is_selfdestructed() {
                    return None;
                }
                let storage_root =
                    root::storage_root_unhashed(account.storage.into_iter().map(
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
    fn fpvm_and_op_reth_h20_execution_produce_the_same_state_root() {
        let admin = Address::repeat_byte(0x11);
        let config = H20Config::new(Some(100), Some(admin)).unwrap();
        let chain_id = 133;
        let env = EvmEnv::new(
            CfgEnv::new_with_spec(OpSpecId::JOVIAN).with_chain_id(chain_id),
            BlockEnv { timestamp: U256::from(100), gas_limit: 30_000_000, ..Default::default() },
        );
        let calldata =
            IActivationRegistry::activateCall { feature: ActivationFeature::H20Asset.id() }
                .abi_encode();
        let base_tx = TxEnv::builder()
            .caller(admin)
            .chain_id(Some(chain_id))
            .kind(TxKind::Call(ActivationRegistryStorage::ADDRESS))
            .data(Bytes::from(calldata))
            .gas_limit(1_000_000)
            .gas_price(0);
        let op_transaction =
            OpTransaction::builder().base(base_tx).enveloped_tx(Some(Bytes::new())).build_fill();

        let database = || {
            let mut db = InMemoryDB::default();
            db.insert_account_info(admin, AccountInfo { balance: U256::MAX, ..Default::default() });
            db
        };

        let mut op_reth = H20OpEvmFactory::<OpTx>::new(config).create_evm(database(), env.clone());
        let op_reth_result = op_reth
            .transact_raw(OpTx(op_transaction.clone()))
            .expect("op-reth H20 activation succeeds");

        let (hint_chan, preimage_chan) =
            (BidirectionalChannel::new().unwrap(), BidirectionalChannel::new().unwrap());
        let mut fpvm = FpvmOpEvmFactory::new(
            HintWriter::new(hint_chan.client),
            OracleReader::new(preimage_chan.client),
        )
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
            assert!(<OpFpvmPrecompiles<_, _> as PrecompileProvider<OpEvmContext<InMemoryDB>>>::contains(
                fpvm.precompiles(),
                &address,
            ));
        }

        let fpvm_result =
            fpvm.transact_raw(FpvmOpTx(op_transaction)).expect("FPVM H20 activation succeeds");

        assert_eq!(op_reth_result.result, fpvm_result.result);
        assert_eq!(op_reth_result.state, fpvm_result.state);
        let op_reth_state_root = state_root(op_reth_result.state);
        let fpvm_state_root = state_root(fpvm_result.state);
        assert_eq!(op_reth_state_root, fpvm_state_root);
        assert_ne!(op_reth_state_root, alloy_trie::EMPTY_ROOT_HASH);

        let bridge_storage_root = B256::repeat_byte(0x22);
        let block_hash = B256::repeat_byte(0x33);
        assert_eq!(
            OutputRoot::from_parts(op_reth_state_root, bridge_storage_root, block_hash).hash(),
            OutputRoot::from_parts(fpvm_state_root, bridge_storage_root, block_hash).hash(),
        );
    }

    #[test]
    fn fpvm_and_op_reth_build_the_same_h20_block_state_and_output_root() {
        let admin = Address::repeat_byte(0x11);
        let config = H20Config::new(Some(100), Some(admin)).unwrap();
        let rollup = RollupConfig {
            block_time: 2,
            l2_chain_id: 133.into(),
            h20_time: Some(100),
            h20_activation_admin: Some(admin),
            ..Default::default()
        };
        let calldata = IActivationRegistry::activateCall {
            feature: ActivationFeature::H20Asset.id(),
        }
        .abi_encode();
        let deposit = TxDeposit {
            source_hash: B256::repeat_byte(0x44),
            from: admin,
            to: TxKind::Call(ActivationRegistryStorage::ADDRESS),
            mint: 1_000_000_000_000_000_000,
            value: U256::ZERO,
            gas_limit: 1_000_000,
            is_system_transaction: false,
            input: Bytes::from(calldata),
        };
        let envelope: OpTxEnvelope = deposit.into();
        let attributes = OpPayloadAttributes {
            payload_attributes: PayloadAttributes {
                timestamp: 100,
                suggested_fee_recipient: Address::ZERO,
                prev_randao: B256::ZERO,
                ..Default::default()
            },
            gas_limit: Some(30_000_000),
            transactions: Some(vec![envelope.encoded_2718().into()]),
            ..Default::default()
        };
        let parent = Header {
            number: 0,
            timestamp: 98,
            gas_limit: 30_000_000,
            state_root: alloy_trie::EMPTY_ROOT_HASH,
            base_fee_per_gas: Some(1_000_000_000),
            ..Default::default()
        }
        .seal_slow();

        let mut op_reth = StatelessL2Builder::new(
            &rollup,
            H20OpEvmFactory::<OpTx>::new(config),
            alloy_op_evm::block::OpAlloyReceiptBuilder::default(),
            NoopTrieDBProvider,
            NoopTrieHinter,
            parent.clone(),
        );
        let op_reth_outcome =
            op_reth.build_block(attributes.clone()).expect("op-reth-style block builds");

        let (hint_chan, preimage_chan) = (
            BidirectionalChannel::new().unwrap(),
            BidirectionalChannel::new().unwrap(),
        );
        let mut fpvm = StatelessL2Builder::new(
            &rollup,
            PostExecEvmFactoryAdapter::new(
                FpvmOpEvmFactory::new(
                    HintWriter::new(hint_chan.client),
                    OracleReader::new(preimage_chan.client),
                )
                .with_h20_config(133, config),
            ),
            alloy_op_evm::block::OpAlloyReceiptBuilder::default(),
            NoopTrieDBProvider,
            NoopTrieHinter,
            parent,
        );
        let fpvm_outcome = fpvm.build_block(attributes).expect("FPVM H20 block builds");

        assert_eq!(op_reth_outcome.header, fpvm_outcome.header);
        assert_eq!(op_reth_outcome.execution_result, fpvm_outcome.execution_result);
        assert_ne!(op_reth_outcome.header.state_root, alloy_trie::EMPTY_ROOT_HASH);
        let bridge_storage_root = alloy_trie::EMPTY_ROOT_HASH;
        assert_eq!(
            OutputRoot::from_parts(
                op_reth_outcome.header.state_root,
                bridge_storage_root,
                op_reth_outcome.header.hash(),
            )
            .hash(),
            OutputRoot::from_parts(
                fpvm_outcome.header.state_root,
                bridge_storage_root,
                fpvm_outcome.header.hash(),
            )
            .hash(),
        );
    }
}
