//! Move-generation and make-move micro-benchmarks for standard chess.
//!
//! Covers [`Position::legal_moves`] on a spread of positions (the starting
//! position, a quiet middlegame, and the tactically dense Kiwipete position)
//! plus [`Position::play`] applied to every legal move from the start.
//!
//! Run with `cargo bench --bench movegen`.

use criterion::{criterion_group, criterion_main, Criterion};
use mcr::Position;
use std::hint::black_box;

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const MIDGAME: &str = "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

fn parse(fen: &str) -> Position {
    Position::from_fen(fen).expect("valid FEN")
}

fn bench_legal_moves(c: &mut Criterion) {
    let mut group = c.benchmark_group("legal_moves");
    for &(name, fen) in &[
        ("startpos", STARTPOS),
        ("midgame", MIDGAME),
        ("kiwipete", KIWIPETE),
    ] {
        let pos = parse(fen);
        group.bench_function(name, |b| {
            b.iter(|| black_box(black_box(&pos).legal_moves()));
        });
    }
    group.finish();
}

fn bench_play(c: &mut Criterion) {
    let pos = parse(STARTPOS);
    let moves = pos.legal_moves();
    c.bench_function("play/startpos_all_moves", |b| {
        b.iter(|| {
            for mv in &moves {
                black_box(black_box(&pos).play(black_box(mv)));
            }
        });
    });
}

criterion_group!(benches, bench_legal_moves, bench_play);
criterion_main!(benches);
