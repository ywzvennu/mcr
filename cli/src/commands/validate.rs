//! `mce validate` — parse and validate a FEN.
//!
//! Exits 0 with an `ok:` line when the FEN parses (and, for the chosen variant,
//! is a legal position the library accepts); exits nonzero with an error message
//! otherwise. Handy in scripts as a cheap legality gate.

use clap::Args;

use crate::util::{self, CliResult};

/// Arguments for `mce validate <FEN>`.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// FEN to validate, or `startpos`.
    fen: String,
    /// Variant rule set (default: standard chess).
    #[arg(long, value_name = "V")]
    variant: Option<String>,
}

pub fn run(args: ValidateArgs) -> CliResult {
    let fen = util::resolve_fen(&args.fen);
    let id = util::parse_variant(args.variant.as_deref())?;

    // Parsing through the variant-aware loader both decodes the FEN and rejects
    // positions the rule set considers illegal; success means a usable position.
    let pos = util::load_variant(id, fen)?;
    println!("ok: {} | {}", pos.variant_id(), pos.to_fen());
    Ok(())
}
