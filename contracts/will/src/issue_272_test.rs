#![cfg(test)]

//! Regression test for issue #272: `CONTRACT_VERSION` and the `contractmeta!`
//! `"Version"` string are two independent representations of the same
//! version, and nothing checked they stayed consistent. They had in fact
//! already drifted: `CONTRACT_VERSION` documents a baseline of "1.0.0" while
//! `contractmeta!` still said `"0.1.0"`, left over from before
//! `CONTRACT_VERSION` was introduced. `contractmeta!` requires its `val` to
//! be a string literal (it cannot be derived from `CONTRACT_VERSION` via a
//! macro or const at compile time), so this test is the mechanism that keeps
//! the two from silently disagreeing again.
//!
//! To avoid the test itself becoming yet another independent copy of the
//! version (which could drift from the real `contractmeta!` invocation), the
//! test parses the actual `contractmeta!(key = "Version", val = ...)`
//! invocation out of `lib.rs` at test time via `include_str!`. This means
//! changing the real metadata version alone will fail verification, and
//! changing only a test-local copy is impossible because there is none.

use crate::CONTRACT_VERSION;

/// The source of `lib.rs`, used to extract the actual `contractmeta!`
/// `"Version"` literal rather than relying on a hand-maintained copy.
const LIB_RS_SOURCE: &str = include_str!("lib.rs");

/// Decode a packed `CONTRACT_VERSION` integer into its `major.minor.patch`
/// semver string form.
fn decode_contract_version(version: u32) -> String {
    let major = version / 1_000_000;
    let minor = (version / 1_000) % 1_000;
    let patch = version % 1_000;
    std::format!("{major}.{minor}.{patch}")
}

/// Extract the `val = "..."` string literal from the
/// `contractmeta!(key = "Version", val = "...")` invocation in the given
/// source text.
///
/// Returns `None` if no such invocation (or no string literal value) can be
/// found.
fn extract_contractmeta_version(source: &str) -> Option<String> {
    // Find the `contractmeta!` invocation that declares the "Version" key.
    let meta_start = source.find("contractmeta!")?;
    let after_meta = &source[meta_start..];

    // Locate the `key = "Version"` portion within this invocation.
    let key_pos = after_meta.find("\"Version\"")?;
    let after_key = &after_meta[key_pos..];

    // Locate the `val =` portion after the key.
    let val_pos = after_key.find("val")?;
    let after_val = &after_key[val_pos..];

    // Find the opening quote of the string literal following `val =`.
    let eq_pos = after_val.find('=')?;
    let after_eq = &after_val[eq_pos + 1..];

    let open_quote = after_eq.find('"')?;
    let after_open = &after_eq[open_quote + 1..];

    let close_quote = after_open.find('"')?;
    Some(after_open[..close_quote].to_string())
}

#[test]
fn contractmeta_version_matches_contract_version_constant() {
    let decoded = decode_contract_version(CONTRACT_VERSION);

    let metadata_version = extract_contractmeta_version(LIB_RS_SOURCE)
        .expect("could not find contractmeta!(key = \"Version\", val = \"...\") in lib.rs");

    assert_eq!(
        metadata_version, decoded,
        "contractmeta!'s \"Version\" literal in lib.rs must match \
         CONTRACT_VERSION's semver-decoded form; update both together"
    );
}

#[test]
fn extract_contractmeta_version_finds_actual_literal() {
    // Sanity check that the parser actually locates a version literal in the
    // real source, so the main test cannot silently pass by finding nothing.
    let metadata_version = extract_contractmeta_version(LIB_RS_SOURCE)
        .expect("could not find contractmeta!(key = \"Version\", val = \"...\") in lib.rs");
    assert!(
        !metadata_version.is_empty(),
        "contractmeta! \"Version\" literal must not be empty"
    );
}

#[test]
fn fixture_mismatched_metadata_version_is_detected() {
    // Simulate changing only the actual metadata version in lib.rs: the
    // parser must surface the new value, which then fails to match the
    // decoded CONTRACT_VERSION.
    let fixture = r#"
        soroban_sdk::contractmeta!(key = "Version", val = "9.9.9");
    "#;
    let extracted = extract_contractmeta_version(fixture)
        .expect("fixture parser should find the metadata version");
    assert_eq!(extracted, "9.9.9");

    let decoded = decode_contract_version(CONTRACT_VERSION);
    assert_ne!(
        extracted, decoded,
        "a mismatched metadata version must be detected as different from \
         CONTRACT_VERSION"
    );
}

#[test]
fn fixture_matching_metadata_version_passes() {
    // Simulate a lib.rs whose metadata version matches CONTRACT_VERSION.
    let decoded = decode_contract_version(CONTRACT_VERSION);
    let fixture = std::format!(
        r#"soroban_sdk::contractmeta!(key = "Version", val = "{decoded}");"#
    );
    let extracted = extract_contractmeta_version(&fixture)
        .expect("fixture parser should find the metadata version");
    assert_eq!(
        extracted, decoded,
        "matching metadata and CONTRACT_VERSION versions must pass"
    );
}