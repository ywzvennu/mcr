//! Move-generation and perft benchmarks for the generic geometry layer.
//!
//! The concrete-engine benches (`movegen`, `perft`, `variants`) cover the
//! frozen 8x8 `u64` path. This bench tracks the *parallel* generic layer that
//! hosts the fairy variants — [`GenericPosition`] over the wider board
//! geometries — so a regression in the geometry codegen has an in-repo,
//! FSF-free signal (complementing the FSF head-to-head in `compare-fairy`).
//!
//! A representative variant is benched per geometry family, spanning the board
//! shapes and backing integers the generic layer must handle:
//!
//! | geometry  | backing | variant      |
//! |-----------|---------|--------------|
//! | 8x8       | `u64`   | Seirawan     |
//! | 9x9       | `u128`  | Shogi        |
//! | 9x10      | `u128`  | Xiangqi      |
//! | 10x10     | `u128`  | Grand        |
//! | 5x5       | `u64`   | Minishogi    |
//! | 7x7       | `u128`  | Minixiangqi  |
//!
//! Each variant is benched for both move generation ([`legal_moves`]) and perft
//! throughput at a fixed shallow depth, from the starting position and from one
//! midgame position. Every FEN is pinned from the corresponding
//! `tests/perft_<variant>.rs`, so they are known-valid. Depths are kept shallow
//! so a full `cargo bench` run finishes in a reasonable time.
//!
//! Run with `cargo bench --bench geometry`.
//!
//! [`legal_moves`]: mce::geometry::GenericPosition::legal_moves

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use mce::geometry::{
    perft, GenericPosition, Geometry, Grand, Minishogi, Minixiangqi, Seirawan, Shogi, WideVariant,
    Xiangqi,
};
use std::hint::black_box;

// ----- Representative position set (FENs pinned from tests/perft_*.rs) --------

// 8x8, u64 — Seirawan (S-chess): gating drops layered on board moves.
const SEIRAWAN_START: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR[HEhe] w KQBCDFGkqbcdfg - 0 1";
const SEIRAWAN_MID: &str =
    "r1bqkb1r/pppppppp/2n2n2/8/8/2N2N2/PPPPPPPP/R1BQKB1R[HEhe] w KQBCDEFGkqbcdefg - 4 3";

// 9x9, u128 — Shogi: persistent capture-fed hand and drops.
const SHOGI_START: &str = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL[] w - - 0 1";
const SHOGI_MID: &str = "lnsgkgsnl/1r5b1/pppppp1pp/9/9/9/PPPPPP1PP/1B5R1/LNSGKGSNL[Pp] b - - 0 1";

// 9x10, u128 — Xiangqi: cannons, horses, palace, flying general.
const XIANGQI_START: &str = "rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/9/RJOUKUOJR w - - 0 1";
const XIANGQI_MID: &str =
    "r1oukuo1r/9/1cj3jc1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1CJ3JC1/9/R1OUKUO1R w - - 0 1";

// 10x10, u128 — Grand chess.
const GRAND_START: &str =
    "r8r/1nbqkeabn1/pppppppppp/10/10/10/10/PPPPPPPPPP/1NBQKEABN1/R8R w - - 0 1";
const GRAND_MID: &str =
    "r8r/2bqkeab2/pppp1ppppp/2n4n2/3Np5/3P6/7N2/PPP1PPPPPP/2BQKEAB2/R8R b - - 1 4";

// 5x5, u64 — Minishogi.
const MINISHOGI_START: &str = "rbsgk/4p/5/P4/KGSBR[] w - - 0 1";
const MINISHOGI_MID: &str = "2k2/5/R3r/5/2K2[Pp] w - - 0 1";

// 7x7, u128 — Minixiangqi.
const MINIXIANGQI_START: &str = "rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1";
const MINIXIANGQI_MID: &str = "r1jkjcr/z1zzz1z/2c4/2J4/7/Z1ZZZ1Z/R1CKJCR w - - 0 1";

// ----- Generic bench helpers --------------------------------------------------

/// Benches [`legal_moves`](GenericPosition::legal_moves) on a single position.
fn add_movegen<G: Geometry, V: WideVariant<G>>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    id: &str,
    pos: &GenericPosition<G, V>,
) {
    group.bench_function(id, |b| {
        b.iter(|| black_box(black_box(pos).legal_moves()));
    });
}

/// Benches [`perft`] to a fixed depth on a single position.
fn add_perft<G: Geometry, V: WideVariant<G>>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    id: &str,
    pos: &GenericPosition<G, V>,
    depth: u32,
) {
    group.bench_function(id, |b| {
        b.iter(|| perft(black_box(pos), black_box(depth)));
    });
}

// ----- Move generation --------------------------------------------------------

fn bench_movegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry_movegen");

    let seirawan_start = Seirawan::from_fen(SEIRAWAN_START).expect("valid Seirawan FEN");
    let seirawan_mid = Seirawan::from_fen(SEIRAWAN_MID).expect("valid Seirawan FEN");
    add_movegen(&mut group, "seirawan_8x8/startpos", &seirawan_start);
    add_movegen(&mut group, "seirawan_8x8/midgame", &seirawan_mid);

    let shogi_start = Shogi::from_fen(SHOGI_START).expect("valid Shogi FEN");
    let shogi_mid = Shogi::from_fen(SHOGI_MID).expect("valid Shogi FEN");
    add_movegen(&mut group, "shogi_9x9/startpos", &shogi_start);
    add_movegen(&mut group, "shogi_9x9/midgame", &shogi_mid);

    let xiangqi_start = Xiangqi::from_fen(XIANGQI_START).expect("valid Xiangqi FEN");
    let xiangqi_mid = Xiangqi::from_fen(XIANGQI_MID).expect("valid Xiangqi FEN");
    add_movegen(&mut group, "xiangqi_9x10/startpos", &xiangqi_start);
    add_movegen(&mut group, "xiangqi_9x10/midgame", &xiangqi_mid);

    let grand_start = Grand::from_fen(GRAND_START).expect("valid Grand FEN");
    let grand_mid = Grand::from_fen(GRAND_MID).expect("valid Grand FEN");
    add_movegen(&mut group, "grand_10x10/startpos", &grand_start);
    add_movegen(&mut group, "grand_10x10/midgame", &grand_mid);

    let minishogi_start = Minishogi::from_fen(MINISHOGI_START).expect("valid Minishogi FEN");
    let minishogi_mid = Minishogi::from_fen(MINISHOGI_MID).expect("valid Minishogi FEN");
    add_movegen(&mut group, "minishogi_5x5/startpos", &minishogi_start);
    add_movegen(&mut group, "minishogi_5x5/midgame", &minishogi_mid);

    let minixiangqi_start =
        Minixiangqi::from_fen(MINIXIANGQI_START).expect("valid Minixiangqi FEN");
    let minixiangqi_mid = Minixiangqi::from_fen(MINIXIANGQI_MID).expect("valid Minixiangqi FEN");
    add_movegen(&mut group, "minixiangqi_7x7/startpos", &minixiangqi_start);
    add_movegen(&mut group, "minixiangqi_7x7/midgame", &minixiangqi_mid);

    group.finish();
}

// ----- Perft ------------------------------------------------------------------

fn bench_perft(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry_perft");

    let seirawan_start = Seirawan::from_fen(SEIRAWAN_START).expect("valid Seirawan FEN");
    let seirawan_mid = Seirawan::from_fen(SEIRAWAN_MID).expect("valid Seirawan FEN");
    add_perft(&mut group, "seirawan_8x8/startpos_d2", &seirawan_start, 2);
    add_perft(&mut group, "seirawan_8x8/midgame_d2", &seirawan_mid, 2);

    let shogi_start = Shogi::from_fen(SHOGI_START).expect("valid Shogi FEN");
    let shogi_mid = Shogi::from_fen(SHOGI_MID).expect("valid Shogi FEN");
    add_perft(&mut group, "shogi_9x9/startpos_d3", &shogi_start, 3);
    add_perft(&mut group, "shogi_9x9/midgame_d3", &shogi_mid, 3);

    let xiangqi_start = Xiangqi::from_fen(XIANGQI_START).expect("valid Xiangqi FEN");
    let xiangqi_mid = Xiangqi::from_fen(XIANGQI_MID).expect("valid Xiangqi FEN");
    add_perft(&mut group, "xiangqi_9x10/startpos_d3", &xiangqi_start, 3);
    add_perft(&mut group, "xiangqi_9x10/midgame_d3", &xiangqi_mid, 3);

    let grand_start = Grand::from_fen(GRAND_START).expect("valid Grand FEN");
    let grand_mid = Grand::from_fen(GRAND_MID).expect("valid Grand FEN");
    add_perft(&mut group, "grand_10x10/startpos_d3", &grand_start, 3);
    add_perft(&mut group, "grand_10x10/midgame_d3", &grand_mid, 3);

    let minishogi_start = Minishogi::from_fen(MINISHOGI_START).expect("valid Minishogi FEN");
    let minishogi_mid = Minishogi::from_fen(MINISHOGI_MID).expect("valid Minishogi FEN");
    add_perft(&mut group, "minishogi_5x5/startpos_d3", &minishogi_start, 3);
    add_perft(&mut group, "minishogi_5x5/midgame_d3", &minishogi_mid, 3);

    let minixiangqi_start =
        Minixiangqi::from_fen(MINIXIANGQI_START).expect("valid Minixiangqi FEN");
    let minixiangqi_mid = Minixiangqi::from_fen(MINIXIANGQI_MID).expect("valid Minixiangqi FEN");
    add_perft(
        &mut group,
        "minixiangqi_7x7/startpos_d3",
        &minixiangqi_start,
        3,
    );
    add_perft(
        &mut group,
        "minixiangqi_7x7/midgame_d3",
        &minixiangqi_mid,
        3,
    );

    group.finish();
}

criterion_group!(benches, bench_movegen, bench_perft);
criterion_main!(benches);
