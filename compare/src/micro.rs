//! Non-perft micro-benchmarks: the real hot paths beyond raw perft.
//!
//! Perft measures one thing (recursive make/unmake + count). Real engines and
//! tools also spend time in single-shot operations: generating the legal move
//! list once, making one move, parsing and serializing FENs, and (for tooling)
//! producing SAN and Zobrist keys. This module measures the throughput of each
//! over a fixed sample of positions, comparing mce against shakmaty where the
//! operation is comparable and reporting mce-only figures (SAN, Zobrist) where
//! shakmaty's surface differs enough that a head-to-head would be apples to
//! oranges.
//!
//! Methodology mirrors the perft timer: warm up, then take repeated timed
//! batches and report the median throughput (ops/sec). Each batch runs the
//! operation `INNER` times across the sample so the per-call cost is large
//! relative to the clock resolution. `std::hint::black_box` keeps the optimizer
//! from eliding the work.

use std::hint::black_box;
use std::time::Instant;

use shakmaty::fen::Fen;
use shakmaty::{CastlingMode, Chess as ShChess, EnPassantMode, Position as ShPosition};

use mce::{AnyVariant, Position, VariantId};

use crate::stats::{summarize, TimeStats};

/// Timed batches per micro-benchmark.
const BATCHES: usize = 9;
/// Inner repetitions of the whole sample per batch.
const INNER: usize = 40;

/// One micro-benchmark result: a name, the per-second throughput for each engine
/// (shakmaty optional when the op is mce-only), and the mce sample spread.
pub struct MicroResult {
    /// Operation name, e.g. `"legal_moves"`.
    pub name: &'static str,
    /// mce throughput in operations per second (median-based).
    pub mce_ops: f64,
    /// shakmaty throughput in ops/sec, or `None` for mce-only operations.
    pub shak_ops: Option<f64>,
    /// mce timing spread (coefficient of variation).
    pub mce_cv: f64,
}

impl MicroResult {
    /// mce/shakmaty ops-per-second ratio (>1 means mce does more ops/sec), or
    /// `None` for mce-only operations.
    pub fn ratio(&self) -> Option<f64> {
        self.shak_ops
            .map(|s| if s > 0.0 { self.mce_ops / s } else { f64::NAN })
    }
}

/// Time `f` over `BATCHES` batches; return ops/sec for `ops_per_batch` ops/batch.
fn time_throughput(ops_per_batch: u64, mut f: impl FnMut()) -> TimeStats {
    // Warm up.
    for _ in 0..2 {
        f();
    }
    let mut samples = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_nanos() as u64);
    }
    let _ = ops_per_batch;
    summarize(&samples)
}

/// Convert a per-batch timing + ops-per-batch into median ops/sec.
fn ops_per_sec(stats: &TimeStats, ops_per_batch: u64) -> f64 {
    ops_per_batch as f64 / stats.median_s
}

/// Run all micro-benchmarks over the given standard-chess sample FENs (a subset
/// of the EPD + curated positions; SAN/Zobrist are most meaningful on standard
/// chess). Returns one [`MicroResult`] per operation.
pub fn run(sample_fens: &[String]) -> Vec<MicroResult> {
    // Pre-parse the sample once into each engine's position type. Only keep FENs
    // both engines accept so every op runs on the same set.
    let mut mce_pos: Vec<Position> = Vec::new();
    let mut shak_pos: Vec<ShChess> = Vec::new();
    let mut fens: Vec<String> = Vec::new();
    for f in sample_fens {
        let Ok(m) = Position::from_fen(f) else {
            continue;
        };
        let Some(s) = Fen::from_ascii(f.as_bytes())
            .ok()
            .and_then(|fen| fen.into_position::<ShChess>(CastlingMode::Standard).ok())
        else {
            continue;
        };
        mce_pos.push(m);
        shak_pos.push(s);
        fens.push(f.clone());
    }
    assert!(!mce_pos.is_empty(), "no shared standard sample positions");
    let n = mce_pos.len() as u64;
    let ops_per_batch = n * INNER as u64;

    let mut out = Vec::new();

    // ---- legal_moves(): move generation throughput ------------------------
    {
        let mce_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for p in &mce_pos {
                    black_box(p.legal_moves().len());
                }
            }
        });
        let shak_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for p in &shak_pos {
                    black_box(p.legal_moves().len());
                }
            }
        });
        out.push(MicroResult {
            name: "legal_moves",
            mce_ops: ops_per_sec(&mce_t, ops_per_batch),
            shak_ops: Some(ops_per_sec(&shak_t, ops_per_batch)),
            mce_cv: mce_t.cv(),
        });
    }

    // ---- play(): make-move throughput (first legal move of each position) --
    {
        // Pre-pick the first legal move per position for each engine.
        let mce_moves: Vec<_> = mce_pos.iter().map(|p| p.legal_moves()[0]).collect();
        let shak_moves: Vec<_> = shak_pos
            .iter()
            .map(|p| p.legal_moves()[0].clone())
            .collect();
        let mce_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for (p, mv) in mce_pos.iter().zip(&mce_moves) {
                    black_box(p.play(mv));
                }
            }
        });
        let shak_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for (p, mv) in shak_pos.iter().zip(&shak_moves) {
                    let mut q = p.clone();
                    q.play_unchecked(mv);
                    black_box(q.turn());
                }
            }
        });
        out.push(MicroResult {
            name: "play",
            mce_ops: ops_per_sec(&mce_t, ops_per_batch),
            shak_ops: Some(ops_per_sec(&shak_t, ops_per_batch)),
            mce_cv: mce_t.cv(),
        });
    }

    // ---- FEN parse + serialize round-trip throughput ----------------------
    {
        let mce_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for f in &fens {
                    let p = Position::from_fen(black_box(f)).unwrap();
                    black_box(p.to_fen());
                }
            }
        });
        let shak_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for f in &fens {
                    let p: ShChess = Fen::from_ascii(black_box(f).as_bytes())
                        .unwrap()
                        .into_position(CastlingMode::Standard)
                        .unwrap();
                    black_box(Fen::from_position(p, EnPassantMode::Legal).to_string());
                }
            }
        });
        out.push(MicroResult {
            name: "fen_roundtrip",
            mce_ops: ops_per_sec(&mce_t, ops_per_batch),
            shak_ops: Some(ops_per_sec(&shak_t, ops_per_batch)),
            mce_cv: mce_t.cv(),
        });
    }

    // ---- SAN serialization (mce-only): SAN of every legal move ------------
    {
        // ops/batch here is the total number of moves SAN'd, not positions.
        let total_moves: u64 = mce_pos
            .iter()
            .map(|p| p.legal_moves().len() as u64)
            .sum::<u64>()
            * INNER as u64;
        let san_t = time_throughput(total_moves, || {
            for _ in 0..INNER {
                for p in &mce_pos {
                    for mv in p.legal_moves() {
                        black_box(p.san(&mv));
                    }
                }
            }
        });
        out.push(MicroResult {
            name: "san (mce-only)",
            mce_ops: ops_per_sec(&san_t, total_moves),
            shak_ops: None,
            mce_cv: san_t.cv(),
        });
    }

    // ---- Zobrist hashing (mce-only): key of each position -----------------
    {
        // Use AnyVariant zobrist so this exercises the public runtime path.
        let any: Vec<AnyVariant> = fens
            .iter()
            .map(|f| AnyVariant::from_fen(VariantId::Standard, f).unwrap())
            .collect();
        let zob_t = time_throughput(ops_per_batch, || {
            for _ in 0..INNER {
                for p in &any {
                    black_box(p.zobrist());
                }
            }
        });
        out.push(MicroResult {
            name: "zobrist (mce-only)",
            mce_ops: ops_per_sec(&zob_t, ops_per_batch),
            shak_ops: None,
            mce_cv: zob_t.cv(),
        });
    }

    out
}
