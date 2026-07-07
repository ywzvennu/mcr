//! Apples-to-apples standard-chess benchmark: the **concrete** 8x8 engine
//! versus the **generic** engine monomorphized to the same board.
//!
//! mcr implements standard 8x8 chess twice:
//!
//! * the concrete, hand-written engine — [`mcr::Position`] over frozen `u64`
//!   bitboards (`src/position.rs`, `src/board.rs`, `src/variant/`), and
//! * the generic/wide engine — [`GenericPosition`] parametrized over a
//!   [`Geometry`] and a [`WideVariant`], where `GenericPosition<Chess8x8,
//!   StandardChess>` monomorphizes the width to `u64` and is documented to
//!   const-fold the geometry's `& 7` / `>> 3` coordinate math to the same ops
//!   the concrete path hand-writes.
//!
//! This bench runs the **same positions at the same depths** through both paths
//! so the throughput gap (`generic / concrete`) is directly readable. It is the
//! data behind the "unify or keep the split" decision in
//! `docs/perf-concrete-vs-generic.md`: the concrete `perft` / `movegen` /
//! `variants` benches only cover the concrete path, and `geometry` only covers
//! the *wider* fairy geometries, so neither pits the two 8x8 paths against each
//! other on identical work. This one does.
//!
//! Every perft group sets [`Throughput::Elements`] to the (fixed, known) node
//! count, so criterion reports **nodes/sec** directly and the two engines line
//! up on one scale. The positions and depths mirror the concrete `perft` /
//! `movegen` benches. Parity of the node counts themselves is a *test*
//! invariant (`tests/perft_generic.rs`), not re-asserted here.
//!
//! Run with `cargo bench --bench concrete_vs_generic`.
//!
//! [`GenericPosition`]: mcr::geometry::GenericPosition
//! [`Geometry`]: mcr::geometry::Geometry
//! [`WideVariant`]: mcr::geometry::WideVariant

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use mcr::geometry::{perft as gperft, Chess8x8, GenericPosition, StandardChess};
use mcr::{perft as cperft, Position};
use std::hint::black_box;

/// The generic engine monomorphized to the concrete board: 8x8, `u64`-backed,
/// standard-chess rules.
type GenPos = GenericPosition<Chess8x8, StandardChess>;

// ----- Shared position set ----------------------------------------------------
//
// Identical FENs for both engines; the same three the concrete `movegen` bench
// uses, plus the two perft anchors from the concrete `perft` bench.

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const MIDGAME: &str = "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

/// `(name, fen, depth, node_count)` for the perft comparison. The node counts
/// are the standard reference values (see `tests/perft_generic.rs`) and drive
/// the criterion throughput so the report is in nodes/sec.
const PERFT_CASES: &[(&str, &str, u32, u64)] = &[
    ("startpos_d4", STARTPOS, 4, 197_281),
    ("startpos_d5", STARTPOS, 5, 4_865_609),
    ("kiwipete_d3", KIWIPETE, 3, 97_862),
    ("kiwipete_d4", KIWIPETE, 4, 4_085_603),
];

/// `(name, fen)` for the legal-move-generation comparison.
const MOVEGEN_CASES: &[(&str, &str)] = &[
    ("startpos", STARTPOS),
    ("midgame", MIDGAME),
    ("kiwipete", KIWIPETE),
];

// ----- Legal move generation --------------------------------------------------

fn bench_movegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("cvg_movegen");
    for &(name, fen) in MOVEGEN_CASES {
        let cpos = Position::from_fen(fen).expect("valid FEN (concrete)");
        let gpos = GenPos::from_fen(fen).expect("valid FEN (generic)");
        // Throughput = one legal-move enumeration per iteration; criterion then
        // reports elements/sec, i.e. movegen calls/sec, on a shared scale.
        group.throughput(Throughput::Elements(1));
        group.bench_function(format!("concrete/{name}"), |b| {
            b.iter(|| black_box(black_box(&cpos).legal_moves()));
        });
        group.bench_function(format!("generic/{name}"), |b| {
            b.iter(|| black_box(black_box(&gpos).legal_moves()));
        });
    }
    group.finish();
}

// ----- Perft ------------------------------------------------------------------

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("cvg_perft");
    for &(name, fen, depth, nodes) in PERFT_CASES {
        let cpos = Position::from_fen(fen).expect("valid FEN (concrete)");
        let gpos = GenPos::from_fen(fen).expect("valid FEN (generic)");
        // Nodes/sec: the visited-node count is fixed and known, so feeding it as
        // the throughput makes criterion print the number that matters here.
        group.throughput(Throughput::Elements(nodes));
        group.bench_function(format!("concrete/{name}"), |b| {
            b.iter(|| cperft(black_box(&cpos), black_box(depth)));
        });
        group.bench_function(format!("generic/{name}"), |b| {
            b.iter(|| gperft(black_box(&gpos), black_box(depth)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_movegen, bench_perft);
criterion_main!(benches);
