//! Generator + drift-check for `docs/rules/*.json`, the machine/LLM-consumable
//! rules output (issue #546).
//!
//! For every variant in [`VariantRef::ALL`] this serializes the engine-derived
//! [`VariantRules`](mcr::geometry::VariantRules) model to a stable,
//! pretty-printed JSON file, plus an `index.json` discovery manifest listing all
//! variants with their key fields. Because the JSON is produced straight from
//! the `serde::Serialize` derive on the model — the very same model derived from
//! the move-generation hooks — it can never drift from the engine.
//!
//! The whole file is gated behind the `serde` feature (the serialize impls live
//! there), so a default `cargo test` skips it; run it with
//! `cargo test --features serde --test rules_json`. Regenerate the committed
//! files with `REGEN=1 cargo test --features serde --test rules_json` (or set
//! `BLESS=1`).

#![cfg(feature = "serde")]

use std::path::{Path, PathBuf};

use mcr::VariantRef;

/// The directory the per-variant JSON files and the index live in.
fn rules_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/rules")
}

/// A single discovery-manifest entry: the key fields a consumer needs to locate
/// and triage a variant without opening its full ruleset file.
#[derive(serde::Serialize)]
struct IndexEntry {
    /// The variant's canonical name (and the stem of its `<name>.json` file).
    name: String,
    /// Which family the variant belongs to: `"concrete"` (the 8x8 engine) or
    /// `"wide"` (the generic-geometry fairy layer).
    family: &'static str,
    /// Board width in files.
    width: u8,
    /// Board height in ranks.
    height: u8,
    /// The starting position in mcr's FEN dialect.
    start_fen: String,
    /// The roles on the starting board, by name (the army roster).
    roles: Vec<String>,
    /// The external oracle the variant is validated against, serialized in the
    /// same shape as the per-variant file's `oracle` field.
    oracle: mcr::geometry::ValidationOracle,
}

/// Serializes a value to pretty JSON with a trailing newline (stable key order
/// follows struct field declaration order).
fn to_json<T: serde::Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("serialize to JSON");
    s.push('\n');
    s
}

/// Builds `(relative_file_name, contents)` for every variant plus the index, in
/// a deterministic order.
fn generate() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut index = Vec::new();
    for &vref in VariantRef::ALL.iter() {
        let name = vref.name();
        let rules = vref.rules();
        out.push((format!("{name}.json"), to_json(&rules)));
        index.push(IndexEntry {
            name: name.to_string(),
            family: match vref {
                VariantRef::Concrete(_) => "concrete",
                VariantRef::Wide(_) => "wide",
            },
            width: rules.board.width,
            height: rules.board.height,
            start_fen: rules.board.start_fen.clone(),
            roles: rules.army.iter().map(|p| p.name.clone()).collect(),
            oracle: rules.oracle,
        });
    }
    out.push(("index.json".to_string(), to_json(&index)));
    out
}

#[test]
fn rules_json_is_up_to_date() {
    let dir = rules_dir();
    let files = generate();
    let regen = std::env::var_os("REGEN").is_some() || std::env::var_os("BLESS").is_some();
    if regen {
        std::fs::create_dir_all(&dir).expect("create docs/rules");
    }
    for (name, generated) in &files {
        let path = dir.join(name);
        if regen {
            std::fs::write(&path, generated).unwrap_or_else(|e| panic!("write {name}: {e}"));
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            &committed, generated,
            "docs/rules/{name} is out of date; regenerate with \
             `REGEN=1 cargo test --features serde --test rules_json`",
        );
    }
}

/// Every generated JSON file must be valid, parseable JSON, and the manifest
/// must cover every registered variant exactly once — a structural check
/// independent of the golden-file comparison.
#[test]
fn generated_json_is_valid_and_complete() {
    let files = generate();
    // One file per variant, plus the index.
    assert_eq!(files.len(), VariantRef::ALL.len() + 1);
    for (name, contents) in &files {
        serde_json::from_str::<serde_json::Value>(contents)
            .unwrap_or_else(|e| panic!("{name} is not valid JSON: {e}"));
    }
    let index_json = &files
        .iter()
        .find(|(n, _)| n == "index.json")
        .expect("index.json generated")
        .1;
    let index: serde_json::Value = serde_json::from_str(index_json).unwrap();
    assert_eq!(
        index.as_array().map(|a| a.len()),
        Some(VariantRef::ALL.len()),
        "index.json must list every variant",
    );
}
