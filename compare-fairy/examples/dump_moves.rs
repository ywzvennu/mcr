//! Throwaway: dump mce legal moves + child perft(1) for a (variant, FEN).
//! Usage: cargo run --release --example dump_moves -- <variant> "<fen>"
use mce::geometry::{AnyWideVariant, WideVariantId};

fn main() {
    let mut a = std::env::args().skip(1);
    let v = a.next().unwrap();
    let fen = a.next().unwrap();
    let id: WideVariantId = v.parse().unwrap();
    let pos = AnyWideVariant::from_fen(id, &fen).unwrap();
    let mut moves: Vec<(String, u64)> = pos
        .legal_moves()
        .iter()
        .map(|m| (pos.to_uci(m), pos.play(m).perft(1)))
        .collect();
    moves.sort();
    println!("mce legal_moves: {}", moves.len());
    for (m, n) in &moves {
        println!("{m} {n}");
    }
}
