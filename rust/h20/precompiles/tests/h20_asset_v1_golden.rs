//! Golden tests pinning Asset **V1** behavior of the H20 precompile.
//!
//! These are authored and pinned against the shipped **v1.1.1** (pre-versioned) asset
//! implementation; the conversion to the versioned precompile structure is behavior-preserving
//! and continues to satisfy every pin below unchanged.
//!
//! Every op (mutations, computed reads, direct/const reads) is driven through the
//! **version-resolver-gated** dispatch path (`H20Spec::Beryl` -> `AssetVersion::V1`) against
//! the real EVM-backed `H20AssetStorage` over `HashMapStorageProvider`, with an
//! `FakePolicyAccounting` for deterministic allow/block decisions. Each case asserts:
//!   1. exact returned ABI bytes (or the typed revert),
//!   2. resulting state (balances / supply / roles / allowances / multiplier / metadata / storage),
//!   3. emitted events, and
//!   4. a per-case keccak storage **hash** snapshot (the frozen-manifest baseline).
//!
//! Because the per-op suite resolves the version via `AssetVersions::from_spec`, it breaks
//! if dispatch ever routes to the wrong version. Privileged behavior is exercised via
//! `route` with `privileged = true`; the guard envelope (nonpayable / uninitialized / pre-Beryl)
//! via the full `dispatch_with_observer`.
//!
//! ## Blessing storage hashes
//! State-root constants below are pinned. To (re)generate them after an intentional change, run:
//! `BLESS_GOLDEN=1 cargo test -p hsk-h20-precompiles --features test-utils \
//!    --test h20_asset_v1_golden -- --nocapture` and copy the printed `GOLDEN_ROOT` values.

use alloy_primitives::{Address, B256, Bytes, U256, b256, keccak256};
use alloy_sol_types::{SolCall, SolError, SolEvent, SolValue};
use h20_precompile_storage::{H20PrecompileError, HashMapStorageProvider, StorageCtx};
use hsk_h20_precompiles::{
    Asset, AssetAccounting, AssetV1, AssetVersion, AssetVersions, FakePolicyAccounting,
    H20_MAX_SUPPLY_CAP, H20AssetInit, H20AssetStorage, H20AssetToken, H20PolicyType, H20Spec,
    H20TokenRole, IH20, IH20Asset, NoopPrecompileCallObserver, PolicyVersion, TokenAccounting,
};

mod common;
use common::{
    ADMIN, ALICE, BOB, CAROL, CHAIN_ID, MEMO, POLICY_ID, TOKEN, anvil_owner, bless_or_assert_gas,
    bless_or_assert_root, hash_token_state, ok_true, signed_permit, u,
};

// --- fixtures ---------------------------------------------------------------

const NAME: &str = "HSK Asset";
const SYMBOL: &str = "bASSET";
const DECIMALS: u8 = 6;
const LOGIC: AssetV1 = AssetV1;

// --- pinned storage hashes (bless with BLESS_GOLDEN=1; see module docs) --------

const ROOT_FRESH: B256 = b256!("e72f0ac753527ff8ccdebb7282fe047bd2c5c9d3ede18f68c4b7023dcdb03bcb");
const ROOT_TRANSFER_PRIV: B256 =
    b256!("745fcd12af5255ca7503f5e9af631e5ae63c22c616d803a9432872f4d1eecb5c");
const ROOT_TRANSFER_UNPRIV: B256 =
    b256!("ad41f06621c450971f9287dbfe69b9f4fab71258f454653b22ad3624106e2a98");
const ROOT_TRANSFER_WITH_MEMO: B256 =
    b256!("88c44a2c020712b43308f9c5554b12479d5c5a9ff7a1bcf8bea26177d8762691");
const ROOT_TRANSFER_FROM_FINITE: B256 =
    b256!("c8f8e3a2cbd872ec4a6bc33282c9e1bf0921cac07af4ba2fc37dfb337c095e41");
const ROOT_TRANSFER_FROM_INFINITE: B256 =
    b256!("05832590b133ba6328d731f738826829979ea57dfbd182f7837027a126156d49");
const ROOT_TRANSFER_FROM_WITH_MEMO: B256 =
    b256!("d1a612d3627b85467b186bdcb2a0cf8eda5fc59518e4d7cbf65951344519cf9d");
const ROOT_APPROVE: B256 =
    b256!("81737b9e34ded7f2e57b8de24667d43ff40af065f85ca098b508f2d8bb2d0ba5");
const ROOT_MINT_PRIV: B256 =
    b256!("b1feb201245adaec308f0990af5394fbfc6e6b91fa97cf273e5fd44ee4f9ba74");
const ROOT_MINT_UNPRIV: B256 =
    b256!("6d415f11a1ff27cecfc020d25c841e0a0481b0908c38bbe8fa10b946961f67e2");
const ROOT_MINT_WITH_MEMO: B256 =
    b256!("9bacb79742238476432be38285fde6c487f965f2ea5e64390f3931d636f7e169");
const ROOT_BURN: B256 = b256!("6aa0cdcdbddc5ff56b991324350572f9242f3466b2988a14bb529aba52f2c096");
const ROOT_BURN_WITH_MEMO: B256 =
    b256!("399c806a791dda866ce4524abcbe6cf2fe64c7e5c8c83cc24eb3e716315bbeea");
const ROOT_BURN_BLOCKED: B256 =
    b256!("fb7c4336e1e3dda5a56c2545bc744d0a9e00e25ab5c4a679384daa291b1ffa05");
const ROOT_PAUSE: B256 = b256!("3045f72c95fecf6cdde08d26c976d248ecf9ecdfc113ac04cab61fa567b6ea67");
const ROOT_UNPAUSE: B256 =
    b256!("9ee3ab4650c9d1add035c81f95c16bbbdfdba75da5853558e2c14977d539ff4c");
const ROOT_UPDATE_SUPPLY_CAP: B256 =
    b256!("be6effca50e37f6902aa57d616453aa028885e45f0e3f4625060c0a6d705fcb7");
const ROOT_UPDATE_NAME: B256 =
    b256!("81b445044647f3f853bfae0ee00ef9c97e7b98e68eeeec5dac009aa5e1efe8e7");
const ROOT_UPDATE_SYMBOL: B256 =
    b256!("85e6e2712a682482f66cde7bf241ea8e8ac6150c1137dac73ea738712ef6935e");
const ROOT_UPDATE_CONTRACT_URI: B256 =
    b256!("6475688b73c4e486fe41b0270570ed66f9e46114836c8ceaef0700308b4442d3");
const ROOT_GRANT_ROLE: B256 =
    b256!("b69f3a1e6d0253b43aec80e045b53f558be96e51db2b29244f80e4c9d315c79f");
const ROOT_REVOKE_ROLE: B256 =
    b256!("7ea621f5173c9bbb778ffee40281a95db225ee45df8b24766814e3add1957031");
const ROOT_RENOUNCE_ROLE: B256 =
    b256!("5ee5fdf1bcf6535621e06e49bdc743c4272465da10573e0125e1e3c9fbfd45e2");
const ROOT_RENOUNCE_LAST_ADMIN: B256 =
    b256!("ebcf9e50d745db5de136fc46778e38ab9b10f24691ca6ab7e22599b27091b279");
const ROOT_SET_ROLE_ADMIN: B256 =
    b256!("ba3bed516eed267b1a3926707ccee3a8d2f55c44aec3d4b4b2c7c5c9e6d978ca");
const ROOT_UPDATE_POLICY: B256 =
    b256!("46838f2601bf04a0cb59d27a7877c17f4f5303fddef90f6ea2bf13573177be08");
const ROOT_PERMIT: B256 = b256!("53b56446b15ae21570b426c97da8f419360d47a06e27c9ad6709ad95f78bc3dc");
const ROOT_GRANT_DEFAULT_ADMIN: B256 =
    b256!("4b468609fdcf8e2e95a0c55ef4fab890c71058b88b5afd468f30c8220520ac07");
const ROOT_GRANT_IDEMPOTENT: B256 =
    b256!("f7d0d17931202e39a18b044e19840369546611c26fe98379211d966f4a4729d8");
const ROOT_GRANT_UNCHECKED: B256 =
    b256!("973f900dd3ee8d005856c2e1a71ae3b77b9b37139e87253564b621065b4e5dad");

// asset-specific (blessed against v1.1.1)
const ROOT_MULTIPLIER_SCALED: B256 =
    b256!("673b44f2db0c3c08d7755905d8113d5b8a7f291c85ae7ac91739cb83bb6e4207");
const ROOT_ANNOUNCE_ID_USED: B256 =
    b256!("f51c33c0ff8b0f0a3570fe52b80fea507c46a44fbf5e1019263fee983bf62ae1");
const ROOT_EXTRA_METADATA_READ: B256 =
    b256!("e6623f806638965a65f7adcf57eb621e9c42c1af4222daebdb0d7616323e226b");
const ROOT_UPDATE_MULTIPLIER: B256 =
    b256!("3807a9ebdaae87cd3dfb2361dbe9c5eea63a78b0f7e031dc80eb59ea75b9237d");
const ROOT_BATCH_MINT: B256 =
    b256!("b234dd9c521aa7ae520f6b633ac2cfcac4d012c11fc9d779e06854145142aa21");
const ROOT_METADATA_SET: B256 =
    b256!("2f0f517b64ee713ea806de5a3d813ea76771e42e5e69f172cf49205bf77efa3a");
const ROOT_METADATA_REMOVE: B256 =
    b256!("ba75e3a5e4ed72f040b1ab2068f396354b11e1835ea7584fe039e6201fa0e3d5");
const ROOT_ANNOUNCE: B256 =
    b256!("618a608fbd44bd82b675d9798f8dcf17ccff09ba30eeb8ffc8fc7f15baf5c7a0");

// --- harness ----------------------------------------------------------------

/// Fresh provider with an initialized `HSK Asset` at [`TOKEN`] (multiplier = 1 WAD).
fn fresh() -> HashMapStorageProvider {
    let mut storage = HashMapStorageProvider::new(CHAIN_ID);
    StorageCtx::enter(&mut storage, |ctx| {
        let mut token = H20AssetStorage::from_address(TOKEN, ctx);
        token
            .initialize(H20AssetInit {
                name: NAME.into(),
                symbol: SYMBOL.into(),
                supply_cap: H20_MAX_SUPPLY_CAP,
                multiplier: H20AssetStorage::WAD,
                decimals: DECIMALS,
            })
            .expect("initialize asset");
    });
    storage
}

/// Mutates raw token storage through the accounting port (test setup only).
fn seed(storage: &mut HashMapStorageProvider, f: impl FnOnce(&mut H20AssetStorage<'_>)) {
    StorageCtx::enter(storage, |ctx| {
        let mut token = H20AssetStorage::from_address(TOKEN, ctx);
        f(&mut token);
    });
}

/// Reads token state through the accounting port.
fn read<R>(storage: &mut HashMapStorageProvider, f: impl FnOnce(&H20AssetStorage<'_>) -> R) -> R {
    StorageCtx::enter(storage, |ctx| f(&H20AssetStorage::from_address(TOKEN, ctx)))
}

/// Drives one op through the resolver-gated (`Beryl` -> V1) unprivileged path.
fn op(
    storage: &mut HashMapStorageProvider,
    caller: Address,
    policy: FakePolicyAccounting,
    calldata: Vec<u8>,
) -> Result<Bytes, H20PrecompileError> {
    storage.set_caller(caller);
    StorageCtx::enter(storage, |ctx| {
        let version = AssetVersions::from_spec(H20Spec::Beryl).expect("Beryl activates V1");
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            policy,
            PolicyVersion::V1,
        )
        .route(ctx, &calldata, version, false, NoopPrecompileCallObserver)
    })
}

/// Drives one op through V1 with factory-init privilege (guards skipped).
fn op_privileged(
    storage: &mut HashMapStorageProvider,
    caller: Address,
    policy: FakePolicyAccounting,
    calldata: Vec<u8>,
) -> Result<Bytes, H20PrecompileError> {
    storage.set_caller(caller);
    StorageCtx::enter(storage, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            policy,
            PolicyVersion::V1,
        )
        .route(ctx, &calldata, AssetVersion::V1, true, NoopPrecompileCallObserver)
    })
}

/// Topic-0 (signature hash) of the last event emitted by the token.
fn last_topic0(storage: &HashMapStorageProvider) -> B256 {
    storage.get_events(TOKEN).last().expect("an emitted event").topics()[0]
}

/// Asserts the token's storage hash, or prints it under `BLESS_GOLDEN` for (re)pinning.
#[track_caller]
fn assert_root(label: &str, storage: HashMapStorageProvider, expected: B256) {
    bless_or_assert_root(label, hash_token_state(storage, TOKEN), expected);
}

/// Grants `role` to `who` and bumps the role member count (setup only).
fn give_role(token: &mut H20AssetStorage<'_>, role: B256, who: Address) {
    token.set_role(role, who, true).unwrap();
    let next = token.role_member_count(role).unwrap() + U256::ONE;
    token.set_role_member_count(role, next).unwrap();
}

/// Credits `who` with `amount` and grows total supply to match (setup only).
fn fund(token: &mut H20AssetStorage<'_>, who: Address, amount: U256) {
    let balance = token.balance_of(who).unwrap();
    token.set_balance(who, balance + amount).unwrap();
    let supply = token.total_supply().unwrap();
    token.set_total_supply(supply + amount).unwrap();
}

/// The asset operator role id: `keccak256("OPERATOR_ROLE")` (V1 pins this equality).
fn operator_role() -> B256 {
    keccak256("OPERATOR_ROLE")
}

/// The V1 EIP-712 domain separator for the token at [`TOKEN`] on [`CHAIN_ID`].
fn domain_separator(storage: &mut HashMapStorageProvider) -> B256 {
    StorageCtx::enter(storage, |ctx| {
        let token = H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        );
        LOGIC.domain_separator(&token, CHAIN_ID).unwrap()
    })
}

// ============================================================================
// Version resolver
// ============================================================================

#[test]
fn resolver_maps_forks_to_versions() {
    assert_eq!(AssetVersions::from_spec(H20Spec::Disabled), None);
    assert_eq!(AssetVersions::from_spec(H20Spec::Beryl), Some(AssetVersion::V1));
}

// ============================================================================
// transfer
// ============================================================================

#[test]
fn golden_transfer_privileged() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(100)));

    let out = op_privileged(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: BOB, amount: u(30) }.abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    read(&mut s, |t| {
        assert_eq!(t.balance_of(ALICE).unwrap(), u(70));
        assert_eq!(t.balance_of(BOB).unwrap(), u(30));
    });
    assert_eq!(last_topic0(&s), IH20::Transfer::SIGNATURE_HASH);
    assert_root("transfer_privileged", s, ROOT_TRANSFER_PRIV);
}

#[test]
fn golden_transfer_unprivileged_allowed() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_policy_id(H20PolicyType::TransferSender.id(), POLICY_ID).unwrap();
        t.set_policy_id(H20PolicyType::TransferReceiver.id(), POLICY_ID).unwrap();
    });
    // Authorize sender + receiver under the configured policy => guards pass.
    let mut policy = FakePolicyAccounting::new();
    policy.allow(POLICY_ID, ALICE);
    policy.allow(POLICY_ID, BOB);
    let out = op(&mut s, ALICE, policy, IH20::transferCall { to: BOB, amount: u(10) }.abi_encode())
        .unwrap();

    assert_eq!(out, ok_true());
    read(&mut s, |t| {
        assert_eq!(t.balance_of(ALICE).unwrap(), u(90));
        assert_eq!(t.balance_of(BOB).unwrap(), u(10));
    });
    assert_root("transfer_unprivileged", s, ROOT_TRANSFER_UNPRIV);
}

#[test]
fn golden_transfer_unprivileged_blocked_sender_reverts() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        // Configure a real sender policy that authorizes nobody => sender blocked.
        t.set_policy_id(H20PolicyType::TransferSender.id(), POLICY_ID).unwrap();
    });
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: BOB, amount: u(10) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::PolicyForbids {
            policyScope: H20PolicyType::TransferSender.id(),
            policyId: POLICY_ID,
        })
    );
}

#[test]
fn golden_transfer_reverts_zero_receiver() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(10)));
    let err = op_privileged(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: Address::ZERO, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidReceiver { receiver: Address::ZERO }));
}

#[test]
fn golden_transfer_reverts_insufficient_balance() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(10)));
    let err = op_privileged(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: BOB, amount: u(50) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::InsufficientBalance {
            sender: ALICE,
            balance: u(10),
            needed: u(50),
        })
    );
}

#[test]
fn golden_transfer_reverts_when_paused() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(10)));
    op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::TRANSFER] }.abi_encode(),
    )
    .unwrap();
    let err = op_privileged(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: BOB, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::ContractPaused {
            feature: IH20::PausableFeature::TRANSFER
        })
    );
}

#[test]
fn golden_transfer_with_memo_emits_transfer_then_memo() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(100)));
    let out = op_privileged(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::transferWithMemoCall { to: BOB, amount: u(30), memo: MEMO }.abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    let events = s.get_events(TOKEN);
    assert_eq!(events[events.len() - 2].topics()[0], IH20::Transfer::SIGNATURE_HASH);
    assert_eq!(events[events.len() - 1].topics()[0], IH20::Memo::SIGNATURE_HASH);
    assert_root("transfer_with_memo", s, ROOT_TRANSFER_WITH_MEMO);
}

// ============================================================================
// transferFrom
// ============================================================================

#[test]
fn golden_transfer_from_finite_allowance_decrements() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_allowance(ALICE, BOB, u(40)).unwrap();
    });
    let out = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: ALICE, to: BOB, amount: u(30) }.abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    read(&mut s, |t| {
        assert_eq!(t.allowance(ALICE, BOB).unwrap(), u(10));
        assert_eq!(t.balance_of(BOB).unwrap(), u(30));
    });
    assert_root("transfer_from_finite", s, ROOT_TRANSFER_FROM_FINITE);
}

#[test]
fn golden_transfer_from_infinite_allowance_not_decremented() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_allowance(ALICE, BOB, U256::MAX).unwrap();
    });
    let out = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: ALICE, to: BOB, amount: u(30) }.abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    read(&mut s, |t| assert_eq!(t.allowance(ALICE, BOB).unwrap(), U256::MAX));
    assert_root("transfer_from_infinite", s, ROOT_TRANSFER_FROM_INFINITE);
}

#[test]
fn golden_transfer_from_reverts_insufficient_allowance() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_allowance(ALICE, BOB, u(5)).unwrap();
    });
    let err = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: ALICE, to: BOB, amount: u(30) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::InsufficientAllowance {
            spender: BOB,
            allowance: u(5),
            needed: u(30),
        })
    );
}

#[test]
fn golden_transfer_from_unprivileged_enforces_executor_policy() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_allowance(ALICE, BOB, u(40)).unwrap();
    });
    seed(&mut s, |t| {
        t.set_policy_id(H20PolicyType::TransferExecutor.id(), POLICY_ID).unwrap();
    });
    // BOB (executor, != from) is not authorized under the executor policy => forbidden.
    let err = op(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: ALICE, to: CAROL, amount: u(10) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::PolicyForbids {
            policyScope: H20PolicyType::TransferExecutor.id(),
            policyId: POLICY_ID,
        })
    );
}

#[test]
fn golden_transfer_from_with_memo() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_allowance(ALICE, BOB, u(40)).unwrap();
    });
    let out = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromWithMemoCall { from: ALICE, to: CAROL, amount: u(30), memo: MEMO }
            .abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    let events = s.get_events(TOKEN);
    assert_eq!(events[events.len() - 2].topics()[0], IH20::Transfer::SIGNATURE_HASH);
    assert_eq!(events[events.len() - 1].topics()[0], IH20::Memo::SIGNATURE_HASH);
    assert_root("transfer_from_with_memo", s, ROOT_TRANSFER_FROM_WITH_MEMO);
}

// ============================================================================
// approve
// ============================================================================

#[test]
fn golden_approve_sets_allowance_and_emits() {
    let mut s = fresh();
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::approveCall { spender: BOB, amount: u(50) }.abi_encode(),
    )
    .unwrap();

    assert_eq!(out, ok_true());
    read(&mut s, |t| assert_eq!(t.allowance(ALICE, BOB).unwrap(), u(50)));
    assert_eq!(last_topic0(&s), IH20::Approval::SIGNATURE_HASH);
    assert_root("approve", s, ROOT_APPROVE);
}

#[test]
fn golden_approve_reverts_zero_spender() {
    let mut s = fresh();
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::approveCall { spender: Address::ZERO, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidSpender { spender: Address::ZERO }));
}

// ============================================================================
// mint
// ============================================================================

#[test]
fn golden_mint_privileged_still_enforces_receiver_policy() {
    let mut s = fresh();
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB); // MintReceiver enforced even when privileged
    let out = op_privileged(
        &mut s,
        ADMIN,
        policy,
        IH20::mintCall { to: BOB, amount: u(100) }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert_eq!(t.balance_of(BOB).unwrap(), u(100));
        assert_eq!(t.total_supply().unwrap(), u(100));
    });
    assert_eq!(last_topic0(&s), IH20::Transfer::SIGNATURE_HASH);
    assert_root("mint_privileged", s, ROOT_MINT_PRIV);
}

#[test]
fn golden_mint_unprivileged_requires_role_and_policy() {
    let mut s = fresh();
    // Missing MINT_ROLE => unauthorized.
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB);
    let err = op(&mut s, ALICE, policy, IH20::mintCall { to: BOB, amount: u(1) }.abi_encode())
        .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::Mint.id(),
        })
    );

    // With MINT_ROLE + authorized receiver => succeeds.
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB);
    let out =
        op(&mut s, ALICE, policy, IH20::mintCall { to: BOB, amount: u(75) }.abi_encode()).unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.balance_of(BOB).unwrap(), u(75)));
    assert_root("mint_unprivileged", s, ROOT_MINT_UNPRIV);
}

#[test]
fn golden_mint_reverts_over_supply_cap() {
    let mut s = fresh();
    seed(&mut s, |t| t.set_supply_cap(u(50)).unwrap());
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB);
    let err = op_privileged(
        &mut s,
        ADMIN,
        policy,
        IH20::mintCall { to: BOB, amount: u(100) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::SupplyCapExceeded { cap: u(50), attempted: u(100) })
    );
}

#[test]
fn golden_mint_with_memo() {
    let mut s = fresh();
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB);
    let out = op_privileged(
        &mut s,
        ADMIN,
        policy,
        IH20::mintWithMemoCall { to: BOB, amount: u(40), memo: MEMO }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    let events = s.get_events(TOKEN);
    assert_eq!(events[events.len() - 2].topics()[0], IH20::Transfer::SIGNATURE_HASH);
    assert_eq!(events[events.len() - 1].topics()[0], IH20::Memo::SIGNATURE_HASH);
    assert_root("mint_with_memo", s, ROOT_MINT_WITH_MEMO);
}

// ============================================================================
// burn / burnBlocked
// ============================================================================

#[test]
fn golden_burn_requires_role_then_reduces_supply() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(100)));

    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::burnCall { amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::Burn.id(),
        })
    );

    seed(&mut s, |t| give_role(t, H20TokenRole::Burn.id(), ALICE));
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::burnCall { amount: u(40) }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert_eq!(t.balance_of(ALICE).unwrap(), u(60));
        assert_eq!(t.total_supply().unwrap(), u(60));
    });
    assert_eq!(last_topic0(&s), IH20::Transfer::SIGNATURE_HASH);
    assert_root("burn", s, ROOT_BURN);
}

#[test]
fn golden_burn_with_memo() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        give_role(t, H20TokenRole::Burn.id(), ALICE);
    });
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::burnWithMemoCall { amount: u(40), memo: MEMO }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    let events = s.get_events(TOKEN);
    assert_eq!(events[events.len() - 2].topics()[0], IH20::Transfer::SIGNATURE_HASH);
    assert_eq!(events[events.len() - 1].topics()[0], IH20::Memo::SIGNATURE_HASH);
    assert_root("burn_with_memo", s, ROOT_BURN_WITH_MEMO);
}

#[test]
fn golden_burn_blocked_destroys_from_blocked_account() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        // Configure a real transfer-sender policy that does not authorize ALICE => blocked.
        t.set_policy_id(H20PolicyType::TransferSender.id(), POLICY_ID).unwrap();
    });
    // ALICE blocked; privileged skips the role check.
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::burnBlockedCall { from: ALICE, amount: u(40) }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.balance_of(ALICE).unwrap(), u(60)));
    assert_eq!(last_topic0(&s), IH20::BurnedBlocked::SIGNATURE_HASH);
    assert_root("burn_blocked", s, ROOT_BURN_BLOCKED);
}

#[test]
fn golden_burn_blocked_reverts_when_not_blocked() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_policy_id(H20PolicyType::TransferSender.id(), POLICY_ID).unwrap();
    });
    let mut policy = FakePolicyAccounting::new();
    policy.allow(POLICY_ID, ALICE); // authorized => not blocked
    let err = op_privileged(
        &mut s,
        ADMIN,
        policy,
        IH20::burnBlockedCall { from: ALICE, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::AccountNotBlocked { account: ALICE }));
}

// ============================================================================
// pause / unpause
// ============================================================================

#[test]
fn golden_pause_sets_feature_bit() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    assert_eq!(last_topic0(&s), IH20::Paused::SIGNATURE_HASH);
    assert_root("pause", s, ROOT_PAUSE);
}

#[test]
fn golden_unpause_clears_feature_bit() {
    let mut s = fresh();
    op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::unpauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    assert_eq!(last_topic0(&s), IH20::Unpaused::SIGNATURE_HASH);
    assert_root("unpause", s, ROOT_UNPAUSE);
}

#[test]
fn golden_pause_reverts_empty_feature_set() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::EmptyFeatureSet {}));
}

#[test]
fn golden_pause_unprivileged_requires_role() {
    let mut s = fresh();
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::Pause.id(),
        })
    );
}

// ============================================================================
// config / metadata
// ============================================================================

#[test]
fn golden_update_supply_cap() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updateSupplyCapCall { newSupplyCap: u(1_000) }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.supply_cap().unwrap(), u(1_000)));
    assert_eq!(last_topic0(&s), IH20::SupplyCapUpdated::SIGNATURE_HASH);
    assert_root("update_supply_cap", s, ROOT_UPDATE_SUPPLY_CAP);
}

#[test]
fn golden_update_supply_cap_reverts_below_supply() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(500)));
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updateSupplyCapCall { newSupplyCap: u(100) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::InvalidSupplyCap {
            currentSupply: u(500),
            proposedCap: u(100),
        })
    );
}

#[test]
fn golden_update_name_emits_name_and_domain_changed() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updateNameCall { newName: "New Name".into() }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.name().unwrap(), "New Name"));
    let events = s.get_events(TOKEN);
    assert_eq!(events[events.len() - 2].topics()[0], IH20::NameUpdated::SIGNATURE_HASH);
    assert_eq!(events[events.len() - 1].topics()[0], IH20::EIP712DomainChanged::SIGNATURE_HASH);
    assert_root("update_name", s, ROOT_UPDATE_NAME);
}

#[test]
fn golden_update_symbol() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updateSymbolCall { newSymbol: "USDX".into() }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.symbol().unwrap(), "USDX"));
    assert_eq!(last_topic0(&s), IH20::SymbolUpdated::SIGNATURE_HASH);
    assert_root("update_symbol", s, ROOT_UPDATE_SYMBOL);
}

#[test]
fn golden_update_contract_uri() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updateContractURICall { newURI: "ipfs://x".into() }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.contract_uri().unwrap(), "ipfs://x"));
    assert_eq!(last_topic0(&s), IH20::ContractURIUpdated::SIGNATURE_HASH);
    assert_root("update_contract_uri", s, ROOT_UPDATE_CONTRACT_URI);
}

// ============================================================================
// roles
// ============================================================================

#[test]
fn golden_grant_role() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::grantRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert!(t.has_role(H20TokenRole::Mint.id(), ALICE).unwrap()));
    assert_eq!(last_topic0(&s), IH20::RoleGranted::SIGNATURE_HASH);
    assert_root("grant_role", s, ROOT_GRANT_ROLE);
}

#[test]
fn golden_revoke_role() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::revokeRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert!(!t.has_role(H20TokenRole::Mint.id(), ALICE).unwrap()));
    assert_eq!(last_topic0(&s), IH20::RoleRevoked::SIGNATURE_HASH);
    assert_root("revoke_role", s, ROOT_REVOKE_ROLE);
}

#[test]
fn golden_revoke_last_admin_rejected() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::revokeRoleCall { role: H20TokenRole::DefaultAdmin.id(), account: ADMIN }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::LastAdminCannotRenounce {}));
}

#[test]
fn golden_renounce_role() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::renounceRoleCall { role: H20TokenRole::Mint.id(), callerConfirmation: ALICE }
            .abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert!(!t.has_role(H20TokenRole::Mint.id(), ALICE).unwrap()));
    assert_eq!(last_topic0(&s), IH20::RoleRevoked::SIGNATURE_HASH);
    assert_root("renounce_role", s, ROOT_RENOUNCE_ROLE);
}

#[test]
fn golden_renounce_role_bad_confirmation() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::renounceRoleCall { role: H20TokenRole::Mint.id(), callerConfirmation: BOB }
            .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::AccessControlBadConfirmation {}));
}

#[test]
fn golden_renounce_last_admin() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let out =
        op(&mut s, ADMIN, FakePolicyAccounting::new(), IH20::renounceLastAdminCall {}.abi_encode())
            .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert!(!t.has_role(H20TokenRole::DefaultAdmin.id(), ADMIN).unwrap());
        assert_eq!(t.role_member_count(H20TokenRole::DefaultAdmin.id()).unwrap(), U256::ZERO);
    });
    assert_eq!(last_topic0(&s), IH20::LastAdminRenounced::SIGNATURE_HASH);
    assert_root("renounce_last_admin", s, ROOT_RENOUNCE_LAST_ADMIN);
}

#[test]
fn golden_renounce_last_admin_reverts_when_not_sole() {
    let mut s = fresh();
    seed(&mut s, |t| {
        give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN);
        give_role(t, H20TokenRole::DefaultAdmin.id(), BOB);
    });
    let err =
        op(&mut s, ADMIN, FakePolicyAccounting::new(), IH20::renounceLastAdminCall {}.abi_encode())
            .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::NotSoleAdmin {}));
}

#[test]
fn golden_set_role_admin() {
    let mut s = fresh();
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::setRoleAdminCall {
            role: H20TokenRole::Mint.id(),
            newAdminRole: H20TokenRole::Metadata.id(),
        }
        .abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert_eq!(t.role_admin(H20TokenRole::Mint.id()).unwrap(), H20TokenRole::Metadata.id())
    });
    assert_eq!(last_topic0(&s), IH20::RoleAdminChanged::SIGNATURE_HASH);
    assert_root("set_role_admin", s, ROOT_SET_ROLE_ADMIN);
}

// ============================================================================
// policy
// ============================================================================

#[test]
fn golden_update_policy() {
    let mut s = fresh();
    let mut policy = FakePolicyAccounting::new();
    policy.create_existing_policy(7);
    let out = op_privileged(
        &mut s,
        ADMIN,
        policy,
        IH20::updatePolicyCall { policyScope: H20PolicyType::TransferSender.id(), newPolicyId: 7 }
            .abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.policy_id(H20PolicyType::TransferSender.id()).unwrap(), 7));
    assert_eq!(last_topic0(&s), IH20::PolicyUpdated::SIGNATURE_HASH);
    assert_root("update_policy", s, ROOT_UPDATE_POLICY);
}

#[test]
fn golden_update_policy_reverts_missing_policy() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::updatePolicyCall { policyScope: H20PolicyType::TransferSender.id(), newPolicyId: 99 }
            .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::PolicyNotFound { policyId: 99 }));
}

// ============================================================================
// permit
// ============================================================================

#[test]
fn golden_permit_sets_allowance_and_increments_nonce() {
    let mut s = fresh();
    let owner = anvil_owner();
    let domain = domain_separator(&mut s);
    let call = signed_permit(domain, U256::ZERO, owner, BOB, u(500), U256::MAX);
    s.set_timestamp(U256::ZERO);
    let out = op(&mut s, owner, FakePolicyAccounting::new(), call.abi_encode()).unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert_eq!(t.allowance(owner, BOB).unwrap(), u(500));
        assert_eq!(t.nonce(owner).unwrap(), U256::ONE);
    });
    assert_eq!(last_topic0(&s), IH20::Approval::SIGNATURE_HASH);
    assert_root("permit", s, ROOT_PERMIT);
}

#[test]
fn golden_permit_reverts_when_expired() {
    let mut s = fresh();
    let owner = anvil_owner();
    let domain = domain_separator(&mut s);
    let call = signed_permit(domain, U256::ZERO, owner, BOB, u(1), u(10));
    s.set_timestamp(u(11));
    let err = op(&mut s, owner, FakePolicyAccounting::new(), call.abi_encode()).unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::ExpiredSignature { deadline: u(10) }));
}

// ============================================================================
// computed reads
// ============================================================================

#[test]
fn golden_read_is_paused_and_paused_features() {
    let mut s = fresh();
    op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap();

    let paused_mint = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::isPausedCall { feature: IH20::PausableFeature::MINT }.abi_encode(),
    )
    .unwrap();
    assert_eq!(paused_mint, Bytes::from(true.abi_encode()));

    let paused_transfer = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::isPausedCall { feature: IH20::PausableFeature::TRANSFER }.abi_encode(),
    )
    .unwrap();
    assert_eq!(paused_transfer, Bytes::from(false.abi_encode()));

    let features =
        op(&mut s, ALICE, FakePolicyAccounting::new(), IH20::pausedFeaturesCall {}.abi_encode())
            .unwrap();
    assert_eq!(features, Bytes::from(vec![IH20::PausableFeature::MINT].abi_encode()));
}

#[test]
fn golden_read_policy_id_and_unsupported_scope() {
    let mut s = fresh();
    let ok = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::policyIdCall { policyScope: H20PolicyType::TransferSender.id() }.abi_encode(),
    )
    .unwrap();
    assert_eq!(ok, Bytes::from(0u64.abi_encode()));

    let bad_scope = B256::repeat_byte(0xEE);
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::policyIdCall { policyScope: bad_scope }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::UnsupportedPolicyType { policyScope: bad_scope })
    );
}

#[test]
fn golden_read_domain_separator() {
    let mut s = fresh();
    let expected = domain_separator(&mut s);
    let out =
        op(&mut s, ALICE, FakePolicyAccounting::new(), IH20::DOMAIN_SEPARATORCall {}.abi_encode())
            .unwrap();
    assert_eq!(out, Bytes::from(expected.abi_encode()));
    assert_root("read_domain_separator", s, ROOT_FRESH);
}

#[test]
fn golden_read_eip712_domain() {
    let mut s = fresh();
    let out =
        op(&mut s, ALICE, FakePolicyAccounting::new(), IH20::eip712DomainCall {}.abi_encode())
            .unwrap();
    let decoded = IH20::eip712DomainCall::abi_decode_returns(&out).unwrap();
    assert_eq!(decoded.name, NAME);
    assert_eq!(decoded.version, "1");
    assert_eq!(decoded.chainId, U256::from(CHAIN_ID));
    assert_eq!(decoded.verifyingContract, TOKEN);
    assert_eq!(decoded.fields, alloy_primitives::FixedBytes::<1>::from([0x0f]));
}

// ============================================================================
// direct + constant reads
// ============================================================================

#[test]
fn golden_read_metadata_and_supply() {
    let mut s = fresh();
    let cases: Vec<(Vec<u8>, Bytes)> = vec![
        (IH20::nameCall {}.abi_encode(), Bytes::from(NAME.abi_encode())),
        (IH20::symbolCall {}.abi_encode(), Bytes::from(SYMBOL.abi_encode())),
        (IH20::decimalsCall {}.abi_encode(), Bytes::from(u(6).abi_encode())),
        (IH20::totalSupplyCall {}.abi_encode(), Bytes::from(U256::ZERO.abi_encode())),
        (IH20::supplyCapCall {}.abi_encode(), Bytes::from(H20_MAX_SUPPLY_CAP.abi_encode())),
        (IH20::contractURICall {}.abi_encode(), Bytes::from(String::new().abi_encode())),
        (IH20::balanceOfCall { account: ALICE }.abi_encode(), Bytes::from(U256::ZERO.abi_encode())),
        (
            IH20::allowanceCall { owner: ALICE, spender: BOB }.abi_encode(),
            Bytes::from(U256::ZERO.abi_encode()),
        ),
        (IH20::noncesCall { owner: ALICE }.abi_encode(), Bytes::from(U256::ZERO.abi_encode())),
        (
            IH20::hasRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
            Bytes::from(false.abi_encode()),
        ),
        (
            IH20::getRoleAdminCall { role: H20TokenRole::Mint.id() }.abi_encode(),
            Bytes::from(B256::ZERO.abi_encode()),
        ),
    ];
    for (calldata, expected) in cases {
        let out = op(&mut s, ALICE, FakePolicyAccounting::new(), calldata).unwrap();
        assert_eq!(out, expected);
    }
    assert_root("read_metadata", s, ROOT_FRESH);
}

#[test]
fn golden_read_role_and_policy_constants() {
    let mut s = fresh();
    let cases: Vec<(Vec<u8>, B256)> = vec![
        (IH20::DEFAULT_ADMIN_ROLECall {}.abi_encode(), H20TokenRole::DefaultAdmin.id()),
        (IH20::MINT_ROLECall {}.abi_encode(), H20TokenRole::Mint.id()),
        (IH20::BURN_ROLECall {}.abi_encode(), H20TokenRole::Burn.id()),
        (IH20::BURN_BLOCKED_ROLECall {}.abi_encode(), H20TokenRole::BurnBlocked.id()),
        (IH20::PAUSE_ROLECall {}.abi_encode(), H20TokenRole::Pause.id()),
        (IH20::UNPAUSE_ROLECall {}.abi_encode(), H20TokenRole::Unpause.id()),
        (IH20::METADATA_ROLECall {}.abi_encode(), H20TokenRole::Metadata.id()),
        (IH20::TRANSFER_SENDER_POLICYCall {}.abi_encode(), H20PolicyType::TransferSender.id()),
        (IH20::TRANSFER_RECEIVER_POLICYCall {}.abi_encode(), H20PolicyType::TransferReceiver.id()),
        (IH20::TRANSFER_EXECUTOR_POLICYCall {}.abi_encode(), H20PolicyType::TransferExecutor.id()),
        (IH20::MINT_RECEIVER_POLICYCall {}.abi_encode(), H20PolicyType::MintReceiver.id()),
    ];
    for (calldata, expected) in cases {
        let out = op(&mut s, ALICE, FakePolicyAccounting::new(), calldata).unwrap();
        assert_eq!(out, Bytes::from(expected.abi_encode()));
    }
    assert_root("read_constants", s, ROOT_FRESH);
}

// ============================================================================
// dispatch envelope (full path: nonpayable / uninitialized / pre-Beryl)
// ============================================================================

#[test]
fn dispatch_rejects_nonzero_value() {
    let mut s = fresh();
    let calldata = IH20::balanceOfCall { account: ALICE }.abi_encode();
    s.set_call_value(U256::ONE);
    let out = StorageCtx::enter(&mut s, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        )
        .dispatch_with_observer(ctx, &calldata, H20Spec::Beryl, NoopPrecompileCallObserver)
    })
    .expect("dispatch must not fatally error");
    assert!(out.is_revert());
    assert_eq!(out.bytes, Bytes::from(IH20::NonPayable {}.abi_encode()));
}

#[test]
fn dispatch_reverts_before_beryl() {
    let mut s = fresh();
    let calldata = IH20::balanceOfCall { account: ALICE }.abi_encode();
    let out = StorageCtx::enter(&mut s, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        )
        .dispatch_with_observer(
            ctx,
            &calldata,
            H20Spec::Disabled,
            NoopPrecompileCallObserver,
        )
    })
    .expect("dispatch must not fatally error");
    assert!(out.is_revert());
    assert!(out.bytes.is_empty());
}

#[test]
fn dispatch_reverts_when_uninitialized() {
    // No `fresh()` init and no marker bytecode => is_initialized is false.
    let mut s = HashMapStorageProvider::new(CHAIN_ID);
    let calldata = IH20::balanceOfCall { account: ALICE }.abi_encode();
    let out = StorageCtx::enter(&mut s, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        )
        .dispatch_with_observer(ctx, &calldata, H20Spec::Beryl, NoopPrecompileCallObserver)
    })
    .expect("dispatch must not fatally error");
    assert!(out.is_revert());
    assert!(out.bytes.is_empty());
}

// ============================================================================
// additional branch coverage: unprivileged auth guards + revert edges
// ============================================================================

#[test]
fn golden_transfer_reverts_zero_sender() {
    let mut s = fresh();
    // caller (the sender) is the zero address.
    let err = op_privileged(
        &mut s,
        Address::ZERO,
        FakePolicyAccounting::new(),
        IH20::transferCall { to: BOB, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidSender { sender: Address::ZERO }));
}

#[test]
fn golden_transfer_from_reverts_zero_receiver() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: ALICE, to: Address::ZERO, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidReceiver { receiver: Address::ZERO }));
}

#[test]
fn golden_transfer_from_reverts_zero_sender() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::transferFromCall { from: Address::ZERO, to: CAROL, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidSender { sender: Address::ZERO }));
}

#[test]
fn golden_approve_reverts_zero_approver() {
    let mut s = fresh();
    // caller (the approver) is the zero address.
    let err = op(
        &mut s,
        Address::ZERO,
        FakePolicyAccounting::new(),
        IH20::approveCall { spender: BOB, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidApprover { approver: Address::ZERO }));
}

#[test]
fn golden_mint_reverts_zero_receiver() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::mintCall { to: Address::ZERO, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::InvalidReceiver { receiver: Address::ZERO }));
}

#[test]
fn golden_burn_reverts_insufficient_balance() {
    let mut s = fresh();
    seed(&mut s, |t| {
        fund(t, ALICE, u(10));
        give_role(t, H20TokenRole::Burn.id(), ALICE);
    });
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::burnCall { amount: u(50) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::InsufficientBalance {
            sender: ALICE,
            balance: u(10),
            needed: u(50),
        })
    );
}

#[test]
fn golden_burn_blocked_unprivileged_requires_role() {
    let mut s = fresh();
    seed(&mut s, |t| fund(t, ALICE, u(100)));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::burnBlockedCall { from: BOB, amount: u(1) }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::BurnBlocked.id(),
        })
    );
}

#[test]
fn golden_unpause_reverts_empty_feature_set() {
    let mut s = fresh();
    let err = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::unpauseCall { features: vec![] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::EmptyFeatureSet {}));
}

#[test]
fn golden_unpause_unprivileged_requires_role() {
    let mut s = fresh();
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::unpauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::Unpause.id(),
        })
    );
}

/// Asserts an unprivileged metadata/admin op reverts for a caller lacking `role`.
#[track_caller]
fn assert_unprivileged_requires_role(calldata: Vec<u8>, role: B256) {
    let mut s = fresh();
    let err = op(&mut s, ALICE, FakePolicyAccounting::new(), calldata).unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: role,
        })
    );
}

#[test]
fn golden_update_supply_cap_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20::updateSupplyCapCall { newSupplyCap: u(1) }.abi_encode(),
        H20TokenRole::DefaultAdmin.id(),
    );
}

#[test]
fn golden_update_name_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20::updateNameCall { newName: "x".into() }.abi_encode(),
        H20TokenRole::Metadata.id(),
    );
}

#[test]
fn golden_update_symbol_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20::updateSymbolCall { newSymbol: "x".into() }.abi_encode(),
        H20TokenRole::Metadata.id(),
    );
}

#[test]
fn golden_update_contract_uri_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20::updateContractURICall { newURI: "x".into() }.abi_encode(),
        H20TokenRole::Metadata.id(),
    );
}

#[test]
fn golden_update_policy_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20::updatePolicyCall { policyScope: H20PolicyType::TransferSender.id(), newPolicyId: 1 }
            .abi_encode(),
        H20TokenRole::DefaultAdmin.id(),
    );
}

#[test]
fn golden_grant_role_unprivileged_no_admin_reverts() {
    // No admin exists yet → the admin-availability guard reverts.
    let mut s = fresh();
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::grantRoleCall { role: H20TokenRole::Mint.id(), account: BOB }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::DefaultAdmin.id(),
        })
    );
}

#[test]
fn golden_grant_role_unprivileged_non_admin_caller_reverts() {
    // An admin exists, but ALICE is not the role's admin → the role check reverts.
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20::grantRoleCall { role: H20TokenRole::Mint.id(), account: BOB }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: ALICE,
            neededRole: H20TokenRole::DefaultAdmin.id(),
        })
    );
}

#[test]
fn golden_revoke_role_unprivileged_non_admin_caller_reverts() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let err = op(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::revokeRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: BOB,
            neededRole: H20TokenRole::DefaultAdmin.id(),
        })
    );
}

#[test]
fn golden_set_role_admin_unprivileged_non_admin_caller_reverts() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let err = op(
        &mut s,
        BOB,
        FakePolicyAccounting::new(),
        IH20::setRoleAdminCall {
            role: H20TokenRole::Mint.id(),
            newAdminRole: H20TokenRole::Metadata.id(),
        }
        .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::AccessControlUnauthorizedAccount {
            account: BOB,
            neededRole: H20TokenRole::DefaultAdmin.id(),
        })
    );
}

#[test]
fn golden_renounce_role_reverts_last_admin() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let err = op(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::renounceRoleCall { role: H20TokenRole::DefaultAdmin.id(), callerConfirmation: ADMIN }
            .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20::LastAdminCannotRenounce {}));
}

#[test]
fn golden_grant_default_admin_bumps_member_count() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN));
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::grantRoleCall { role: H20TokenRole::DefaultAdmin.id(), account: ALICE }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| {
        assert!(t.has_role(H20TokenRole::DefaultAdmin.id(), ALICE).unwrap());
        assert_eq!(t.role_member_count(H20TokenRole::DefaultAdmin.id()).unwrap(), u(2));
    });
    assert_eq!(last_topic0(&s), IH20::RoleGranted::SIGNATURE_HASH);
    assert_root("grant_default_admin", s, ROOT_GRANT_DEFAULT_ADMIN);
}

#[test]
fn golden_grant_role_idempotent_when_already_held() {
    let mut s = fresh();
    seed(&mut s, |t| {
        give_role(t, H20TokenRole::DefaultAdmin.id(), ADMIN);
        give_role(t, H20TokenRole::Mint.id(), ALICE);
    });
    // ALICE already holds MINT_ROLE → grant is a no-op (no event, count unchanged).
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::grantRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    // ALICE still holds MINT_ROLE; the grant emitted nothing (early return).
    read(&mut s, |t| assert!(t.has_role(H20TokenRole::Mint.id(), ALICE).unwrap()));
    assert_root("grant_idempotent", s, ROOT_GRANT_IDEMPOTENT);
}

#[test]
fn golden_revoke_role_noop_when_not_held() {
    let mut s = fresh();
    // ALICE does not hold MINT_ROLE → revoke is a no-op; state stays at fresh.
    let out = op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::revokeRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
    )
    .unwrap();

    assert!(out.is_empty());
    read(&mut s, |t| assert!(!t.has_role(H20TokenRole::Mint.id(), ALICE).unwrap()));
    assert_root("revoke_noop", s, ROOT_FRESH);
}

// ============================================================================
// dispatch harness wrappers (no-observer dispatch, version-resolver gating, factory bootstrap)
// ============================================================================

#[test]
fn golden_dispatch_no_observer_wrapper_reverts_uninitialized() {
    // Exercises the no-observer `dispatch()` wrapper + the is_initialized=false gate.
    let mut s = HashMapStorageProvider::new(CHAIN_ID);
    s.set_caller(ALICE);
    let calldata = IH20::balanceOfCall { account: ALICE }.abi_encode();
    let out = StorageCtx::enter(&mut s, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        )
        .dispatch(ctx, &calldata, H20Spec::Beryl)
    })
    .expect("dispatch must not fatally error");
    assert!(out.is_revert());
}

#[test]
fn golden_inner_reverts_before_beryl() {
    // Exercises the version-resolution None branch (pre-introduction fork): before Beryl no
    // version is active, so the resolver-gated path reverts without routing.
    let mut s = fresh();
    let calldata = IH20::balanceOfCall { account: ALICE }.abi_encode();
    let err = StorageCtx::enter(&mut s, |ctx| {
        let mut token = H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        );
        AssetVersions::from_spec(H20Spec::Disabled).map_or_else(
            || Err(H20PrecompileError::Revert(Bytes::new())),
            |version| token.route(ctx, &calldata, version, false, NoopPrecompileCallObserver),
        )
    })
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::Revert(Bytes::new()));
}

#[test]
fn golden_grant_role_unchecked_bootstraps_first_admin() {
    // The factory bootstrap path: grants DEFAULT_ADMIN with no caller-auth check.
    let mut s = fresh();
    s.set_caller(TOKEN);
    StorageCtx::enter(&mut s, |ctx| {
        let mut token = H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            FakePolicyAccounting::new(),
            PolicyVersion::V1,
        );
        token.grant_role_unchecked(H20TokenRole::DefaultAdmin.id(), ADMIN, TOKEN).unwrap();
    });
    read(&mut s, |t| {
        assert!(t.has_role(H20TokenRole::DefaultAdmin.id(), ADMIN).unwrap());
        assert_eq!(t.role_member_count(H20TokenRole::DefaultAdmin.id()).unwrap(), U256::ONE);
    });
    assert_eq!(last_topic0(&s), IH20::RoleGranted::SIGNATURE_HASH);
    assert_root("grant_unchecked", s, ROOT_GRANT_UNCHECKED);
}

// ============================================================================
// asset reads: OPERATOR_ROLE / WAD / multiplier / scaled balances / metadata
// ============================================================================

#[test]
fn golden_read_operator_role_and_wad() {
    let mut s = fresh();
    let role = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::OPERATOR_ROLECall {}.abi_encode(),
    )
    .unwrap();
    assert_eq!(role, Bytes::from(operator_role().abi_encode()));
    let wad = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::WAD_PRECISIONCall {}.abi_encode(),
    )
    .unwrap();
    assert_eq!(wad, Bytes::from(H20AssetStorage::WAD.abi_encode()));
    assert_root("read_operator_wad", s, ROOT_FRESH);
}

#[test]
fn golden_read_multiplier_and_scaled_balances() {
    let mut s = fresh();
    // A doubled multiplier exercises the `* multiplier / WAD` and `* WAD / multiplier` paths.
    seed(&mut s, |t| {
        fund(t, ALICE, u(100));
        t.set_multiplier(H20AssetStorage::WAD * u(2)).unwrap();
    });
    let m =
        op(&mut s, ALICE, FakePolicyAccounting::new(), IH20Asset::multiplierCall {}.abi_encode())
            .unwrap();
    assert_eq!(m, Bytes::from((H20AssetStorage::WAD * u(2)).abi_encode()));

    let scaled = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::toScaledBalanceCall { rawBalance: u(100) }.abi_encode(),
    )
    .unwrap();
    assert_eq!(scaled, Bytes::from(u(200).abi_encode()));

    let raw = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::toRawBalanceCall { scaledBalance: u(200) }.abi_encode(),
    )
    .unwrap();
    assert_eq!(raw, Bytes::from(u(100).abi_encode()));

    let sbo = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::scaledBalanceOfCall { account: ALICE }.abi_encode(),
    )
    .unwrap();
    assert_eq!(sbo, Bytes::from(u(200).abi_encode()));

    assert_root("read_multiplier_scaled", s, ROOT_MULTIPLIER_SCALED);
}

#[test]
fn golden_read_is_announcement_id_used() {
    let mut s = fresh();
    let unused = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::isAnnouncementIdUsedCall { id: "evt-1".into() }.abi_encode(),
    )
    .unwrap();
    assert_eq!(unused, Bytes::from(false.abi_encode()));

    seed(&mut s, |t| t.mark_announcement_id_used("evt-1").unwrap());
    let used = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::isAnnouncementIdUsedCall { id: "evt-1".into() }.abi_encode(),
    )
    .unwrap();
    assert_eq!(used, Bytes::from(true.abi_encode()));
    assert_root("read_announcement_id_used", s, ROOT_ANNOUNCE_ID_USED);
}

#[test]
fn golden_read_extra_metadata() {
    let mut s = fresh();
    let empty = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::extraMetadataCall { key: "category".into() }.abi_encode(),
    )
    .unwrap();
    assert_eq!(empty, Bytes::from(String::new().abi_encode()));

    seed(&mut s, |t| t.set_extra_metadata_value("category", "commodity".into()).unwrap());
    let set = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::extraMetadataCall { key: "category".into() }.abi_encode(),
    )
    .unwrap();
    assert_eq!(set, Bytes::from("commodity".abi_encode()));
    assert_root("read_extra_metadata", s, ROOT_EXTRA_METADATA_READ);
}

// ============================================================================
// updateMultiplier
// ============================================================================

#[test]
fn golden_update_multiplier() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::updateMultiplierCall { newMultiplier: H20AssetStorage::WAD * u(2) }.abi_encode(),
    )
    .unwrap();
    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.multiplier().unwrap(), H20AssetStorage::WAD * u(2)));
    assert_eq!(
        *s.get_events(TOKEN).last().unwrap(),
        IH20Asset::MultiplierUpdated { multiplier: H20AssetStorage::WAD * u(2) }.encode_log_data()
    );
    assert_root("update_multiplier", s, ROOT_UPDATE_MULTIPLIER);
}

#[test]
fn golden_update_multiplier_reverts_zero() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::updateMultiplierCall { newMultiplier: U256::ZERO }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20Asset::InvalidMultiplier {}));
}

#[test]
fn golden_update_multiplier_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20Asset::updateMultiplierCall { newMultiplier: H20AssetStorage::WAD }.abi_encode(),
        operator_role(),
    );
}

// ============================================================================
// batchMint
// ============================================================================

#[test]
fn golden_batch_mint() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let mut policy = FakePolicyAccounting::new();
    policy.allow(0, BOB);
    policy.allow(0, CAROL);
    let out = op(
        &mut s,
        ALICE,
        policy,
        IH20Asset::batchMintCall { recipients: vec![BOB, CAROL], amounts: vec![u(30), u(70)] }
            .abi_encode(),
    )
    .unwrap();
    assert!(out.is_empty());
    read(&mut s, |t| {
        assert_eq!(t.balance_of(BOB).unwrap(), u(30));
        assert_eq!(t.balance_of(CAROL).unwrap(), u(70));
        assert_eq!(t.total_supply().unwrap(), u(100));
    });
    assert_root("batch_mint", s, ROOT_BATCH_MINT);
}

#[test]
fn golden_batch_mint_reverts_length_mismatch() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::batchMintCall { recipients: vec![BOB, CAROL], amounts: vec![u(30)] }
            .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20Asset::LengthMismatch { leftLen: u(2), rightLen: u(1) })
    );
}

#[test]
fn golden_batch_mint_reverts_empty() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::batchMintCall { recipients: vec![], amounts: vec![] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20Asset::EmptyBatch {}));
}

#[test]
fn golden_batch_mint_reverts_when_paused() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Mint.id(), ALICE));
    op_privileged(
        &mut s,
        ADMIN,
        FakePolicyAccounting::new(),
        IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
    )
    .unwrap();
    let err = op(
        &mut s,
        ALICE,
        allow0(BOB),
        IH20Asset::batchMintCall { recipients: vec![BOB], amounts: vec![u(1)] }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20::ContractPaused { feature: IH20::PausableFeature::MINT })
    );
}

#[test]
fn golden_batch_mint_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20Asset::batchMintCall { recipients: vec![BOB], amounts: vec![u(1)] }.abi_encode(),
        H20TokenRole::Mint.id(),
    );
}

// ============================================================================
// updateExtraMetadata
// ============================================================================

#[test]
fn golden_update_extra_metadata_set() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Metadata.id(), ALICE));
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::updateExtraMetadataCall { key: "category".into(), value: "commodity".into() }
            .abi_encode(),
    )
    .unwrap();
    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.extra_metadata("category").unwrap(), "commodity"));
    assert_eq!(
        *s.get_events(TOKEN).last().unwrap(),
        IH20Asset::ExtraMetadataUpdated { key: "category".into(), value: "commodity".into() }
            .encode_log_data()
    );
    assert_root("update_extra_metadata_set", s, ROOT_METADATA_SET);
}

#[test]
fn golden_update_extra_metadata_remove() {
    let mut s = fresh();
    seed(&mut s, |t| {
        give_role(t, H20TokenRole::Metadata.id(), ALICE);
        t.set_extra_metadata_value("category", "commodity".into()).unwrap();
    });
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::updateExtraMetadataCall { key: "category".into(), value: String::new() }
            .abi_encode(),
    )
    .unwrap();
    assert!(out.is_empty());
    read(&mut s, |t| assert_eq!(t.extra_metadata("category").unwrap(), ""));
    assert_eq!(
        *s.get_events(TOKEN).last().unwrap(),
        IH20Asset::ExtraMetadataUpdated { key: "category".into(), value: String::new() }
            .encode_log_data()
    );
    assert_root("update_extra_metadata_remove", s, ROOT_METADATA_REMOVE);
}

#[test]
fn golden_update_extra_metadata_reverts_empty_key() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, H20TokenRole::Metadata.id(), ALICE));
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::updateExtraMetadataCall { key: String::new(), value: "x".into() }.abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20Asset::InvalidMetadataKey {}));
}

#[test]
fn golden_update_extra_metadata_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20Asset::updateExtraMetadataCall { key: "category".into(), value: "x".into() }
            .abi_encode(),
        H20TokenRole::Metadata.id(),
    );
}

// ============================================================================
// announce (posts announcement, atomically runs internalCalls)
// ============================================================================

#[test]
fn golden_announce_emits_and_runs_internal_calls() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    // An internal `updateMultiplier` runs under the operator role ALICE already holds.
    let inner =
        IH20Asset::updateMultiplierCall { newMultiplier: H20AssetStorage::WAD * u(2) }.abi_encode();
    let out = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::announceCall {
            internalCalls: vec![Bytes::from(inner)],
            id: "2026-split".into(),
            description: "2:1 split".into(),
            uri: "ipfs://split".into(),
        }
        .abi_encode(),
    )
    .unwrap();
    assert!(out.is_empty());
    read(&mut s, |t| {
        assert!(t.is_announcement_id_used("2026-split").unwrap());
        assert_eq!(t.multiplier().unwrap(), H20AssetStorage::WAD * u(2));
    });
    let events = s.get_events(TOKEN);
    // Announcement, MultiplierUpdated (internal call), EndAnnouncement.
    assert_eq!(
        events[events.len() - 3],
        IH20Asset::Announcement {
            caller: ALICE,
            id: "2026-split".into(),
            description: "2:1 split".into(),
            uri: "ipfs://split".into(),
        }
        .encode_log_data()
    );
    assert_eq!(events[events.len() - 2].topics()[0], IH20Asset::MultiplierUpdated::SIGNATURE_HASH);
    assert_eq!(
        *events.last().unwrap(),
        IH20Asset::EndAnnouncement { id: "2026-split".into() }.encode_log_data()
    );
    assert_root("announce", s, ROOT_ANNOUNCE);
}

#[test]
fn golden_announce_reverts_id_already_used() {
    let mut s = fresh();
    seed(&mut s, |t| {
        give_role(t, operator_role(), ALICE);
        t.mark_announcement_id_used("dup").unwrap();
    });
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::announceCall {
            internalCalls: vec![],
            id: "dup".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20Asset::AnnouncementIdAlreadyUsed { id: "dup".into() })
    );
}

#[test]
fn golden_announce_reverts_internal_call_malformed() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    let malformed = Bytes::from(vec![0x01u8, 0x02]);
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::announceCall {
            internalCalls: vec![malformed.clone()],
            id: "x".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        H20PrecompileError::revert(IH20Asset::InternalCallMalformed { call: malformed })
    );
}

#[test]
fn golden_announce_reverts_nested_announce() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    let nested = Bytes::from(
        IH20Asset::announceCall {
            internalCalls: vec![],
            id: "inner".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
    );
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::announceCall {
            internalCalls: vec![nested],
            id: "outer".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20Asset::AnnouncementInProgress {}));
}

#[test]
fn golden_announce_reverts_internal_call_failed() {
    let mut s = fresh();
    seed(&mut s, |t| give_role(t, operator_role(), ALICE));
    // A valid selector whose business logic reverts (zero multiplier) => wrapped.
    let inner =
        Bytes::from(IH20Asset::updateMultiplierCall { newMultiplier: U256::ZERO }.abi_encode());
    let err = op(
        &mut s,
        ALICE,
        FakePolicyAccounting::new(),
        IH20Asset::announceCall {
            internalCalls: vec![inner.clone()],
            id: "x".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
    )
    .unwrap_err();
    assert_eq!(err, H20PrecompileError::revert(IH20Asset::InternalCallFailed { call: inner }));
}

#[test]
fn golden_announce_unprivileged_requires_role() {
    assert_unprivileged_requires_role(
        IH20Asset::announceCall {
            internalCalls: vec![],
            id: "x".into(),
            description: String::new(),
            uri: String::new(),
        }
        .abi_encode(),
        operator_role(),
    );
}

// ============================================================================
// gas: storage-access footprint per op
// ============================================================================
//
// `gas_deducted` is 0 under the test gas schedule, so we pin the deterministic,
// schedule-independent signal instead: the SLOAD / SSTORE / KECCAK256 op counts a
// call performs. These are the storage-access footprint that drives real gas, so a
// change here (e.g. an extra SLOAD in V1) is caught even when bytes/state/events match.

/// Runs `calldata` privileged after `setup`, returning `(sload, sstore, keccak256)` counts.
fn gas(
    setup: impl FnOnce(&mut H20AssetStorage<'_>),
    caller: Address,
    policy: FakePolicyAccounting,
    calldata: Vec<u8>,
) -> (u64, u64, u64) {
    let mut s = fresh();
    seed(&mut s, setup);
    s.set_caller(caller);
    s.reset_counters();
    StorageCtx::enter(&mut s, |ctx| {
        H20AssetToken::with_storage_and_policy(
            H20AssetStorage::from_address(TOKEN, ctx),
            policy,
            PolicyVersion::V1,
        )
        .route(ctx, &calldata, AssetVersion::V1, true, NoopPrecompileCallObserver)
    })
    .expect("gas-footprint op must succeed");
    (s.counter_sload(), s.counter_sstore(), s.counter_keccak256())
}

/// An `FakePolicyAccounting` authorizing `who` under the default (0) scope.
fn allow0(who: Address) -> FakePolicyAccounting {
    let mut p = FakePolicyAccounting::new();
    p.allow(0, who);
    p
}

#[test]
fn golden_gas_footprints() {
    let actual: Vec<(&str, (u64, u64, u64))> = vec![
        (
            "transfer",
            gas(
                |t| fund(t, ALICE, u(100)),
                ALICE,
                FakePolicyAccounting::new(),
                IH20::transferCall { to: BOB, amount: u(30) }.abi_encode(),
            ),
        ),
        (
            "transfer_from",
            gas(
                |t| {
                    fund(t, ALICE, u(100));
                    t.set_allowance(ALICE, BOB, u(40)).unwrap();
                },
                BOB,
                FakePolicyAccounting::new(),
                IH20::transferFromCall { from: ALICE, to: BOB, amount: u(30) }.abi_encode(),
            ),
        ),
        (
            "approve",
            gas(
                |_t| {},
                ALICE,
                FakePolicyAccounting::new(),
                IH20::approveCall { spender: BOB, amount: u(50) }.abi_encode(),
            ),
        ),
        (
            "mint",
            gas(
                |_t| {},
                ADMIN,
                allow0(BOB),
                IH20::mintCall { to: BOB, amount: u(100) }.abi_encode(),
            ),
        ),
        (
            "burn",
            gas(
                |t| {
                    fund(t, ALICE, u(100));
                    give_role(t, H20TokenRole::Burn.id(), ALICE);
                },
                ALICE,
                FakePolicyAccounting::new(),
                IH20::burnCall { amount: u(40) }.abi_encode(),
            ),
        ),
        (
            "burn_blocked",
            gas(
                |t| {
                    fund(t, ALICE, u(100));
                    t.set_policy_id(H20PolicyType::TransferSender.id(), POLICY_ID).unwrap();
                },
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::burnBlockedCall { from: ALICE, amount: u(40) }.abi_encode(),
            ),
        ),
        (
            "pause",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::pauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
            ),
        ),
        (
            "unpause",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::unpauseCall { features: vec![IH20::PausableFeature::MINT] }.abi_encode(),
            ),
        ),
        (
            "update_supply_cap",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::updateSupplyCapCall { newSupplyCap: u(1_000) }.abi_encode(),
            ),
        ),
        (
            "update_name",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::updateNameCall { newName: "New Name".into() }.abi_encode(),
            ),
        ),
        (
            "update_symbol",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::updateSymbolCall { newSymbol: "USDX".into() }.abi_encode(),
            ),
        ),
        (
            "update_contract_uri",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::updateContractURICall { newURI: "ipfs://x".into() }.abi_encode(),
            ),
        ),
        (
            "grant_role",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::grantRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
            ),
        ),
        (
            "revoke_role",
            gas(
                |t| give_role(t, H20TokenRole::Mint.id(), ALICE),
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::revokeRoleCall { role: H20TokenRole::Mint.id(), account: ALICE }.abi_encode(),
            ),
        ),
        (
            "set_role_admin",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20::setRoleAdminCall {
                    role: H20TokenRole::Mint.id(),
                    newAdminRole: H20TokenRole::Metadata.id(),
                }
                .abi_encode(),
            ),
        ),
        (
            "update_policy",
            gas(
                |_t| {},
                ADMIN,
                {
                    let mut p = FakePolicyAccounting::new();
                    p.create_existing_policy(7);
                    p
                },
                IH20::updatePolicyCall {
                    policyScope: H20PolicyType::TransferSender.id(),
                    newPolicyId: 7,
                }
                .abi_encode(),
            ),
        ),
        (
            "update_multiplier",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20Asset::updateMultiplierCall { newMultiplier: H20AssetStorage::WAD * u(2) }
                    .abi_encode(),
            ),
        ),
        (
            "batch_mint",
            gas(
                |_t| {},
                ADMIN,
                {
                    let mut p = FakePolicyAccounting::new();
                    p.allow(0, BOB);
                    p.allow(0, CAROL);
                    p
                },
                IH20Asset::batchMintCall {
                    recipients: vec![BOB, CAROL],
                    amounts: vec![u(30), u(70)],
                }
                .abi_encode(),
            ),
        ),
        (
            "announce",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20Asset::announceCall {
                    internalCalls: vec![],
                    id: "gas".into(),
                    description: String::new(),
                    uri: String::new(),
                }
                .abi_encode(),
            ),
        ),
        (
            "update_extra_metadata",
            gas(
                |_t| {},
                ADMIN,
                FakePolicyAccounting::new(),
                IH20Asset::updateExtraMetadataCall {
                    key: "category".into(),
                    value: "commodity".into(),
                }
                .abi_encode(),
            ),
        ),
    ];

    let expected: &[(&str, (u64, u64, u64))] = &[
        ("transfer", (3, 2, 0)),
        ("transfer_from", (4, 3, 0)),
        ("approve", (0, 1, 0)),
        ("mint", (5, 2, 0)),
        ("burn", (4, 2, 0)),
        ("burn_blocked", (4, 2, 0)),
        ("pause", (1, 1, 0)),
        ("unpause", (1, 1, 0)),
        ("update_supply_cap", (2, 1, 0)),
        ("update_name", (0, 1, 0)),
        ("update_symbol", (0, 1, 0)),
        ("update_contract_uri", (0, 1, 0)),
        ("grant_role", (1, 1, 0)),
        ("revoke_role", (1, 1, 0)),
        ("set_role_admin", (1, 1, 0)),
        ("update_policy", (2, 1, 0)),
        ("update_multiplier", (0, 1, 0)),
        ("batch_mint", (11, 4, 0)),
        ("announce", (1, 1, 0)),
        ("update_extra_metadata", (0, 1, 0)),
    ];

    bless_or_assert_gas(&actual, expected);
}

// ============================================================================
// meta: op coverage checklist
// ============================================================================

/// Compile-time coverage checklist — never called; it exists only for its two
/// exhaustive `match`es (no `_` arm), each arm naming the golden `#[test]` fn(s) that
/// pin the op via [`covered`].
///
/// This gives two compile-time guarantees:
///   * add an op to the ABI (a new `IH20Calls` / `IH20AssetCalls` variant) → the wildcard-free
///     match fails to build until an arm (and thus a golden) is added;
///   * rename or remove a golden `#[test]` fn → the `covered(&[...])` reference fails to build.
///
/// Because Asset V1 is **frozen**, this checklist is NOT expected to ever be
/// updated: a compile error here means the frozen V1 op surface changed, which must be
/// reviewed.
#[allow(dead_code)]
fn v1_op_coverage_checklist(call: IH20::IH20Calls, ext: IH20Asset::IH20AssetCalls) {
    use IH20::IH20Calls as C;
    use IH20Asset::IH20AssetCalls as SC;

    // No-op: forces each arm to name real golden `#[test]` fns by path.
    fn covered(_goldens: &[fn()]) {}

    match call {
        // ERC-20 core
        C::transfer(_) => covered(&[
            golden_transfer_privileged,
            golden_transfer_unprivileged_allowed,
            golden_transfer_unprivileged_blocked_sender_reverts,
            golden_transfer_reverts_zero_receiver,
            golden_transfer_reverts_insufficient_balance,
            golden_transfer_reverts_when_paused,
            golden_transfer_reverts_zero_sender,
        ]),
        C::transferFrom(_) => covered(&[
            golden_transfer_from_finite_allowance_decrements,
            golden_transfer_from_infinite_allowance_not_decremented,
            golden_transfer_from_reverts_insufficient_allowance,
            golden_transfer_from_unprivileged_enforces_executor_policy,
            golden_transfer_from_reverts_zero_receiver,
            golden_transfer_from_reverts_zero_sender,
        ]),
        C::approve(_) => covered(&[
            golden_approve_sets_allowance_and_emits,
            golden_approve_reverts_zero_spender,
            golden_approve_reverts_zero_approver,
        ]),
        C::transferWithMemo(_) => covered(&[golden_transfer_with_memo_emits_transfer_then_memo]),
        C::transferFromWithMemo(_) => covered(&[golden_transfer_from_with_memo]),

        // mint / burn
        C::mint(_) => covered(&[
            golden_mint_privileged_still_enforces_receiver_policy,
            golden_mint_unprivileged_requires_role_and_policy,
            golden_mint_reverts_over_supply_cap,
            golden_mint_reverts_zero_receiver,
        ]),
        C::mintWithMemo(_) => covered(&[golden_mint_with_memo]),
        C::burn(_) => covered(&[
            golden_burn_requires_role_then_reduces_supply,
            golden_burn_reverts_insufficient_balance,
        ]),
        C::burnWithMemo(_) => covered(&[golden_burn_with_memo]),
        C::burnBlocked(_) => covered(&[
            golden_burn_blocked_destroys_from_blocked_account,
            golden_burn_blocked_reverts_when_not_blocked,
            golden_burn_blocked_unprivileged_requires_role,
        ]),

        // pause / config / roles / policy / permit
        C::pause(_) => covered(&[
            golden_pause_sets_feature_bit,
            golden_pause_reverts_empty_feature_set,
            golden_pause_unprivileged_requires_role,
        ]),
        C::unpause(_) => covered(&[
            golden_unpause_clears_feature_bit,
            golden_unpause_reverts_empty_feature_set,
            golden_unpause_unprivileged_requires_role,
        ]),
        C::updateSupplyCap(_) => covered(&[
            golden_update_supply_cap,
            golden_update_supply_cap_reverts_below_supply,
            golden_update_supply_cap_unprivileged_requires_role,
        ]),
        C::updateName(_) => covered(&[
            golden_update_name_emits_name_and_domain_changed,
            golden_update_name_unprivileged_requires_role,
        ]),
        C::updateSymbol(_) => {
            covered(&[golden_update_symbol, golden_update_symbol_unprivileged_requires_role])
        }
        C::updateContractURI(_) => covered(&[
            golden_update_contract_uri,
            golden_update_contract_uri_unprivileged_requires_role,
        ]),
        C::grantRole(_) => covered(&[
            golden_grant_role,
            golden_grant_role_unprivileged_no_admin_reverts,
            golden_grant_role_unprivileged_non_admin_caller_reverts,
            golden_grant_default_admin_bumps_member_count,
            golden_grant_role_idempotent_when_already_held,
        ]),
        C::revokeRole(_) => covered(&[
            golden_revoke_role,
            golden_revoke_last_admin_rejected,
            golden_revoke_role_unprivileged_non_admin_caller_reverts,
            golden_revoke_role_noop_when_not_held,
        ]),
        C::renounceRole(_) => covered(&[
            golden_renounce_role,
            golden_renounce_role_bad_confirmation,
            golden_renounce_role_reverts_last_admin,
        ]),
        C::renounceLastAdmin(_) => {
            covered(&[golden_renounce_last_admin, golden_renounce_last_admin_reverts_when_not_sole])
        }
        C::setRoleAdmin(_) => covered(&[
            golden_set_role_admin,
            golden_set_role_admin_unprivileged_non_admin_caller_reverts,
        ]),
        C::updatePolicy(_) => covered(&[
            golden_update_policy,
            golden_update_policy_reverts_missing_policy,
            golden_update_policy_unprivileged_requires_role,
        ]),
        C::permit(_) => covered(&[
            golden_permit_sets_allowance_and_increments_nonce,
            golden_permit_reverts_when_expired,
        ]),

        // computed reads
        C::isPaused(_) | C::pausedFeatures(_) => {
            covered(&[golden_read_is_paused_and_paused_features])
        }
        C::policyId(_) => covered(&[golden_read_policy_id_and_unsupported_scope]),
        C::DOMAIN_SEPARATOR(_) => covered(&[golden_read_domain_separator]),
        C::eip712Domain(_) => covered(&[golden_read_eip712_domain]),

        // direct reads
        C::name(_) |
        C::symbol(_) |
        C::decimals(_) |
        C::totalSupply(_) |
        C::balanceOf(_) |
        C::allowance(_) |
        C::supplyCap(_) |
        C::nonces(_) |
        C::contractURI(_) |
        C::hasRole(_) |
        C::getRoleAdmin(_) => covered(&[golden_read_metadata_and_supply]),

        // role / policy-id constants
        C::DEFAULT_ADMIN_ROLE(_) |
        C::MINT_ROLE(_) |
        C::BURN_ROLE(_) |
        C::BURN_BLOCKED_ROLE(_) |
        C::PAUSE_ROLE(_) |
        C::UNPAUSE_ROLE(_) |
        C::METADATA_ROLE(_) |
        C::TRANSFER_SENDER_POLICY(_) |
        C::TRANSFER_RECEIVER_POLICY(_) |
        C::TRANSFER_EXECUTOR_POLICY(_) |
        C::MINT_RECEIVER_POLICY(_) => covered(&[golden_read_role_and_policy_constants]),
    }

    match ext {
        // asset-specific reads
        SC::OPERATOR_ROLE(_) | SC::WAD_PRECISION(_) => {
            covered(&[golden_read_operator_role_and_wad])
        }
        SC::multiplier(_) |
        SC::toScaledBalance(_) |
        SC::toRawBalance(_) |
        SC::scaledBalanceOf(_) => covered(&[golden_read_multiplier_and_scaled_balances]),
        SC::isAnnouncementIdUsed(_) => covered(&[golden_read_is_announcement_id_used]),
        SC::extraMetadata(_) => covered(&[golden_read_extra_metadata]),

        // asset-specific mutations
        SC::updateMultiplier(_) => covered(&[
            golden_update_multiplier,
            golden_update_multiplier_reverts_zero,
            golden_update_multiplier_unprivileged_requires_role,
        ]),
        SC::batchMint(_) => covered(&[
            golden_batch_mint,
            golden_batch_mint_reverts_length_mismatch,
            golden_batch_mint_reverts_empty,
            golden_batch_mint_reverts_when_paused,
            golden_batch_mint_unprivileged_requires_role,
        ]),
        SC::updateExtraMetadata(_) => covered(&[
            golden_update_extra_metadata_set,
            golden_update_extra_metadata_remove,
            golden_update_extra_metadata_reverts_empty_key,
            golden_update_extra_metadata_unprivileged_requires_role,
        ]),
        SC::announce(_) => covered(&[
            golden_announce_emits_and_runs_internal_calls,
            golden_announce_reverts_id_already_used,
            golden_announce_reverts_internal_call_malformed,
            golden_announce_reverts_nested_announce,
            golden_announce_reverts_internal_call_failed,
            golden_announce_unprivileged_requires_role,
        ]),
    }
}
