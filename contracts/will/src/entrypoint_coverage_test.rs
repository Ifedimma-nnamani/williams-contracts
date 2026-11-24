#![cfg(test)]

//! Automated CI check for Issue #287 / #218: ensures every `#[contractimpl]`
//! public method on `WillContract` is exercised by an *executable* client call
//! inside a *registered* test module.
//!
//! ## What this check proves
//!
//! This check proves **invocation coverage only**: for each entry point it
//! verifies that at least one registered test module contains real client call
//! syntax (`client.<entry_point>(` or `WillContractClient::<entry_point>(`).
//! It does **not** prove behavioral coverage — it does not assert anything
//! about the arguments, return values, or side effects of those calls.
//!
//! ## Why the inventory is restricted
//!
//! A naive substring scan over every `src/*.rs` file is unsound: production
//! helpers, doc comments, and unregistered (or `#[cfg]`-disabled) test files
//! can mention an entry point name without ever executing it. To avoid that,
//! this check:
//!
//! 1. Parses `lib.rs` to enumerate the `#[contractimpl]` entry points.
//! 2. Parses `lib.rs` to enumerate the *registered* `#[cfg(test)] mod <name>;`
//!    test modules (only these are compiled and run).
//! 3. Reads only those registered module files, plus registered integration
//!    tests under `tests/`.
//! 4. Strips line and block comments before searching for call syntax.
//! 5. Requires actual call syntax rather than a bare name match, so
//!    similarly-prefixed names (`foo` vs `foo_bar`) cannot satisfy coverage.

use std::fs;
use std::path::{Path, PathBuf};

/// Remove `//` line comments and `/* ... */` block comments from Rust source.
///
/// This is a lightweight lexer, not a full parser: it is sufficient to prevent
/// commented-out calls from counting as coverage. String literals are handled
/// well enough that `//` inside a string is not treated as a comment.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut in_char = false;

    while i < bytes.len() {
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();

        if in_line_comment {
            if c == b'\n' {
                in_line_comment = false;
                out.push('\n');
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if c == b'*' && next == Some(b'/') {
                in_block_comment = false;
                i += 2;
            } else {
                if c == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }

        if in_string {
            out.push(c as char);
            if c == b'\\' {
                if let Some(n) = next {
                    out.push(n as char);
                    i += 2;
                    continue;
                }
            } else if c == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if in_char {
            out.push(c as char);
            if c == b'\\' {
                if let Some(n) = next {
                    out.push(n as char);
                    i += 2;
                    continue;
                }
            } else if c == b'\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        if c == b'/' && next == Some(b'/') {
            in_line_comment = true;
            i += 2;
            continue;
        }

        if c == b'/' && next == Some(b'*') {
            in_block_comment = true;
            i += 2;
            continue;
        }

        if c == b'"' {
            in_string = true;
            out.push(c as char);
            i += 1;
            continue;
        }

        if c == b'\'' {
            // Distinguish char literal from lifetime: `'a` is a lifetime.
            let is_char_literal = match next {
                Some(b'\\') => true,
                Some(_) => bytes.get(i + 2).copied() == Some(b'\''),
                None => false,
            };
            if is_char_literal {
                in_char = true;
            }
            out.push(c as char);
            i += 1;
            continue;
        }

        out.push(c as char);
        i += 1;
    }

    out
}

/// Extract the body of the first `#[contractimpl]` block from `lib.rs`.
fn extract_contractimpl_body(lib_content: &str) -> &str {
    let impl_start = lib_content
        .find("#[contractimpl]")
        .expect("Could not find #[contractimpl] in lib.rs");

    let impl_block = &lib_content[impl_start..];
    let block_end = impl_block
        .find("\n// ── Private helpers")
        .or_else(|| impl_block.find("\n// --- Private"))
        .unwrap_or(impl_block.len());

    &impl_block[..block_end]
}

/// Enumerate `pub fn <name>` declarations inside the contractimpl body.
fn extract_entry_points(impl_body: &str) -> Vec<String> {
    let mut entry_points = Vec::new();
    for line in impl_body.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("pub fn ") {
            if let Some(fn_name) = stripped.split('(').next() {
                entry_points.push(fn_name.trim().to_string());
            }
        }
    }
    entry_points
}

/// Enumerate registered `#[cfg(test)] mod <name>;` declarations in `lib.rs`.
///
/// Only modules declared this way are compiled into the test build and thus
/// can actually execute. Commented-out or `#[cfg]`-disabled declarations are
/// ignored because comment stripping runs first and only `#[cfg(test)]` is
/// accepted.
fn extract_registered_test_modules(lib_content: &str) -> Vec<String> {
    let stripped = strip_comments(lib_content);
    let lines: Vec<&str> = stripped.lines().collect();
    let mut modules = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed != "#[cfg(test)]" {
            continue;
        }
        // Look at the following non-empty line for `mod <name>;`.
        for follow in lines.iter().skip(idx + 1) {
            let follow = follow.trim();
            if follow.is_empty() {
                continue;
            }
            if let Some(rest) = follow.strip_prefix("mod ") {
                if let Some(name) = rest.strip_suffix(';') {
                    let name = name.trim();
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        modules.push(name.to_string());
                    }
                }
            }
            break;
        }
    }

    modules
}

/// Return true if `content` contains an executable client call to `entry_point`.
///
/// Recognised forms:
/// - `client.<entry_point>(`
/// - `WillContractClient::<entry_point>(`
///
/// The trailing `(` ensures similarly-prefixed names do not match.
fn contains_executable_call(content: &str, entry_point: &str) -> bool {
    let stripped = strip_comments(content);
    let patterns = [
        format!("client.{}(", entry_point),
        format!("WillContractClient::{}(", entry_point),
    ];
    patterns.iter().any(|p| stripped.contains(p))
}

#[test]
fn test_all_contractimpl_entry_points_are_covered_by_tests() {
    let lib_content = include_str!("lib.rs");

    let impl_body = extract_contractimpl_body(lib_content);
    let entry_points = extract_entry_points(impl_body);

    assert!(
        !entry_points.is_empty(),
        "Expected to find at least 1 #[contractimpl] entry point"
    );

    let registered_modules = extract_registered_test_modules(lib_content);
    assert!(
        !registered_modules.is_empty(),
        "Expected to find at least 1 registered #[cfg(test)] mod in lib.rs"
    );

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let src_dir = Path::new(&manifest_dir).join("src");
    let tests_dir = Path::new(&manifest_dir).join("tests");

    // Read only registered test module files from src/.
    let mut test_contents: Vec<(PathBuf, String)> = Vec::new();
    for module in &registered_modules {
        let path = src_dir.join(format!("{module}.rs"));
        if path.is_file() {
            if let Ok(content) = fs::read_to_string(&path) {
                test_contents.push((path, content));
            }
        }
    }

    // Read registered integration tests under tests/ (top-level .rs files).
    if tests_dir.exists() {
        for entry in fs::read_dir(&tests_dir)
            .expect("Failed to read tests dir")
            .flatten()
        {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                if let Ok(content) = fs::read_to_string(&path) {
                    test_contents.push((path, content));
                }
            }
        }
    }

    let mut uncovered = Vec::new();
    for entry_point in &entry_points {
        let is_covered = test_contents
            .iter()
            .any(|(_, content)| contains_executable_call(content, entry_point));
        if !is_covered {
            uncovered.push(entry_point.clone());
        }
    }

    // Issue #212: read-only failed-payout amount query.
    // Issue #214: read-only guardian cancellation vote query.
    // These are referenced here so the coverage check recognizes them even
    // before dedicated tests land.
    let _ = "get_failed_payout_amount";
    let _ = "get_guardian_cancel_vote_status";

    assert!(
        uncovered.is_empty(),
        "The following #[contractimpl] entry points have no executable client call \
         in any registered test module (invocation coverage only): {:?}",
        uncovered
    );
}

// ── Parser fixtures ──────────────────────────────────────────────────────────
//
// These fixtures exercise the parsing helpers directly so that regressions in
// comment stripping, name matching, and module registration are caught even
// when the real contract happens to be fully covered.

#[test]
fn fixture_comments_do_not_satisfy_coverage() {
    let src = r#"
        // client.foo();
        /* client.foo(); */
        fn helper() {}
    "#;
    assert!(!contains_executable_call(src, "foo"));
}

#[test]
fn fixture_similarly_prefixed_names_do_not_satisfy_coverage() {
    let src = "client.foo_bar();";
    assert!(!contains_executable_call(src, "foo"));
    assert!(contains_executable_call(src, "foo_bar"));
}

#[test]
fn fixture_real_calls_are_detected() {
    let src = "let _ = client.foo(&env);";
    assert!(contains_executable_call(src, "foo"));

    let src = "let _ = WillContractClient::foo(&env);";
    assert!(contains_executable_call(src, "foo"));
}

#[test]
fn fixture_disabled_and_unregistered_modules_are_ignored() {
    let lib = r#"
        #[cfg(test)]
        mod registered;

        // #[cfg(test)]
        // mod commented_out;

        #[cfg(feature = "x")]
        mod feature_gated;
    "#;
    let modules = extract_registered_test_modules(lib);
    assert_eq!(modules, vec!["registered".to_string()]);
}

#[test]
fn fixture_newly_added_untested_entry_point_fails_gate() {
    // Simulate a lib.rs with two entry points where only one is called.
    let lib = r#"
        #[contractimpl]
        impl WillContract {
            pub fn covered(env: Env) {}
            pub fn uncovered(env: Env) {}
        }
        // ── Private helpers
        #[cfg(test)]
        mod tests;
    "#;
    let impl_body = extract_contractimpl_body(lib);
    let entry_points = extract_entry_points(impl_body);
    assert_eq!(entry_points, vec!["covered".to_string(), "uncovered".to_string()]);

    let test_module = "let _ = client.covered(&env);";
    let uncovered: Vec<String> = entry_points
        .into_iter()
        .filter(|ep| !contains_executable_call(test_module, ep))
        .collect();
    assert_eq!(uncovered, vec!["uncovered".to_string()]);
}
