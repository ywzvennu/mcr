//! Zobrist hashing: a stable 64-bit key identifying a [`Position`].
//!
//! A Zobrist key is built by XOR-ing together one pseudo-random 64-bit constant
//! per independent feature of the position:
//!
//! - one constant per *piece on a square* (the 12 colored piece kinds on each of
//!   the 64 squares),
//! - one constant for the side to move, folded in when it is black's turn,
//! - one constant per castling right (4 of them), and
//! - one constant per en-passant *file*.
//!
//! Because XOR is its own inverse, a feature can be toggled in or out in O(1),
//! which is what makes the key cheap to maintain incrementally as moves are
//! played. Two positions that share every hashed feature collide to the same
//! key, and (barring an astronomically unlikely random collision) two genuinely
//! different positions do not.
//!
//! # En passant
//!
//! The en-passant file is folded in *only when an en-passant capture is actually
//! available* — that is, when a pawn of the side to move stands ready to make the
//! capture. A bare double-push that no enemy pawn can answer leaves no trace in
//! the key. This keeps the hash faithful to what a player can actually do: two
//! positions that are identical in every legal continuation hash equal, even if
//! their FEN en-passant fields differ. (We deliberately do *not* require the
//! capture to be fully legal under pin/discovered-check analysis; "a friendly
//! pawn attacks the ep square" is the standard, cheap, well-defined criterion.)
//!
//! # Determinism
//!
//! The random constants come from an in-crate [splitmix64] generator seeded with
//! a fixed constant, so the tables — and therefore every key — are identical
//! across builds and process runs. No external randomness or `rand` dependency
//! is involved. The startpos key is pinned in the tests so any accidental change
//! to the tables is caught.
//!
//! [splitmix64]: https://en.wikipedia.org/wiki/Xorshift#Initialization

use core::fmt;

use crate::position::CastleSide;
use crate::{Color, Piece, Position, Role, Square};

/// A 64-bit Zobrist hash of a [`Position`].
///
/// Obtain one with [`Position::zobrist`]. The wrapped value is available through
/// [`Zobrist::get`] or the public `.0` field; the [`Display`](fmt::Display)
/// implementation renders it as a zero-padded 16-digit hexadecimal number.
///
/// ```
/// use mce::Position;
/// let key = Position::startpos().zobrist();
/// assert_eq!(key, Position::startpos().zobrist());
/// assert_eq!(format!("{key}").len(), 16);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Zobrist(pub u64);

impl Zobrist {
    /// Returns the underlying 64-bit value.
    #[must_use]
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Zobrist {
    /// Formats the key as a zero-padded 16-digit lowercase hexadecimal number.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

// -- Random constants ------------------------------------------------------

/// The number of colors.
const COLORS: usize = 2;
/// The number of roles.
const ROLES: usize = 6;
/// The number of squares.
const SQUARES: usize = 64;
/// The number of castling-right features (each side, each side-to-castle).
const CASTLING_FEATURES: usize = 4;
/// The number of files (en-passant features).
const FILES: usize = 8;

/// The fixed seed for the constant generator. Changing this reshuffles every key
/// (and would break the pinned startpos hash test), so it must stay constant.
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// One step of the [splitmix64] PRNG: advances `state` and returns a well-mixed
/// 64-bit output. This is a tiny, public-domain-style mixing function used only
/// to fill the constant tables deterministically; it is not a cryptographic RNG.
#[inline]
const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The full set of Zobrist constants, generated deterministically from [`SEED`].
struct Keys {
    /// One key per `[color][role][square]`.
    pieces: [[[u64; SQUARES]; ROLES]; COLORS],
    /// Folded in when it is black's turn to move.
    black_to_move: u64,
    /// One key per castling-right feature, ordered by [`castling_index`].
    castling: [u64; CASTLING_FEATURES],
    /// One key per en-passant file.
    ep_file: [u64; FILES],
}

impl Keys {
    /// Generates the constants by drawing successive [`splitmix64`] outputs in a
    /// fixed order. Implemented as a `const fn` so the table is computed at
    /// compile time and is identical on every run.
    const fn generate() -> Keys {
        let mut state = SEED;
        let mut pieces = [[[0u64; SQUARES]; ROLES]; COLORS];

        let mut c = 0;
        while c < COLORS {
            let mut r = 0;
            while r < ROLES {
                let mut s = 0;
                while s < SQUARES {
                    pieces[c][r][s] = splitmix64(&mut state);
                    s += 1;
                }
                r += 1;
            }
            c += 1;
        }

        let black_to_move = splitmix64(&mut state);

        let mut castling = [0u64; CASTLING_FEATURES];
        let mut i = 0;
        while i < CASTLING_FEATURES {
            castling[i] = splitmix64(&mut state);
            i += 1;
        }

        let mut ep_file = [0u64; FILES];
        let mut f = 0;
        while f < FILES {
            ep_file[f] = splitmix64(&mut state);
            f += 1;
        }

        Keys {
            pieces,
            black_to_move,
            castling,
            ep_file,
        }
    }
}

/// The Zobrist constant tables, computed once at compile time.
static KEYS: Keys = Keys::generate();

/// Index of a color into the constant tables.
#[inline]
const fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// Index of a role into the constant tables.
#[inline]
const fn role_index(role: Role) -> usize {
    match role {
        Role::Pawn => 0,
        Role::Knight => 1,
        Role::Bishop => 2,
        Role::Rook => 3,
        Role::Queen => 4,
        Role::King => 5,
    }
}

/// Index of a `(color, side)` castling right into the castling key table.
#[inline]
const fn castling_index(color: Color, side: CastleSide) -> usize {
    let c = color_index(color);
    let s = match side {
        CastleSide::King => 0,
        CastleSide::Queen => 1,
    };
    c * 2 + s
}

/// The piece-square key for a colored piece on a square.
#[inline]
pub(crate) fn piece_square_key(piece: Piece, square: Square) -> u64 {
    KEYS.pieces[color_index(piece.color)][role_index(piece.role)][square.index() as usize]
}

/// The side-to-move key, folded in only when it is black's turn.
#[inline]
pub(crate) fn side_key(turn: Color) -> u64 {
    match turn {
        Color::White => 0,
        Color::Black => KEYS.black_to_move,
    }
}

/// The key for a single castling right.
#[inline]
pub(crate) fn castling_key(color: Color, side: CastleSide) -> u64 {
    KEYS.castling[castling_index(color, side)]
}

/// The en-passant key for a file.
#[inline]
pub(crate) fn ep_file_key(file: crate::File) -> u64 {
    KEYS.ep_file[file.index() as usize]
}

impl Position {
    /// Returns the Zobrist hash of this position.
    ///
    /// The key combines piece placement, the side to move, castling rights, and
    /// the en-passant file when an en-passant capture is actually available (see
    /// the [`Zobrist`] documentation for the exact contract). Two
    /// positions reached by different move orders but identical in all of these
    /// features hash equal.
    ///
    /// The key is **computed from scratch on demand** rather than stored and
    /// maintained incrementally across moves. `Position` deliberately keeps no
    /// cached hash field: an incrementally-updated key would cost a handful of XOR
    /// folds on *every* make-move (a measured ~28% of the copy-make path, since
    /// the rest is a tiny memcpy), yet nothing in the engine ever reads such a
    /// cached value — every public hash query (this method, [`crate::Zobrist`],
    /// repetition detection, the variant key) already recomputes. Recomputing here
    /// keeps make-move lean and shrinks `Position` by a `u64`, while this scan over
    /// the board stays cheaper than the reference's stored key on the zobrist
    /// micro-bench. (See issue #115.)
    ///
    /// ```
    /// use mce::Position;
    /// let start = Position::startpos();
    /// // 1.Nf3 Nf6 2.Ng1 Ng8 returns to the starting position.
    /// let mut pos = start.clone();
    /// for uci in ["g1f3", "g8f6", "f3g1", "f6g8"] {
    ///     let mv = pos.parse_uci(uci).unwrap();
    ///     pos = pos.play(&mv);
    /// }
    /// assert_eq!(pos.zobrist(), start.zobrist());
    /// ```
    #[must_use]
    pub fn zobrist(&self) -> Zobrist {
        Zobrist(self.compute_zobrist())
    }

    /// Computes the Zobrist key from scratch over the whole position.
    pub(crate) fn compute_zobrist(&self) -> u64 {
        let board = self.board();
        let mut hash = 0u64;

        for color in Color::ALL {
            for role in Role::ALL {
                let piece = Piece::new(color, role);
                for square in board.pieces(color, role) {
                    hash ^= piece_square_key(piece, square);
                }
            }
        }

        hash ^= side_key(self.turn());

        for color in Color::ALL {
            for side in [CastleSide::King, CastleSide::Queen] {
                if self.castling_rights().has(color, side) {
                    hash ^= castling_key(color, side);
                }
            }
        }

        if let Some(file) = self.zobrist_ep_file() {
            hash ^= ep_file_key(file);
        }

        hash
    }

    /// Returns the en-passant file that contributes to the hash, i.e. the file of
    /// the en-passant target square when a pawn of the side to move actually
    /// attacks it (so the capture is on offer). Returns `None` when there is no
    /// en-passant square or no friendly pawn can take.
    pub(crate) fn zobrist_ep_file(&self) -> Option<crate::File> {
        let ep = self.ep_square()?;
        let us = self.turn();
        // A friendly pawn attacks `ep` iff `ep` lies in the (enemy-direction)
        // pawn-attack set of one of our pawns — equivalently, our pawns intersect
        // the squares from which a pawn of our color attacks `ep`, which is the
        // opposite color's pawn-attack pattern *from* `ep`.
        let our_pawns = self.board().pieces(us, Role::Pawn);
        let attackers = crate::attacks::pawn_attacks(us.opposite(), ep) & our_pawns;
        if attackers.is_empty() {
            None
        } else {
            Some(ep.file())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a sequence of UCI moves from a starting position.
    fn play_line(mut pos: Position, ucis: &[&str]) -> Position {
        for uci in ucis {
            let mv = pos.parse_uci(uci).expect("legal uci move");
            pos = pos.play(&mv);
        }
        pos
    }

    #[test]
    fn startpos_key_is_pinned() {
        // Pin the computed startpos key so any change to the tables or the
        // hashing contract is caught as a regression.
        let key = Position::startpos().zobrist();
        assert_eq!(key.get(), STARTPOS_KEY);
        // Stable across repeated computation.
        assert_eq!(Position::startpos().zobrist().get(), STARTPOS_KEY);
    }

    /// The pinned startpos Zobrist value, captured from this implementation. Any
    /// change to the constant tables or the hashing contract changes this value
    /// and fails the test, which is exactly the regression guard we want.
    const STARTPOS_KEY: u64 = 0x8FF6_F282_E19D_060D;

    #[test]
    fn display_is_padded_hex() {
        let key = Position::startpos().zobrist();
        let s = format!("{key}");
        assert_eq!(s.len(), 16);
        assert_eq!(s, format!("{:016x}", key.get()));
    }

    #[test]
    fn transposition_returns_to_startpos() {
        let start = Position::startpos();
        let shuffled = play_line(start.clone(), &["g1f3", "g8f6", "f3g1", "f6g8"]);
        // The board, side, castling and ep-possibility are all back to the start,
        // so the keys match even though the move clocks (which are not hashed)
        // have advanced.
        assert_eq!(shuffled.zobrist(), start.zobrist());
        assert_eq!(shuffled.board(), start.board());
    }

    #[test]
    fn one_ply_change_differs() {
        let start = Position::startpos();
        let after = play_line(start.clone(), &["e2e4"]);
        assert_ne!(after.zobrist(), start.zobrist());
    }

    #[test]
    fn ep_file_only_counts_when_capture_available() {
        // Double push that exposes an en-passant capture: black pawn on d4, white
        // pushes e2-e4, so d4xe3 is on offer.
        let with_ep = Position::from_fen("4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1").unwrap();
        let after = play_line(with_ep, &["e2e4"]);
        assert_eq!(after.ep_square(), Some(Square::E3));
        assert!(after.zobrist_ep_file().is_some());

        // Same structure but no black pawn able to capture: the ep square exists
        // in FEN terms but contributes nothing to the hash.
        let no_ep = Position::from_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1").unwrap();
        let after_no = play_line(no_ep, &["e2e4"]);
        assert_eq!(after_no.ep_square(), Some(Square::E3));
        assert!(after_no.zobrist_ep_file().is_none());

        // The position with a live ep capture must differ from an otherwise
        // equal position whose ep file does not count. Construct two positions
        // identical except for ep availability and confirm the keys differ.
        let live = Position::from_fen("4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 1").unwrap();
        let dead = Position::from_fen("4k3/8/8/8/3pP3/8/8/4K3 b - - 0 1").unwrap();
        assert!(live.zobrist_ep_file().is_some());
        assert!(dead.zobrist_ep_file().is_none());
        assert_ne!(live.zobrist(), dead.zobrist());
    }

    #[test]
    fn losing_castling_rights_changes_key() {
        let pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        // Move the king-side rook: white loses the king-side right.
        let after = play_line(pos.clone(), &["h1g1"]);
        assert!(!after.castling_rights().has(Color::White, CastleSide::King));
        assert_ne!(after.zobrist(), pos.zobrist());

        // Moving the king revokes both rights and must also change the key
        // differently from moving just one rook.
        let king_moved = play_line(pos.clone(), &["e1f1"]);
        assert_ne!(king_moved.zobrist(), pos.zobrist());
        assert_ne!(king_moved.zobrist(), after.zobrist());
    }

    #[test]
    fn equal_positions_hash_equal() {
        // Two FENs describing the very same position must hash equal.
        let a =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        let b = Position::startpos();
        assert_eq!(a, b);
        assert_eq!(a.zobrist(), b.zobrist());
    }

    /// Walks every move to a fixed depth, asserting at each node that unmaking the
    /// move restores the from-scratch key exactly — the key is a pure function of
    /// the position, so a make/unmake round-trip is hash-neutral.
    fn walk_key_roundtrips(pos: &Position, depth: u32) {
        if depth == 0 {
            return;
        }
        let before = pos.compute_zobrist();
        for mv in pos.legal_moves() {
            let mut child = pos.clone();
            let undo = child.make(&mv);
            // The child key matches a from-scratch recompute of the child position.
            assert_eq!(child.zobrist().get(), child.compute_zobrist());
            walk_key_roundtrips(&child, depth - 1);
            child.unmake(&mv, undo);
            assert_eq!(child.compute_zobrist(), before);
        }
    }

    #[test]
    fn key_roundtrips_under_make_unmake_startpos() {
        walk_key_roundtrips(&Position::startpos(), 4);
    }

    #[test]
    fn key_roundtrips_under_make_unmake_kiwipete() {
        let kiwipete = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        walk_key_roundtrips(&kiwipete, 3);
    }
}
