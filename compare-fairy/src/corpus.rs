//! The shared-position corpus: identical positions run through both mcr and FSF.
//!
//! These FENs are reused from the mcr regression tests and the `compare/` (mcr
//! vs shakmaty) basket — opening / midgame / tactical / endgame shapes per
//! variant. Each carries an mcr [`VariantId`] (the FSF mapping + FEN dialect are
//! handled in [`crate::variants`]) and a per-position depth.
//!
//! Depths are kept modest by default because FSF's perft is the slower side of
//! the pair (it carries a far larger movegen); the goal is a broad correctness
//! cross-check plus a representative throughput sample, not a multi-minute deep
//! perft. `--full` deepens them (see `main.rs`).

use mcr::VariantId;

/// One shared corpus position.
#[derive(Debug, Clone, Copy)]
pub struct Case {
    /// The mcr variant.
    pub id: VariantId,
    /// Short human label, unique within the variant.
    pub label: &'static str,
    /// FEN in mcr dialect (the FSF dialect is derived in `variants::fen_to_fsf`).
    pub fen: &'static str,
    /// Default perft depth (deepened by `+1` in `--full`).
    pub depth: u32,
}

/// The shared corpus, grouped by variant. Every variant FSF and mcr both
/// support appears with several positions.
pub const CASES: &[Case] = &[
    // ---- standard ---------------------------------------------------------
    Case {
        id: VariantId::Standard,
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        id: VariantId::Standard,
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    Case {
        id: VariantId::Standard,
        label: "cpw3-rook-ep",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        depth: 5,
    },
    Case {
        id: VariantId::Standard,
        label: "cpw4-promotions",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        depth: 4,
    },
    // ---- chess960 (UCI_Chess960 + fischerandom; X-FEN castling letters) ----
    Case {
        id: VariantId::Chess960,
        label: "frc-id0",
        fen: "bqnb1rkr/pp3ppp/3ppn2/2p5/5P2/P2P4/NPP1P1PP/BQ1BNRKR w HFhf - 2 9",
        depth: 4,
    },
    Case {
        id: VariantId::Chess960,
        label: "frc-id2",
        fen: "b1q1rrkb/pppppppp/3nn3/8/P7/1PPP4/4PPPP/BQNNRKRB w GE - 1 9",
        depth: 4,
    },
    Case {
        id: VariantId::Chess960,
        label: "frc-id9",
        fen: "qn1rbbkr/ppp2p1p/1n1pp1p1/8/3P4/P6P/1PP1PPPK/QNNRBB1R w hd - 2 9",
        depth: 4,
    },
    // ---- king-of-the-hill -------------------------------------------------
    Case {
        id: VariantId::KingOfTheHill,
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        id: VariantId::KingOfTheHill,
        label: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        depth: 4,
    },
    // ---- three-check (mcr trailing W+B; reconciled to FSF after-ep) --------
    Case {
        id: VariantId::ThreeCheck,
        label: "startpos-3+3",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 3+3",
        depth: 5,
    },
    Case {
        id: VariantId::ThreeCheck,
        label: "kiwipete-3+3",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1 3+3",
        depth: 4,
    },
    // ---- racing-kings -----------------------------------------------------
    Case {
        id: VariantId::RacingKings,
        label: "startpos",
        fen: "8/8/8/8/8/8/krbnNBRK/qrbnNBRQ w - - 0 1",
        depth: 5,
    },
    // ---- atomic -----------------------------------------------------------
    Case {
        id: VariantId::Atomic,
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        depth: 5,
    },
    Case {
        id: VariantId::Atomic,
        label: "programfox-1",
        fen: "rn2kb1r/1pp1p2p/p2q1pp1/3P4/2P3b1/4PN2/PP3PPP/R2QKB1R b KQkq - 0 1",
        depth: 4,
    },
    // ---- antichess (FSF: giveaway) ----------------------------------------
    Case {
        id: VariantId::Antichess,
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
        depth: 5,
    },
    Case {
        id: VariantId::Antichess,
        label: "open-center",
        fen: "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/8/PPPP1PPP/RNBQKBNR w - - 2 3",
        depth: 5,
    },
    // ---- horde ------------------------------------------------------------
    Case {
        id: VariantId::Horde,
        label: "startpos",
        fen: "rnbqkbnr/pppppppp/8/1PP2PP1/PPPPPPPP/PPPPPPPP/PPPPPPPP/PPPPPPPP w kq - 0 1",
        depth: 5,
    },
    // ---- crazyhouse (bracketed pocket, identical in both) -----------------
    Case {
        id: VariantId::Crazyhouse,
        label: "middlegame",
        fen: "r1bqk2r/pppp1ppp/2n1p3/4P3/1b1Pn3/2NB1N2/PPP2PPP/R1BQK2R[] b KQkq -",
        depth: 4,
    },
    Case {
        id: VariantId::Crazyhouse,
        label: "drops-Qn",
        fen: "2k5/8/8/8/8/8/8/4K3[Qn] w - -",
        depth: 4,
    },
    Case {
        id: VariantId::Crazyhouse,
        label: "drops-many",
        fen: "2k5/8/8/8/8/8/8/4K3[QRBNqrbn] w - -",
        depth: 3,
    },
];

/// The ordered set of variants present in the corpus (for grouping output).
pub const VARIANTS: &[VariantId] = &[
    VariantId::Standard,
    VariantId::Chess960,
    VariantId::KingOfTheHill,
    VariantId::ThreeCheck,
    VariantId::RacingKings,
    VariantId::Atomic,
    VariantId::Antichess,
    VariantId::Horde,
    VariantId::Crazyhouse,
];
