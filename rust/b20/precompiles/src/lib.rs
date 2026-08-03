#![doc = "Base Beryl B20 v1 core precompiles for HSK."]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod macros;

mod spec;
pub use spec::B20Spec;

mod lookup;
pub use lookup::{BerylLookup, BerylLookupWithObserver};

mod activation;
pub use activation::{
    ActivationAdminConfig, ActivationFeature, ActivationRegistry, ActivationRegistryStorage,
    IActivationRegistry,
};

mod common;
pub use common::{
    B20_MAX_SUPPLY_CAP, B20CoreStorage, B20Guards, B20PausableFeature, B20PolicyType, B20TokenRole,
    Burnable, Configurable, Eip712Domain, IB20, Mintable, Pausable, PermitArgs, Permittable,
    RoleManaged, Token, TokenAccounting, Transferable,
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

mod b20_asset;
pub use b20_asset::{
    Asset, AssetAccounting, AssetV1, AssetVersion, AssetVersions, B20AssetExtensionStorage,
    B20AssetInit, B20AssetPrecompile, B20AssetStorage, B20AssetToken, IB20Asset,
};

mod b20_stablecoin;
pub use b20_stablecoin::{
    B20StablecoinExtensionStorage, B20StablecoinInit, B20StablecoinPrecompile,
    B20StablecoinStorage, B20StablecoinToken, IB20Stablecoin, Stablecoin, StablecoinAccounting,
    StablecoinV1, StablecoinVersion, StablecoinVersions,
};

mod b20_factory;
pub use b20_factory::{
    B20Factory, B20FactoryStorage, B20Variant, CommonParams, Factory, FactoryV1, FactoryVersion,
    FactoryVersions, IB20Factory, TokenCreateParams,
};

mod policy;
pub use policy::{
    IPolicyRegistry, PackedPolicy, PolicyAccounting, PolicyRegistryLogic, PolicyRegistryPrecompile,
    PolicyRegistryStorage, PolicyRegistryV1, PolicyVersion, PolicyVersions,
};
