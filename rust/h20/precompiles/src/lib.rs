#![doc = "HSK H20 v1 core precompiles for HSK."]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod macros;

mod spec;
pub use spec::H20Spec;

mod lookup;
pub use lookup::{BerylLookup, BerylLookupWithObserver};

mod activation;
pub use activation::{
    ActivationAdminConfig, ActivationFeature, ActivationRegistry, ActivationRegistryStorage,
    IActivationRegistry,
};

mod common;
pub use common::{
    Burnable, Configurable, Eip712Domain, H20_MAX_SUPPLY_CAP, H20CoreStorage, H20Guards,
    H20PausableFeature, H20PolicyType, H20TokenRole, IH20, Mintable, Pausable, PermitArgs,
    Permittable, RoleManaged, Token, TokenAccounting, Transferable,
};
#[cfg(any(test, feature = "test-utils"))]
pub use common::{FakePolicyAccounting, InMemoryTokenAccounting, TestStablecoinToken, TestToken};

mod observer;
pub use observer::{EndGuard, NoopPrecompileCallObserver, PrecompileCallObserver};

mod metrics;
pub use metrics::{
    BerylAuxiliaryMetrics, BerylCallOutcome, BerylCallRecorder, BerylCallTimer,
    BerylErrorClassifier, BerylErrorKind, BerylMetricLabels, BerylSelector, CALLDATA_WORD_GAS,
    PrecompileCallMetric, PrecompileCallOutcome, PrecompileCallStatus,
};

mod h20_asset;
pub use h20_asset::{
    Asset, AssetAccounting, AssetV1, AssetVersion, AssetVersions, H20AssetExtensionStorage,
    H20AssetInit, H20AssetPrecompile, H20AssetStorage, H20AssetToken, IH20Asset,
};

mod h20_stablecoin;
pub use h20_stablecoin::{
    H20StablecoinExtensionStorage, H20StablecoinInit, H20StablecoinPrecompile,
    H20StablecoinStorage, H20StablecoinToken, IH20Stablecoin, Stablecoin, StablecoinAccounting,
    StablecoinV1, StablecoinVersion, StablecoinVersions,
};

mod h20_factory;
pub use h20_factory::{
    CommonParams, Factory, FactoryV1, FactoryVersion, FactoryVersions, H20Factory,
    H20FactoryStorage, H20Variant, IH20Factory, TokenCreateParams,
};

mod policy;
pub use policy::{
    IPolicyRegistry, PackedPolicy, PolicyAccounting, PolicyRegistryLogic, PolicyRegistryPrecompile,
    PolicyRegistryStorage, PolicyRegistryV1, PolicyVersion, PolicyVersions,
};
