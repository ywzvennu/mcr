//! `mce perft` — walk the legal-move tree to a fixed depth and count leaf nodes.
//!
//! Perft is the standard correctness/speed check for a move generator: the node
//! count for a position and depth is a fixed number every correct engine must
//! reproduce. `--divide` gives the per-root-move breakdown (the usual way to
//! bisect a discrepancy); `--parallel` uses the rayon-backed split when the crate
//! was built with the `parallel` feature; `--variant` selects a rule set.

use clap::Args;

use crate::util::{self, CliResult};

/// Arguments for `mce perft <FEN|startpos> <depth>`.
#[derive(Debug, Args)]
pub struct PerftArgs {
    /// Start position: a six-field FEN, or `startpos` for the standard opening.
    fen: String,
    /// Search depth (number of plies); depth 0 counts the position itself (1).
    depth: u32,
    /// Variant rule set (default: standard chess). E.g. `atomic`, `koth`, `3check`.
    #[arg(long, value_name = "V")]
    variant: Option<String>,
    /// Print the per-root-move node breakdown as well as the total.
    #[arg(long)]
    divide: bool,
    /// Use the rayon-backed parallel counter (requires the `parallel` build
    /// feature; otherwise falls back to the serial counter with a note).
    #[arg(long)]
    parallel: bool,
}

pub fn run(args: PerftArgs) -> CliResult {
    let fen = util::resolve_fen(&args.fen);
    let id = util::parse_variant(args.variant.as_deref())?;

    if util::is_standard(id) {
        let pos = util::load_position(fen)?;
        run_standard(&pos, args.depth, args.divide, args.parallel);
    } else {
        let pos = util::load_variant(id, fen)?;
        println!("variant: {}", pos.variant_id());
        run_variant(&pos, args.depth, args.divide, args.parallel);
    }
    Ok(())
}

/// Standard-chess perft, with full access to `perft` / `perft_divide` / the
/// optional `perft_parallel`.
fn run_standard(pos: &mce::Position, depth: u32, divide: bool, parallel: bool) {
    if divide {
        let mut total = 0;
        for (mv, count) in mce::perft_divide(pos, depth) {
            println!("{}: {count}", mv.to_uci());
            total += count;
        }
        println!();
        println!("depth {depth}: {total} nodes");
        return;
    }

    let total = total_perft(pos, depth, parallel);
    println!("depth {depth}: {total} nodes");
}

/// Runs the whole-tree count, choosing the parallel counter when asked for and
/// available.
fn total_perft(pos: &mce::Position, depth: u32, parallel: bool) -> u64 {
    if parallel {
        #[cfg(feature = "parallel")]
        {
            return mce::perft_parallel(pos, depth);
        }
        #[cfg(not(feature = "parallel"))]
        {
            eprintln!(
                "note: --parallel requested but this binary was built without the \
                 `parallel` feature; using the serial counter"
            );
        }
    }
    mce::perft(pos, depth)
}

/// Variant perft. `AnyVariant` exposes a whole-tree `perft` but no divide, so the
/// per-root-move breakdown is built here by playing each root move and counting
/// one ply shallower. There is no parallel entry point on `AnyVariant`, so
/// `--parallel` is a no-op (noted) here.
fn run_variant(pos: &mce::AnyVariant, depth: u32, divide: bool, parallel: bool) {
    if parallel {
        eprintln!("note: --parallel has no effect for variants; using the serial counter");
    }

    if !divide {
        println!("depth {depth}: {} nodes", pos.perft(depth));
        return;
    }

    if depth == 0 {
        println!("depth 0: 1 node");
        return;
    }

    let mut total = 0;
    for mv in pos.legal_moves() {
        let count = pos.play(&mv).perft(depth - 1);
        println!("{}: {count}", pos.to_uci(&mv));
        total += count;
    }
    println!();
    println!("depth {depth}: {total} nodes");
}
