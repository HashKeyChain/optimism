//! EVM config for vanilla optimism.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/reth/main/assets/reth-docs.png",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256",
    issue_tracker_base_url = "https://github.com/paradigmxyz/reth/issues/"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

extern crate alloc;

use alloc::sync::Arc;
use alloy_consensus::{BlockHeader, Header};
use alloy_evm::{
    EvmFactory, FromRecoveredTx, FromTxWithEncoded, block::BlockExecutorFactory,
    precompiles::PrecompilesMap,
};
use alloy_op_evm::{
    block::{OpTxEnv, receipt_builder::OpReceiptBuilder},
    evm_env_for_op_block, evm_env_for_op_next_block,
};
use core::fmt::Debug;
use op_alloy_consensus::{
    EIP1559ParamError, OpTransaction as OpConsensusTransaction,
    parse_post_exec_payload_from_transactions,
};
use op_revm::OpSpecId;
use reth_chainspec::EthChainSpec;
use reth_evm::{ConfigureEvm, EvmEnv, eth::NextEvmEnvAttributes};
use reth_optimism_chainspec::{B20ChainSpec, OpChainSpec};
use reth_optimism_forks::OpHardforks;
use reth_optimism_primitives::{DepositReceipt, OpPrimitives};
use reth_primitives_traits::{NodePrimitives, SealedBlock, SealedHeader, SignedTransaction};
use revm::context::BlockEnv;

#[allow(unused_imports)]
use {
    alloy_eips::Decodable2718,
    alloy_primitives::{Bytes, U256},
    op_alloy_rpc_types_engine::OpExecutionData,
    reth_evm::{EvmEnvFor, ExecutionCtxFor},
    reth_primitives_traits::{TxTy, WithEncoded},
    reth_storage_errors::any::AnyError,
    revm::{
        context::CfgEnv, context_interface::block::BlobExcessGasAndPrice,
        primitives::hardfork::SpecId,
    },
};

#[cfg(feature = "std")]
use reth_evm::{ConfigureEngineEvm, ExecutableTxIterator};

mod config;
pub use config::{OpNextBlockEnvAttributes, revm_spec, revm_spec_by_timestamp_after_bedrock};
mod execute;
pub use execute::*;
pub mod l1;
pub use l1::*;
mod receipts;
pub use receipts::*;
mod build;
pub use build::OpBlockAssembler;

mod error;
pub use error::{L1BlockInfoError, OpBlockExecutionError};

pub mod tx;
pub use tx::OpTx;

pub use alloy_op_evm::{
    B20OpEvmFactory, B20OpPrecompiles, OpBlockExecutionCtx, OpBlockExecutorFactory, OpEvm,
    OpEvmFactory, PostExecMode, PreRefundGasUsed,
    post_exec::{PostExecExecutorExt, WarmingRefundEvent, WarmingRefundKind, WarmingState},
};

mod post_exec_ext;
pub use post_exec_ext::*;

/// Optimism-related EVM configuration.
#[derive(Debug)]
pub struct OpEvmConfig<
    ChainSpec = OpChainSpec,
    N: NodePrimitives = OpPrimitives,
    R = OpRethReceiptBuilder,
    EvmFactory = B20OpEvmFactory<OpTx>,
> {
    /// Inner [`OpBlockExecutorFactory`].
    pub executor_factory: OpBlockExecutorFactory<R, Arc<ChainSpec>, EvmFactory>,
    /// Optimism block assembler.
    pub block_assembler: OpBlockAssembler<ChainSpec>,
    #[doc(hidden)]
    pub _pd: core::marker::PhantomData<N>,
}

impl<ChainSpec, N: NodePrimitives, R: Clone, EvmFactory: Clone> Clone
    for OpEvmConfig<ChainSpec, N, R, EvmFactory>
{
    fn clone(&self) -> Self {
        Self {
            executor_factory: self.executor_factory.clone(),
            block_assembler: self.block_assembler.clone(),
            _pd: self._pd,
        }
    }
}

impl<ChainSpec: EthChainSpec<Header = Header> + OpHardforks + B20ChainSpec> OpEvmConfig<ChainSpec> {
    /// Creates a new [`OpEvmConfig`] with the given chain spec for OP chains.
    pub fn optimism(chain_spec: Arc<ChainSpec>) -> Self {
        Self::new(chain_spec, OpRethReceiptBuilder::default())
    }
}

impl<ChainSpec, N: NodePrimitives, R, EvmFactory> OpEvmConfig<ChainSpec, N, R, EvmFactory> {
    /// Creates a new [`OpEvmConfig`] with an explicit EVM factory.
    pub fn new_with_evm_factory(
        chain_spec: Arc<ChainSpec>,
        receipt_builder: R,
        evm_factory: EvmFactory,
    ) -> Self {
        Self {
            block_assembler: OpBlockAssembler::new(chain_spec.clone()),
            executor_factory: OpBlockExecutorFactory::new(receipt_builder, chain_spec, evm_factory),
            _pd: core::marker::PhantomData,
        }
    }
}

impl<ChainSpec: EthChainSpec<Header = Header> + OpHardforks + B20ChainSpec, N: NodePrimitives, R>
    OpEvmConfig<ChainSpec, N, R>
{
    /// Creates a new [`OpEvmConfig`] with the given chain spec.
    pub fn new(chain_spec: Arc<ChainSpec>, receipt_builder: R) -> Self {
        let b20_config = chain_spec.b20_config();
        Self::new_with_evm_factory(
            chain_spec,
            receipt_builder,
            B20OpEvmFactory::<OpTx>::new(b20_config),
        )
    }
}

/// Returns true when SDM post-exec transactions are consensus-active at `timestamp`.
///
/// Defers to the hardfork where SDM is activated, matching op-node's `IsSDM` and kona's
/// `is_sdm_active`.
///
/// Single source of truth for the SDM protocol gate: call sites holding only a chain spec should
/// route through this rather than calling the underlying fork accessor directly.
pub fn is_sdm_active_at_timestamp(chain_spec: &impl OpHardforks, timestamp: u64) -> bool {
    chain_spec.is_lagoon_active_at_timestamp(timestamp)
}

fn post_exec_mode_from_transactions<'a, I, T>(
    transactions: I,
    block_number: u64,
    sdm_active: bool,
) -> Result<PostExecMode, EIP1559ParamError>
where
    I: IntoIterator<Item = &'a T>,
    T: OpConsensusTransaction + 'a,
{
    parse_post_exec_payload_from_transactions(transactions, block_number, sdm_active)
        .map_err(|_| EIP1559ParamError::InvalidPostExecPayload)
        .map(|parsed| {
            parsed.map_or_else(PostExecMode::default, |parsed| PostExecMode::Verify(parsed.payload))
        })
}

impl<ChainSpec, N, R, EvmFactory> OpEvmConfig<ChainSpec, N, R, EvmFactory>
where
    ChainSpec: OpHardforks,
    N: NodePrimitives,
{
    /// Returns the chain spec associated with this configuration.
    pub const fn chain_spec(&self) -> &Arc<ChainSpec> {
        self.executor_factory.spec()
    }

    /// Returns true when SDM post-exec transactions are consensus-active at `timestamp`.
    ///
    /// See the free [`is_sdm_active_at_timestamp`] function, which this delegates to.
    pub fn is_sdm_active_at_timestamp(&self, timestamp: u64) -> bool {
        crate::is_sdm_active_at_timestamp(self.chain_spec(), timestamp)
    }

    /// Builds a block execution context with an optional post-exec mode override.
    pub fn context_for_block_with_post_exec_mode(
        &self,
        block: &SealedBlock<N::Block>,
        post_exec_mode: Option<PostExecMode>,
    ) -> OpBlockExecutionCtx {
        OpBlockExecutionCtx {
            parent_hash: block.header().parent_hash(),
            // No parent header on this path to detect fork-activation blocks, so the executor's
            // check is skipped; the derivation layer enforces the rule instead.
            no_user_tx_activation_block: false,
            parent_beacon_block_root: block.header().parent_beacon_block_root(),
            extra_data: block.header().extra_data().clone(),
            post_exec_mode: post_exec_mode.unwrap_or_default(),
        }
    }

    /// Builds a next-block execution context with the provided post-exec mode.
    pub fn context_for_next_block_with_post_exec_mode(
        &self,
        parent: &SealedHeader<N::BlockHeader>,
        attributes: OpNextBlockEnvAttributes,
        post_exec_mode: PostExecMode,
    ) -> OpBlockExecutionCtx {
        OpBlockExecutionCtx {
            parent_hash: parent.hash(),
            no_user_tx_activation_block: self
                .chain_spec()
                .is_no_user_tx_activation_block(parent.timestamp(), attributes.timestamp),
            parent_beacon_block_root: attributes.parent_beacon_block_root,
            extra_data: attributes.extra_data,
            post_exec_mode,
        }
    }
}

impl<ChainSpec, N, R, EvmF> ConfigureEvm for OpEvmConfig<ChainSpec, N, R, EvmF>
where
    ChainSpec: EthChainSpec<Header = Header> + OpHardforks,
    N: NodePrimitives<
            Receipt = R::Receipt,
            SignedTx = R::Transaction,
            BlockHeader = Header,
            BlockBody = alloy_consensus::BlockBody<R::Transaction>,
            Block = alloy_consensus::Block<R::Transaction>,
        >,
    OpTx: FromRecoveredTx<N::SignedTx> + FromTxWithEncoded<N::SignedTx>,
    N::SignedTx: OpConsensusTransaction,
    R: OpReceiptBuilder<
            Receipt: DepositReceipt,
            Transaction: SignedTransaction + OpConsensusTransaction,
        >,
    EvmF: EvmFactory<
            Tx: FromRecoveredTx<R::Transaction>
                    + FromTxWithEncoded<R::Transaction>
                    + alloy_evm::TransactionEnvMut
                    + OpTxEnv,
            Spec = OpSpecId,
            BlockEnv = BlockEnv,
            Precompiles = PrecompilesMap,
        > + Debug,
    OpBlockExecutorFactory<R, Arc<ChainSpec>, EvmF>: for<'a> BlockExecutorFactory<
            EvmFactory = EvmF,
            ExecutionCtx<'a> = OpBlockExecutionCtx,
            Transaction = R::Transaction,
            Receipt = R::Receipt,
        >,
    Self: Send + Sync + Unpin + Clone + 'static,
{
    type Primitives = N;
    type Error = EIP1559ParamError;
    type NextBlockEnvCtx = OpNextBlockEnvAttributes;
    type BlockExecutorFactory = OpBlockExecutorFactory<R, Arc<ChainSpec>, EvmF>;
    type BlockAssembler = OpBlockAssembler<ChainSpec>;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.executor_factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        &self.block_assembler
    }

    fn evm_env(&self, header: &Header) -> Result<EvmEnv<OpSpecId>, Self::Error> {
        Ok(evm_env_for_op_block(header, self.chain_spec(), self.chain_spec().chain().id()))
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &Self::NextBlockEnvCtx,
    ) -> Result<EvmEnv<OpSpecId>, Self::Error> {
        Ok(evm_env_for_op_next_block(
            parent,
            NextEvmEnvAttributes {
                timestamp: attributes.timestamp,
                suggested_fee_recipient: attributes.suggested_fee_recipient,
                prev_randao: attributes.prev_randao,
                gas_limit: attributes.gas_limit,
                slot_number: None,
            },
            self.chain_spec().next_block_base_fee(parent, attributes.timestamp).unwrap_or_default(),
            self.chain_spec(),
            self.chain_spec().chain().id(),
        ))
    }

    fn context_for_block(
        &self,
        block: &'_ SealedBlock<N::Block>,
    ) -> Result<OpBlockExecutionCtx, Self::Error> {
        let post_exec_mode = post_exec_mode_from_transactions(
            block.body().transactions(),
            block.header().number(),
            self.is_sdm_active_at_timestamp(block.header().timestamp()),
        )?;

        Ok(self.context_for_block_with_post_exec_mode(block, Some(post_exec_mode)))
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader<N::BlockHeader>,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<OpBlockExecutionCtx, Self::Error> {
        Ok(self.context_for_next_block_with_post_exec_mode(
            parent,
            attributes,
            PostExecMode::default(),
        ))
    }
}

#[cfg(feature = "std")]
impl<ChainSpec, N, R> ConfigureEngineEvm<OpExecutionData> for OpEvmConfig<ChainSpec, N, R>
where
    ChainSpec: EthChainSpec<Header = Header> + OpHardforks,
    N: NodePrimitives<
            Receipt = R::Receipt,
            SignedTx = R::Transaction,
            BlockHeader = Header,
            BlockBody = alloy_consensus::BlockBody<R::Transaction>,
            Block = alloy_consensus::Block<R::Transaction>,
        >,
    OpTx: FromRecoveredTx<N::SignedTx> + FromTxWithEncoded<N::SignedTx>,
    N::SignedTx: Decodable2718 + OpConsensusTransaction,
    R: OpReceiptBuilder<
            Receipt: DepositReceipt,
            Transaction: SignedTransaction + OpConsensusTransaction,
        >,
    Self: Send + Sync + Unpin + Clone + 'static,
{
    fn evm_env_for_payload(
        &self,
        payload: &OpExecutionData,
    ) -> Result<EvmEnvFor<Self>, Self::Error> {
        let timestamp = payload.payload.timestamp();
        let block_number = payload.payload.block_number();

        let spec = revm_spec_by_timestamp_after_bedrock(self.chain_spec(), timestamp);

        let cfg_env = CfgEnv::new()
            .with_chain_id(self.chain_spec().chain().id())
            .with_spec_and_mainnet_gas_params(spec);

        let blob_excess_gas_and_price = spec
            .into_eth_spec()
            .is_enabled_in(SpecId::CANCUN)
            .then_some(BlobExcessGasAndPrice { excess_blob_gas: 0, blob_gasprice: 1 });

        let block_env = BlockEnv {
            number: U256::from(block_number),
            beneficiary: payload.payload.as_v1().fee_recipient,
            timestamp: U256::from(timestamp),
            difficulty: if spec.into_eth_spec() >= SpecId::MERGE {
                U256::ZERO
            } else {
                payload.payload.as_v1().prev_randao.into()
            },
            prevrandao: (spec.into_eth_spec() >= SpecId::MERGE)
                .then(|| payload.payload.as_v1().prev_randao),
            gas_limit: payload.payload.as_v1().gas_limit,
            basefee: payload.payload.as_v1().base_fee_per_gas.to(),
            // EIP-4844 excess blob gas of this block, introduced in Cancun
            blob_excess_gas_and_price,
            slot_num: 0,
        };

        Ok(EvmEnv { cfg_env, block_env })
    }

    fn context_for_payload<'a>(
        &self,
        payload: &'a OpExecutionData,
    ) -> Result<ExecutionCtxFor<'a, Self>, Self::Error> {
        let transactions = payload
            .payload
            .transactions()
            .iter()
            .map(|encoded| TxTy::<Self::Primitives>::decode_2718_exact(encoded.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| EIP1559ParamError::InvalidPostExecPayload)?;
        let post_exec_mode = post_exec_mode_from_transactions(
            transactions.iter(),
            payload.payload.block_number(),
            self.is_sdm_active_at_timestamp(payload.payload.timestamp()),
        )?;

        Ok(OpBlockExecutionCtx {
            parent_hash: payload.parent_hash(),
            // No parent header on this path to detect fork-activation blocks, so the executor's
            // check is skipped; the derivation layer enforces the rule instead.
            no_user_tx_activation_block: false,
            parent_beacon_block_root: payload.sidecar.parent_beacon_block_root(),
            extra_data: payload.payload.as_v1().extra_data.clone(),
            post_exec_mode,
        })
    }

    fn tx_iterator_for_payload(
        &self,
        payload: &OpExecutionData,
    ) -> Result<impl ExecutableTxIterator<Self>, Self::Error> {
        let transactions = payload.payload.transactions().clone();
        let convert = |encoded: Bytes| {
            let tx = TxTy::<Self::Primitives>::decode_2718_exact(encoded.as_ref())
                .map_err(AnyError::new)?;
            let signer = tx.try_recover().map_err(AnyError::new)?;
            Ok::<_, AnyError>(WithEncoded::new(encoded, tx.with_signer(signer)))
        };

        Ok((transactions, convert))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloy_consensus::{Block, BlockBody, Header, Receipt, Sealable};
    use alloy_eips::eip7685::Requests;
    use alloy_evm::Evm;
    use alloy_genesis::Genesis;
    use alloy_primitives::{
        Address, B256, Bytes, LogData, TxKind, address, bytes, keccak256,
        map::{AddressMap, B256Map, HashMap},
    };
    use op_alloy_consensus::{SDMGasEntry, build_post_exec_tx};
    use op_revm::{OpSpecId, OpTransaction};
    use reth_chainspec::ChainSpec;
    use reth_evm::execute::ProviderError;
    use reth_execution_types::{
        AccountRevertInit, BundleStateInit, Chain, ExecutionOutcome, RevertsInit,
    };
    use reth_optimism_chainspec::{OP_MAINNET, OpChainSpec, OpChainSpecBuilder};
    use reth_optimism_primitives::{OpBlock, OpPrimitives, OpReceipt, OpTransactionSigned};
    use reth_primitives_traits::{Account, RecoveredBlock, SealedBlock};
    use revm::{
        context::{
            TxEnv,
            result::{ExecutionResult, Output},
        },
        database::{BundleState, CacheDB},
        database_interface::EmptyDBTyped,
        inspector::NoOpInspector,
        primitives::Log,
        state::AccountInfo,
    };
    use std::sync::Arc;

    fn test_evm_config() -> OpEvmConfig {
        OpEvmConfig::optimism(OP_MAINNET.clone())
    }

    fn lagoon_at_timestamp_chain_spec(activation: u64) -> Arc<OpChainSpec> {
        Arc::new(
            OpChainSpecBuilder::default()
                .chain(10.into())
                .genesis(Genesis::default())
                .with_fork(
                    reth_optimism_forks::OpHardfork::Lagoon,
                    reth_chainspec::ForkCondition::Timestamp(activation),
                )
                .build(),
        )
    }

    fn b20_at_timestamp_chain_spec(activation: u64) -> Arc<OpChainSpec> {
        let mut genesis = Genesis::default();
        genesis.config.extra_fields.insert("b20Time".to_string(), serde_json::json!(activation));
        genesis.config.extra_fields.insert(
            "b20ActivationAdmin".to_string(),
            serde_json::json!("0x1111111111111111111111111111111111111111"),
        );
        Arc::new(
            OpChainSpecBuilder::default()
                .chain(10.into())
                .genesis(genesis)
                .bedrock_activated()
                .build(),
        )
    }

    #[test]
    fn all_node_execution_entrypoints_share_the_b20_factory() {
        const FACTORY: Address = address!("0177FF0000000000000000000000000000000000");

        let config = OpEvmConfig::optimism(b20_at_timestamp_chain_spec(100));
        let _: &B20OpEvmFactory<OpTx> = config.evm_factory();

        // Block import and historical RPC simulation derive the EVM from the imported header.
        let before = config
            .evm_for_block(
                CacheDB::<EmptyDBTyped<ProviderError>>::default(),
                &Header { timestamp: 99, gas_limit: 30_000_000, ..Default::default() },
            )
            .unwrap();
        let imported = config
            .evm_for_block(
                CacheDB::<EmptyDBTyped<ProviderError>>::default(),
                &Header { timestamp: 100, gas_limit: 30_000_000, ..Default::default() },
            )
            .unwrap();

        assert!(before.precompiles().get(&FACTORY).is_none());
        assert!(imported.precompiles().get(&FACTORY).is_some());

        // Payload building, pending RPC simulation and flashblocks all use next_evm_env followed by
        // evm_with_env on this same config instance.
        let parent = SealedHeader::seal_slow(Header {
            timestamp: 99,
            gas_limit: 30_000_000,
            ..Default::default()
        });
        let attributes = OpNextBlockEnvAttributes {
            timestamp: 100,
            suggested_fee_recipient: Address::ZERO,
            prev_randao: B256::ZERO,
            gas_limit: 30_000_000,
            parent_beacon_block_root: None,
            extra_data: Default::default(),
        };
        let next_env = config.next_evm_env(&parent, &attributes).unwrap();
        let built_or_pending = config
            .evm_with_env(CacheDB::<EmptyDBTyped<ProviderError>>::default(), next_env.clone());
        assert!(built_or_pending.precompiles().get(&FACTORY).is_some());

        // debug_traceCall/debug_traceTransaction use the inspector constructor with the same env.
        let traced = config.evm_with_env_and_inspector(
            CacheDB::<EmptyDBTyped<ProviderError>>::default(),
            next_env,
            NoOpInspector {},
        );
        assert!(traced.precompiles().get(&FACTORY).is_some());
    }

    #[test]
    fn rpc_simulation_and_trace_produce_identical_b20_results() {
        const FACTORY: Address = address!("0177FF0000000000000000000000000000000000");
        let config = OpEvmConfig::optimism(b20_at_timestamp_chain_spec(100));
        let header = Header { timestamp: 100, gas_limit: 30_000_000, ..Default::default() };
        let evm_env = config.evm_env(&header).unwrap();

        let mut calldata = Vec::with_capacity(36);
        calldata.extend_from_slice(&keccak256("isB20(address)")[..4]);
        calldata.extend_from_slice(&[0u8; 12]);
        calldata
            .extend_from_slice(&address!("0177000000000000000000000000000000000000").into_array());

        let transaction = || {
            OpTx(
                OpTransaction::builder()
                    .base(
                        TxEnv::builder()
                            .caller(Address::repeat_byte(0x11))
                            .chain_id(Some(10))
                            .kind(TxKind::Call(FACTORY))
                            .data(Bytes::from(calldata.clone()))
                            .gas_limit(1_000_000)
                            .gas_price(0),
                    )
                    .enveloped_tx(Some(Bytes::new()))
                    .build_fill(),
            )
        };

        let mut imported = config
            .evm_for_block(CacheDB::<EmptyDBTyped<ProviderError>>::default(), &header)
            .unwrap();
        let imported_result = imported.transact_raw(transaction()).unwrap();

        let mut simulated =
            config.evm_with_env(CacheDB::<EmptyDBTyped<ProviderError>>::default(), evm_env.clone());
        let simulated_result = simulated.transact_raw(transaction()).unwrap();

        let mut traced = config.evm_with_env_and_inspector(
            CacheDB::<EmptyDBTyped<ProviderError>>::default(),
            evm_env,
            NoOpInspector {},
        );
        let traced_result = traced.transact_raw(transaction()).unwrap();

        let mut expected = [0u8; 32];
        expected[31] = 1;
        match &simulated_result.result {
            ExecutionResult::Success { output: Output::Call(output), .. } => {
                assert_eq!(output.as_ref(), expected);
            }
            result => panic!("expected successful B20 isB20 call, got {result:?}"),
        }
        assert_eq!(imported_result.result, simulated_result.result);
        assert_eq!(imported_result.state, simulated_result.state);
        assert_eq!(simulated_result.result, traced_result.result);
        assert_eq!(simulated_result.state, traced_result.state);
    }

    #[test]
    fn sdm_rides_lagoon_activation() {
        // SDM follows Lagoon: inactive before activation, active at and after it.
        let evm_config = OpEvmConfig::optimism(lagoon_at_timestamp_chain_spec(100));

        assert!(!evm_config.is_sdm_active_at_timestamp(0));
        assert!(!evm_config.is_sdm_active_at_timestamp(99));
        assert!(evm_config.is_sdm_active_at_timestamp(100));
        assert!(evm_config.is_sdm_active_at_timestamp(101));
        assert!(evm_config.is_sdm_active_at_timestamp(u64::MAX));
    }

    #[test]
    fn sdm_inactive_without_lagoon_schedule() {
        // Without a Lagoon schedule, SDM never activates — even at far-future timestamps.
        let chain_spec = Arc::new(
            OpChainSpecBuilder::default().chain(10.into()).genesis(Genesis::default()).build(),
        );
        let evm_config = OpEvmConfig::optimism(chain_spec);

        assert!(!evm_config.is_sdm_active_at_timestamp(0));
        assert!(!evm_config.chain_spec().is_lagoon_active_at_timestamp(u64::MAX));
        assert!(!evm_config.is_sdm_active_at_timestamp(u64::MAX));
    }

    fn block_with_post_exec_tx(
        number: u64,
        timestamp: u64,
        tx_block_number: u64,
    ) -> SealedBlock<OpBlock> {
        SealedBlock::new_unhashed(Block::<OpTransactionSigned> {
            header: Header { number, timestamp, ..Default::default() },
            body: BlockBody {
                transactions: vec![OpTransactionSigned::PostExec(
                    build_post_exec_tx(
                        tx_block_number,
                        vec![SDMGasEntry { index: 0, gas_refund: 1 }],
                    )
                    .seal_slow(),
                )],
                ..Default::default()
            },
        })
    }

    // Covers Interop-driven SDM activation for imported blocks: pre-Interop blocks reject 0x7d,
    // Lagoon-active blocks enter Verify mode, and malformed payload anchors are rejected.
    #[test]
    fn context_for_block_applies_sdm_post_exec_mode() {
        let disabled_err = test_evm_config()
            .context_for_block(&block_with_post_exec_tx(7, 123, 7))
            .expect_err("SDM disabled rejects 0x7d");
        assert!(matches!(disabled_err, EIP1559ParamError::InvalidPostExecPayload));

        let evm_config = OpEvmConfig::optimism(lagoon_at_timestamp_chain_spec(0));
        let ctx = evm_config
            .context_for_block(&block_with_post_exec_tx(7, 123, 7))
            .expect("SDM-enabled block parses");
        let PostExecMode::Verify(payload) = ctx.post_exec_mode else {
            panic!("expected Verify mode");
        };
        assert_eq!(payload.block_number, 7);
        assert_eq!(payload.gas_refund_entries, vec![SDMGasEntry { index: 0, gas_refund: 1 }]);

        let mismatch_err = evm_config
            .context_for_block(&block_with_post_exec_tx(7, 123, 8))
            .expect_err("payload block number mismatch is invalid");
        assert!(matches!(mismatch_err, EIP1559ParamError::InvalidPostExecPayload));
    }

    #[test]
    fn test_fill_cfg_and_block_env() {
        // Create a default header
        let header = Header::default();

        // Build the ChainSpec for Ethereum mainnet, activating London, Paris, and Shanghai
        // hardforks
        let chain_spec = ChainSpec::builder()
            .chain(0.into())
            .genesis(Genesis::default())
            .london_activated()
            .paris_activated()
            .shanghai_activated()
            .build();

        // Use the `OpEvmConfig` to create the `cfg_env` and `block_env` based on the ChainSpec,
        // Header, and total difficulty
        let EvmEnv { cfg_env, .. } =
            OpEvmConfig::optimism(Arc::new(OpChainSpec { inner: chain_spec.clone() }))
                .evm_env(&header)
                .unwrap();

        // Assert that the chain ID in the `cfg_env` is correctly set to the chain ID of the
        // ChainSpec
        assert_eq!(cfg_env.chain_id, chain_spec.chain().id());
    }

    #[test]
    fn test_evm_with_env_default_spec() {
        let evm_config = test_evm_config();

        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        let evm_env = EvmEnv::default();

        let evm = evm_config.evm_with_env(db, evm_env.clone());

        // Check that the EVM environment
        assert_eq!(evm.cfg, evm_env.cfg_env);
    }

    #[test]
    fn test_evm_with_env_custom_cfg() {
        let evm_config = test_evm_config();

        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        // Create a custom configuration environment with a chain ID of 111
        let cfg =
            CfgEnv::new().with_chain_id(111).with_spec_and_mainnet_gas_params(OpSpecId::default());

        let evm_env = EvmEnv { cfg_env: cfg.clone(), ..Default::default() };

        let evm = evm_config.evm_with_env(db, evm_env);

        // Check that the EVM environment is initialized with the custom environment
        assert_eq!(evm.cfg, cfg);
    }

    #[test]
    fn test_evm_with_env_custom_block_and_tx() {
        let evm_config = test_evm_config();

        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        // Create customs block and tx env
        let block = BlockEnv {
            basefee: 1000,
            gas_limit: 10_000_000,
            number: U256::from(42),
            ..Default::default()
        };

        let evm_env = EvmEnv { block_env: block, ..Default::default() };

        let evm = evm_config.evm_with_env(db, evm_env.clone());

        // Verify that the block and transaction environments are set correctly
        assert_eq!(evm.block, evm_env.block_env);
    }

    #[test]
    fn test_evm_with_spec_id() {
        let evm_config = test_evm_config();

        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        let evm_env = EvmEnv {
            cfg_env: CfgEnv::new().with_spec_and_mainnet_gas_params(OpSpecId::ECOTONE),
            ..Default::default()
        };

        let evm = evm_config.evm_with_env(db, evm_env.clone());

        assert_eq!(evm.cfg, evm_env.cfg_env);
    }

    #[test]
    fn test_evm_with_env_and_default_inspector() {
        let evm_config = test_evm_config();
        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        let evm_env = EvmEnv { cfg_env: Default::default(), ..Default::default() };

        let evm = evm_config.evm_with_env_and_inspector(db, evm_env.clone(), NoOpInspector {});

        // Check that the EVM environment is set to default values
        assert_eq!(evm.block, evm_env.block_env);
        assert_eq!(evm.cfg, evm_env.cfg_env);
    }

    #[test]
    fn test_evm_with_env_inspector_and_custom_cfg() {
        let evm_config = test_evm_config();
        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        let cfg =
            CfgEnv::new().with_chain_id(111).with_spec_and_mainnet_gas_params(OpSpecId::default());
        let block = BlockEnv::default();
        let evm_env = EvmEnv { block_env: block, cfg_env: cfg.clone() };

        let evm = evm_config.evm_with_env_and_inspector(db, evm_env.clone(), NoOpInspector {});

        // Check that the EVM environment is set with custom configuration
        assert_eq!(evm.cfg, cfg);
        assert_eq!(evm.block, evm_env.block_env);
    }

    #[test]
    fn test_evm_with_env_inspector_and_custom_block_tx() {
        let evm_config = test_evm_config();
        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        // Create custom block and tx environment
        let block = BlockEnv {
            basefee: 1000,
            gas_limit: 10_000_000,
            number: U256::from(42),
            ..Default::default()
        };
        let evm_env = EvmEnv { block_env: block, ..Default::default() };

        let evm = evm_config.evm_with_env_and_inspector(db, evm_env.clone(), NoOpInspector {});

        // Verify that the block and transaction environments are set correctly
        assert_eq!(evm.block, evm_env.block_env);
    }

    #[test]
    fn test_evm_with_env_inspector_and_spec_id() {
        let evm_config = test_evm_config();
        let db = CacheDB::<EmptyDBTyped<ProviderError>>::default();

        let evm_env = EvmEnv {
            cfg_env: CfgEnv::new().with_spec_and_mainnet_gas_params(OpSpecId::ECOTONE),
            ..Default::default()
        };

        let evm = evm_config.evm_with_env_and_inspector(db, evm_env.clone(), NoOpInspector {});

        // Check that the spec ID is set properly
        assert_eq!(evm.cfg, evm_env.cfg_env);
        assert_eq!(evm.block, evm_env.block_env);
    }

    #[test]
    fn receipts_by_block_hash() {
        // Create a default recovered block
        let block: RecoveredBlock<OpBlock> = Default::default();

        // Define block hashes for block1 and block2
        let block1_hash = B256::new([0x01; 32]);
        let block2_hash = B256::new([0x02; 32]);

        // Clone the default block into block1 and block2
        let mut block1 = block.clone();
        let mut block2 = block;

        // Set the hashes of block1 and block2
        block1.set_block_number(10);
        block1.set_hash(block1_hash);

        block2.set_block_number(11);
        block2.set_hash(block2_hash);

        // Create a random receipt object, receipt1
        let receipt1 = OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        });

        // Create another random receipt object, receipt2
        let receipt2 = OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 1325345,
            logs: vec![],
            status: true.into(),
        });

        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![receipt1.clone()], vec![receipt2]];

        // Create an ExecutionOutcome object with the created bundle, receipts, an empty requests
        // vector, and first_block set to 10
        let execution_outcome = ExecutionOutcome::<OpReceipt> {
            bundle: Default::default(),
            receipts,
            requests: vec![],
            first_block: 10,
        };

        // Create a Chain object with a BTreeMap of blocks mapped to their block numbers,
        // including block1_hash and block2_hash, and the execution_outcome
        let chain: Chain<OpPrimitives> =
            Chain::new([block1, block2], execution_outcome.clone(), BTreeMap::new());

        // Assert that the proper receipt vector is returned for block1_hash
        assert_eq!(chain.receipts_by_block_hash(block1_hash), Some(vec![&receipt1]));

        // Create an ExecutionOutcome object with a single receipt vector containing receipt1
        let execution_outcome1 = ExecutionOutcome {
            bundle: Default::default(),
            receipts: vec![vec![receipt1]],
            requests: vec![],
            first_block: 10,
        };

        // Assert that the execution outcome at the first block contains only the first receipt
        assert_eq!(chain.execution_outcome_at_block(10), Some(execution_outcome1));

        // Assert that the execution outcome at the tip block contains the whole execution outcome
        assert_eq!(chain.execution_outcome_at_block(11), Some(execution_outcome));
    }

    #[test]
    fn test_initialization() {
        // Create a new BundleState object with initial data
        let bundle = BundleState::new(
            vec![(Address::new([2; 20]), None, Some(AccountInfo::default()), HashMap::default())],
            vec![vec![(Address::new([2; 20]), None, vec![])]],
            vec![],
        );

        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![Some(OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        }))]];

        // Create a Requests object with a vector of requests
        let requests = vec![Requests::new(vec![bytes!("dead"), bytes!("beef"), bytes!("beebee")])];

        // Define the first block number
        let first_block = 123;

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res = ExecutionOutcome {
            bundle: bundle.clone(),
            receipts: receipts.clone(),
            requests: requests.clone(),
            first_block,
        };

        // Assert that creating a new ExecutionOutcome using the constructor matches exec_res
        assert_eq!(
            ExecutionOutcome::new(bundle, receipts.clone(), first_block, requests.clone()),
            exec_res
        );

        // Create a BundleStateInit object and insert initial data
        let mut state_init: BundleStateInit = AddressMap::default();
        state_init
            .insert(Address::new([2; 20]), (None, Some(Account::default()), B256Map::default()));

        // Create an AddressMap for account reverts and insert initial data
        let mut revert_inner: AddressMap<AccountRevertInit> = AddressMap::default();
        revert_inner.insert(Address::new([2; 20]), (None, vec![]));

        // Create a RevertsInit object and insert the revert_inner data
        let mut revert_init: RevertsInit = HashMap::default();
        revert_init.insert(123, revert_inner);

        // Assert that creating a new ExecutionOutcome using the new_init method matches
        // exec_res
        assert_eq!(
            ExecutionOutcome::new_init(
                state_init,
                revert_init,
                vec![],
                receipts,
                first_block,
                requests,
            ),
            exec_res
        );
    }

    #[test]
    fn test_block_number_to_index() {
        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![Some(OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        }))]];

        // Define the first block number
        let first_block = 123;

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res = ExecutionOutcome {
            bundle: Default::default(),
            receipts,
            requests: vec![],
            first_block,
        };

        // Test before the first block
        assert_eq!(exec_res.block_number_to_index(12), None);

        // Test after the first block but index larger than receipts length
        assert_eq!(exec_res.block_number_to_index(133), None);

        // Test after the first block
        assert_eq!(exec_res.block_number_to_index(123), Some(0));
    }

    #[test]
    fn test_get_logs() {
        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![Log::<LogData>::default()],
            status: true.into(),
        })]];

        // Define the first block number
        let first_block = 123;

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res = ExecutionOutcome {
            bundle: Default::default(),
            receipts,
            requests: vec![],
            first_block,
        };

        // Get logs for block number 123
        let logs: Vec<&Log> = exec_res.logs(123).unwrap().collect();

        // Assert that the logs match the expected logs
        assert_eq!(logs, vec![&Log::<LogData>::default()]);
    }

    #[test]
    fn test_receipts_by_block() {
        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![Some(OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![Log::<LogData>::default()],
            status: true.into(),
        }))]];

        // Define the first block number
        let first_block = 123;

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res = ExecutionOutcome {
            bundle: Default::default(), // Default value for bundle
            receipts,                   // Include the created receipts
            requests: vec![],           // Empty vector for requests
            first_block,                // Set the first block number
        };

        // Get receipts for block number 123 and convert the result into a vector
        let receipts_by_block: Vec<_> = exec_res.receipts_by_block(123).iter().collect();

        // Assert that the receipts for block number 123 match the expected receipts
        assert_eq!(
            receipts_by_block,
            vec![&Some(OpReceipt::Legacy(Receipt::<Log> {
                cumulative_gas_used: 46913,
                logs: vec![Log::<LogData>::default()],
                status: true.into(),
            }))]
        );
    }

    #[test]
    fn test_receipts_len() {
        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![Some(OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![Log::<LogData>::default()],
            status: true.into(),
        }))]];

        // Create an empty Receipts object
        let receipts_empty = vec![];

        // Define the first block number
        let first_block = 123;

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res = ExecutionOutcome {
            bundle: Default::default(), // Default value for bundle
            receipts,                   // Include the created receipts
            requests: vec![],           // Empty vector for requests
            first_block,                // Set the first block number
        };

        // Assert that the length of receipts in exec_res is 1
        assert_eq!(exec_res.len(), 1);

        // Assert that exec_res is not empty
        assert!(!exec_res.is_empty());

        // Create a ExecutionOutcome object with an empty Receipts object
        let exec_res_empty_receipts: ExecutionOutcome<OpReceipt> = ExecutionOutcome {
            bundle: Default::default(), // Default value for bundle
            receipts: receipts_empty,   // Include the empty receipts
            requests: vec![],           // Empty vector for requests
            first_block,                // Set the first block number
        };

        // Assert that the length of receipts in exec_res_empty_receipts is 0
        assert_eq!(exec_res_empty_receipts.len(), 0);

        // Assert that exec_res_empty_receipts is empty
        assert!(exec_res_empty_receipts.is_empty());
    }

    #[test]
    fn test_revert_to() {
        // Create a random receipt object
        let receipt = OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        });

        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![vec![Some(receipt.clone())], vec![Some(receipt.clone())]];

        // Define the first block number
        let first_block = 123;

        // Create a request.
        let request = bytes!("deadbeef");

        // Create a vector of Requests containing the request.
        let requests =
            vec![Requests::new(vec![request.clone()]), Requests::new(vec![request.clone()])];

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let mut exec_res =
            ExecutionOutcome { bundle: Default::default(), receipts, requests, first_block };

        // Assert that the revert_to method returns true when reverting to the initial block number.
        assert!(exec_res.revert_to(123));

        // Assert that the receipts are properly cut after reverting to the initial block number.
        assert_eq!(exec_res.receipts, vec![vec![Some(receipt)]]);

        // Assert that the requests are properly cut after reverting to the initial block number.
        assert_eq!(exec_res.requests, vec![Requests::new(vec![request])]);

        // Assert that the revert_to method returns false when attempting to revert to a block
        // number greater than the initial block number.
        assert!(!exec_res.revert_to(133));

        // Assert that the revert_to method returns false when attempting to revert to a block
        // number less than the initial block number.
        assert!(!exec_res.revert_to(10));
    }

    #[test]
    fn test_extend_execution_outcome() {
        // Create a Receipt object with specific attributes.
        let receipt = OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        });

        // Create a Receipts object containing the receipt.
        let receipts = vec![vec![Some(receipt.clone())]];

        // Create a request.
        let request = bytes!("deadbeef");

        // Create a vector of Requests containing the request.
        let requests = vec![Requests::new(vec![request.clone()])];

        // Define the initial block number.
        let first_block = 123;

        // Create an ExecutionOutcome object.
        let mut exec_res =
            ExecutionOutcome { bundle: Default::default(), receipts, requests, first_block };

        // Extend the ExecutionOutcome object by itself.
        exec_res.extend(exec_res.clone());

        // Assert the extended ExecutionOutcome matches the expected outcome.
        assert_eq!(
            exec_res,
            ExecutionOutcome {
                bundle: Default::default(),
                receipts: vec![vec![Some(receipt.clone())], vec![Some(receipt)]],
                requests: vec![Requests::new(vec![request.clone()]), Requests::new(vec![request])],
                first_block: 123,
            }
        );
    }

    #[test]
    fn test_split_at_execution_outcome() {
        // Create a random receipt object
        let receipt = OpReceipt::Legacy(Receipt::<Log> {
            cumulative_gas_used: 46913,
            logs: vec![],
            status: true.into(),
        });

        // Create a Receipts object with a vector of receipt vectors
        let receipts = vec![
            vec![Some(receipt.clone())],
            vec![Some(receipt.clone())],
            vec![Some(receipt.clone())],
        ];

        // Define the first block number
        let first_block = 123;

        // Create a request.
        let request = bytes!("deadbeef");

        // Create a vector of Requests containing the request.
        let requests = vec![
            Requests::new(vec![request.clone()]),
            Requests::new(vec![request.clone()]),
            Requests::new(vec![request.clone()]),
        ];

        // Create a ExecutionOutcome object with the created bundle, receipts, requests, and
        // first_block
        let exec_res =
            ExecutionOutcome { bundle: Default::default(), receipts, requests, first_block };

        // Split the ExecutionOutcome at block number 124
        let result = exec_res.clone().split_at(124);

        // Define the expected lower ExecutionOutcome after splitting
        let lower_execution_outcome = ExecutionOutcome {
            bundle: Default::default(),
            receipts: vec![vec![Some(receipt.clone())]],
            requests: vec![Requests::new(vec![request.clone()])],
            first_block,
        };

        // Define the expected higher ExecutionOutcome after splitting
        let higher_execution_outcome = ExecutionOutcome {
            bundle: Default::default(),
            receipts: vec![vec![Some(receipt.clone())], vec![Some(receipt)]],
            requests: vec![Requests::new(vec![request.clone()]), Requests::new(vec![request])],
            first_block: 124,
        };

        // Assert that the split result matches the expected lower and higher outcomes
        assert_eq!(result.0, Some(lower_execution_outcome));
        assert_eq!(result.1, higher_execution_outcome);

        // Assert that splitting at the first block number returns None for the lower outcome
        assert_eq!(exec_res.clone().split_at(123), (None, exec_res));
    }
}
