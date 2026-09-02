//! Generates the explicitly gated, test-only H20 network override.

use std::{env, fs, path::PathBuf};

const ENABLED: &str = "KONA_H20_TEST_OVERRIDE";
const EXTERNAL_REGISTRY_TEST: &str = "KONA_EXTERNAL_REGISTRY_TEST";
const L1_CHAIN_ID: &str = "KONA_H20_TEST_L1_CHAIN_ID";
const L2_CHAIN_ID: &str = "KONA_H20_TEST_L2_CHAIN_ID";
const ACTIVATION_TIME: &str = "KONA_H20_TEST_ACTIVATION_TIME";
const ACTIVATION_ADMIN: &str = "KONA_H20_TEST_ACTIVATION_ADMIN";

fn main() {
    for name in [
        ENABLED,
        EXTERNAL_REGISTRY_TEST,
        L1_CHAIN_ID,
        L2_CHAIN_ID,
        ACTIVATION_TIME,
        ACTIVATION_ADMIN,
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"))
        .join("h20_test_networks.rs");
    let enabled = env::var(ENABLED).is_ok_and(|value| value == "true");
    if !enabled {
        for name in [L1_CHAIN_ID, L2_CHAIN_ID, ACTIVATION_TIME, ACTIVATION_ADMIN] {
            assert!(
                !env::var(name).is_ok_and(|value| !value.is_empty()),
                "{name} requires the explicit test-only {ENABLED}=true gate"
            );
        }
        fs::write(output, "&[]\n").expect("write disabled H20 test override");
        return;
    }

    assert_eq!(
        env::var(EXTERNAL_REGISTRY_TEST).as_deref(),
        Ok("true"),
        "{ENABLED}=true is only valid with {EXTERNAL_REGISTRY_TEST}=true"
    );
    let l1_chain_id = parse_positive_u64(L1_CHAIN_ID);
    let l2_chain_id = parse_positive_u64(L2_CHAIN_ID);
    let activation_time = parse_positive_u64(ACTIVATION_TIME);
    let admin = parse_address(&required(ACTIVATION_ADMIN));
    let bytes = admin.iter().map(|byte| format!("0x{byte:02x}")).collect::<Vec<_>>().join(", ");
    let generated = format!(
        "&[H20NetworkConfig::new({l1_chain_id}, {l2_chain_id}, {activation_time}, Address::new([{bytes}]))]\n"
    );
    fs::write(output, generated).expect("write H20 test override");
}

fn required(name: &str) -> String {
    let value = env::var(name).unwrap_or_else(|_| panic!("{name} is required when {ENABLED}=true"));
    assert!(!value.is_empty(), "{name} is required when {ENABLED}=true");
    value
}

fn parse_positive_u64(name: &str) -> u64 {
    let value = required(name);
    let parsed = value
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be an unsigned decimal integer"));
    assert!(parsed > 0, "{name} must be non-zero");
    parsed
}

fn parse_address(value: &str) -> [u8; 20] {
    let hex = value.strip_prefix("0x").expect("KONA_H20_TEST_ACTIVATION_ADMIN must start with 0x");
    assert_eq!(hex.len(), 40, "KONA_H20_TEST_ACTIVATION_ADMIN must contain 20 bytes");
    let mut address = [0u8; 20];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).expect("address is ASCII");
        address[index] =
            u8::from_str_radix(text, 16).expect("KONA_H20_TEST_ACTIVATION_ADMIN must be hex");
    }
    assert!(
        address.iter().any(|byte| *byte != 0),
        "KONA_H20_TEST_ACTIVATION_ADMIN must be non-zero"
    );
    address
}
