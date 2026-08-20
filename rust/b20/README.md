# HSK B20 core

This directory contains the standalone Base Beryl B20 v1 execution core used by the HSK port.

## Scope

- `precompile-macros`: storage layout, namespace, accounting and precompile procedural macros.
- `precompile-storage`: REVM-backed native precompile storage, journaling abstractions, packing and test storage.
- `precompiles`: ActivationRegistry, PolicyRegistry, B20 Factory, Asset v1, Stablecoin v1 and common token logic.

The execution core remains independent of OP hardfork selection. M4 adds the adjacent `config` crate and connects this core to the OP EVM through `alloy-op-evm::B20OpPrecompiles` and `B20OpEvmFactory`.

Only Base Beryl B20 v1 is supported. The crate does not include TxContext, NonceManager, EIP-8130, state-backed ActivationRegistry administration or Cobalt execution behavior. `setAdmin(address)` is not part of the Beryl ABI and its selector remains an unknown-selector revert.

## Source baseline

The implementation was ported from Base commit:

```text
1345ec15eb84f90c437c4f321391e96d5d58073b
```

The source directory structure and the four Base golden suites are preserved. Intentional adaptations are limited to:

- workspace crate names and dependency paths;
- replacing `BaseUpgrade` with the standalone `B20Spec::{Disabled, Beryl}` selector;
- removing non-Beryl precompile/provider modules and ActivationRegistry admin rotation;
- compatibility with the pinned Optimism REVM 41 / Alloy EVM 0.37.1 workspace.

Canonical Base namespaces, addresses, feature identifiers, ABI selectors, storage layouts, gas accounting, logs, revert bytes and golden state hashes remain unchanged for Beryl behavior.

## M4 integration

- `config`: consensus configuration containing the inclusive activation timestamp and static Beryl `ActivationRegistry` administrator.
- `alloy-op-evm::B20OpPrecompiles`: wraps canonical `OpPrecompiles` and builds the `PrecompilesMap` required by reth.
- `alloy-op-evm::B20OpEvmFactory`: selects the map from the executed block timestamp for both normal and inspected EVMs.
- `reth-optimism-chainspec`: parses `h20Time` and `h20ActivationAdmin` from genesis config.
- `reth-optimism-evm`: uses the B20-aware factory for block execution and post-exec paths.

Before `h20Time`, only canonical OP precompiles are installed. At and after `h20Time`, the three B20 singletons and the Beryl dynamic lookup are installed. Structurally valid Asset and Stablecoin addresses resolve through lookup and intentionally remain absent from the static warm-address set.

## M5 execution entrypoints

The default op-reth node carries the same `OpEvmConfig<B20OpEvmFactory<OpTx>>` through every host execution path:

- imported/Engine blocks use `evm_for_block` and the configured block executor;
- Payload Builder uses `post_exec_builder_for_next_block`;
- pending blocks and flashblocks use `next_evm_env` followed by `evm_with_env`;
- call, estimate and simulate RPC methods use the node's `RpcNodeCore::Evm`;
- debug and trace methods use `evm_with_env_and_inspector` or the same configured executor.

No RPC or builder subsystem installs B20 independently. The executed block or pending payload timestamp is always the activation input, so historical calls before the fork remain B20-disabled even when the node's latest head is after the fork.

## M6 proof execution

Kona proof boot configuration carries `h20_time` and `h20_activation_admin` in `RollupConfig`.
They are validated as one consensus configuration and remain independent from OP hardfork and
`OpSpecId` selection.

- `FpvmOpEvmFactory` selects B20 config by L2 chain ID for single-chain and interop proofs.
- `OpFpvmPrecompiles` preserves host-accelerated canonical precompiles and delegates all remaining
  addresses to the same OP+B20 map, including Beryl dynamic lookup.
- Kona host witness re-execution uses `B20OpEvmFactory`.
- `ZkvmOpEvmFactory` and `OpZkvmPrecompiles` provide the same behavior for SP1 range and
  super-consolidation proofs while retaining ZKVM cycle tracking.
- Proof executor fixtures construct their factory from the fixture's validated `RollupConfig`.

Differential tests execute a state-changing `ActivationRegistry` call through op-reth, FPVM, and
SP1 factories and compare the complete execution result, state diff, MPT state root, and output
root. A `StatelessL2Builder` test additionally proves that op-reth-style and FPVM execution produce
the same complete B20 block header, receipts root, state root, block hash, and output root.

## Verification

Run from `optimism/rust`:

```bash
cargo test --locked \
  -p b20-precompile-macros \
  -p b20-precompile-storage \
  -p hsk-b20-precompiles \
  --all-features --no-fail-fast

cargo clippy --locked \
  -p b20-precompile-macros \
  -p b20-precompile-storage \
  -p hsk-b20-precompiles \
  --all-targets --all-features -- -D warnings

cargo test --locked \
  -p hsk-b20-config \
  -p hsk-b20-precompiles \
  -p alloy-op-evm \
  -p reth-optimism-chainspec \
  -p reth-optimism-evm

cargo clippy --locked \
  -p hsk-b20-config \
  -p hsk-b20-precompiles \
  -p alloy-op-evm \
  -p reth-optimism-chainspec \
  -p reth-optimism-evm \
  --all-targets -- -D warnings
```

The golden suites are:

- `b20_asset_v1_golden`
- `b20_factory_v1_golden`
- `b20_policy_v1_golden`
- `b20_stablecoin_v1_golden`
