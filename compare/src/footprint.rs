//! Static lookup-table footprint of each engine.
//!
//! Both engines keep their attack/lookup tables as *private* `static`s, so they
//! cannot be reached from this binary via `size_of_val`. Instead we compute the
//! footprint from each table's known shape, taken directly from the engine
//! source. A `Bitboard` / packed attack entry is a `u64` (8 bytes) in both
//! engines, which makes the arithmetic transparent.
//!
//! mce uses **hyperbola-quintessence** sliders (no magic attack array), so its
//! tables are small. shakmaty uses **fixed-shift magic bitboards** with one
//! large shared attack array, which dominates its footprint.

/// One named table and its byte size.
struct Table {
    name: &'static str,
    bytes: usize,
}

/// mce's lookup tables (from `src/attacks.rs` and `src/zobrist.rs`).
///
/// Sizes are derived from the array shapes; `Bitboard` is a `u64` (8 bytes).
const MCE_TABLES: &[Table] = &[
    // src/attacks.rs — move-generation attack/ray tables.
    Table {
        name: "KNIGHT_ATTACKS [Bitboard;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "KING_ATTACKS [Bitboard;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "PAWN_ATTACKS [[Bitboard;64];2]",
        bytes: 2 * 64 * 8,
    },
    Table {
        name: "DIAG [Bitboard;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "ANTI_DIAG [Bitboard;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "BETWEEN [[Bitboard;64];64]",
        bytes: 64 * 64 * 8,
    },
    Table {
        name: "LINE [[Bitboard;64];64]",
        bytes: 64 * 64 * 8,
    },
    // src/zobrist.rs — hashing constants (KEYS static).
    Table {
        name: "ZOBRIST KEYS (pieces+state)",
        // pieces [[[u64;64];6];2] + black_to_move u64 + castling [u64;4] + ep_file [u64;8]
        bytes: 2 * 6 * 64 * 8 + 8 + 4 * 8 + 8 * 8,
    },
];

/// shakmaty's lookup tables (from `shakmaty-0.27/src/bootstrap.rs` and
/// `src/magics.rs`). Magic attack array dominates.
const SHAKMATY_TABLES: &[Table] = &[
    Table {
        name: "ATTACKS magic array [u64;88772]",
        bytes: 88772 * 8,
    },
    Table {
        name: "RAYS [[u64;64];64]",
        bytes: 64 * 64 * 8,
    },
    Table {
        name: "KNIGHT_ATTACKS [u64;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "KING_ATTACKS [u64;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "WHITE_PAWN_ATTACKS [u64;64]",
        bytes: 64 * 8,
    },
    Table {
        name: "BLACK_PAWN_ATTACKS [u64;64]",
        bytes: 64 * 8,
    },
    Table {
        // Magic { mask: u64, factor: u64, offset: usize } = 24 bytes on 64-bit.
        name: "ROOK_MAGICS [Magic;64]",
        bytes: 64 * 24,
    },
    Table {
        name: "BISHOP_MAGICS [Magic;64]",
        bytes: 64 * 24,
    },
];

/// Sum the byte sizes of a table set.
fn total(tables: &[Table]) -> usize {
    tables.iter().map(|t| t.bytes).sum()
}

/// Print the static-table footprint breakdown for both engines.
pub fn report() {
    println!("static lookup-table footprint (computed from known table shapes):");

    println!("  mce (hyperbola-quintessence sliders — no magic attack array):");
    for t in MCE_TABLES {
        println!("    {:<34} {:>9} B", t.name, t.bytes);
    }
    let mce_total = total(MCE_TABLES);
    println!(
        "    {:<34} {:>9} B  ({:.1} KiB)",
        "TOTAL",
        mce_total,
        mce_total as f64 / 1024.0,
    );

    println!("  shakmaty 0.27 (fixed-shift magic bitboards):");
    for t in SHAKMATY_TABLES {
        println!("    {:<34} {:>9} B", t.name, t.bytes);
    }
    let shak_total = total(SHAKMATY_TABLES);
    println!(
        "    {:<34} {:>9} B  ({:.1} KiB)",
        "TOTAL",
        shak_total,
        shak_total as f64 / 1024.0,
    );
}
