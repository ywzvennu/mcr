//! A generic, compile-time board geometry: the parallel generic layer of
//! [`Bitboard`] / [`Square`] primitives that supports board sizes beyond 8x8.
//!
//! This module is a **separate, parallel hierarchy** from the crate's concrete
//! [`crate::Bitboard`] / [`crate::Square`] types. Those concrete `u64` 8x8
//! types are frozen and untouched; nothing here re-parametrises them. The
//! generic layer exists for fairy variants that need a wider board, while the
//! standard 8x8 path keeps its proven, frozen codegen.
//!
//! The design follows `docs/fairy-variants-architecture.md` §2.3:
//!
//! - A [`Geometry`] is a compile-time description of a board — its width,
//!   height, square count, backing integer type, and the derived file / rank /
//!   edge masks. It is monomorphised per board, so there is no runtime
//!   dispatch.
//! - [`Bitboard<G>`] wraps `G::Bits` and carries the geometry's masks, giving
//!   set operations, iteration, and edge-masked directional shifts that work
//!   for any width (including non-power-of-two widths like 9 or 10).
//! - [`Square<G>`] is a thin `u8` newtype whose `file` / `rank` use `% WIDTH`
//!   and `/ WIDTH`; for an 8x8 geometry these const-fold to `& 7` / `>> 3`,
//!   identical to the concrete path.
//!
//! Two geometries are provided: [`Chess8x8`] (`Bits = u64`) and [`Cap10x8`]
//! (`Bits = u128`, ten files by eight ranks = 80 squares) which exercises the
//! `u128` backing and the non-power-of-two width.
//!
//! ```
//! use mce::geometry::{Bitboard, Chess8x8, Geometry, Square};
//!
//! let bb = Bitboard::<Chess8x8>::from_square(Square::new(0));
//! assert_eq!(bb.count(), 1);
//! assert_eq!(Chess8x8::SQUARES, 64);
//! ```

pub mod attacks;
mod backing;
mod bitboard;
mod board;
pub mod position;
mod role;
mod square;
pub mod variant;
pub mod variants;
mod wide_move;

pub use backing::BitboardBacking;
pub use bitboard::{Bitboard, Squares};
pub use board::{Board, ParseBoardError, WidePiece};
pub use position::{
    perft, perft_divide, GenericCastling, GenericGating, GenericPlacement, GenericPosition,
    GenericState, WideFenError, WideOutcome,
};
pub use role::{WideRole, OVERFLOW_PREFIX};
pub use square::Square;
pub use variant::{PromotionConfig, StandardChess, WideEndReason, WideRegion, WideVariant};
pub use variants::{
    Capablanca, CapablancaRules, Chak, ChakRules, Duck, DuckRules, Empire, EmpireRules, Grand,
    GrandRules, HoppelPoppel, HoppelPoppelRules, Janggi, JanggiRules, Knightmate, KnightmateRules,
    Makruk, MakrukRules, Manchu, ManchuRules, Minishogi, MinishogiRules, Minixiangqi,
    MinixiangqiRules, Orda, OrdaRules, Ordamirror, OrdamirrorRules, Seirawan, SeirawanRules, Shako,
    ShakoRules, Shatar, ShatarRules, Shinobi, ShinobiRules, Shogi, ShogiRules, Shogun, ShogunRules,
    Sittuyin, SittuyinRules, Spartan, SpartanRules, Synochess, SynochessRules, Xiangqi,
    XiangqiRules,
};
pub use wide_move::{GateRole, GateSquare, WideMove, WideMoveKind};

/// A compile-time board geometry.
///
/// Implementors are zero-sized marker types describing a board shape. Every
/// constant is `const`, so [`Bitboard<G>`] and [`Square<G>`] monomorphise to
/// straight-line code with the masks folded in — there is no runtime geometry
/// dispatch.
///
/// The derived masks ([`FILE_A_MASK`](Geometry::FILE_A_MASK),
/// [`LAST_FILE_MASK`](Geometry::LAST_FILE_MASK),
/// [`BOARD_MASK`](Geometry::BOARD_MASK)) are expressed over the backing integer
/// `Bits`; the [`geometry!`](crate::geometry) macro fills them in from `WIDTH`
/// and `HEIGHT` for you, so most implementors never write them by hand.
pub trait Geometry: Copy + 'static {
    /// The integer backing a [`Bitboard<Self>`]: `u64` for 8x8, `u128` for
    /// wider boards (up to 128 squares).
    type Bits: BitboardBacking;

    /// The number of files (board width).
    const WIDTH: u8;

    /// The number of ranks (board height).
    const HEIGHT: u8;

    /// The number of squares, `WIDTH * HEIGHT`. Must be `<= Bits::BITS`.
    const SQUARES: u8;

    /// A mask of the first file (file `0`): one bit set in every rank, at the
    /// low file. The concrete 8x8 analogue is `Bitboard::FILE_A`.
    const FILE_A_MASK: Self::Bits;

    /// A mask of the last file (file `WIDTH - 1`). The concrete 8x8 analogue is
    /// `Bitboard::FILE_H`. Used to stop east / diagonal shifts wrapping past
    /// the right edge.
    const LAST_FILE_MASK: Self::Bits;

    /// A mask of exactly the `SQUARES` low bits — every on-board square. This is
    /// the generic analogue of the concrete `Bitboard::FULL` (which, for the
    /// full-width `u64`, is `!0`).
    const BOARD_MASK: Self::Bits;
}

/// Builds the first-file mask for a board: one bit at file `0` of each of
/// `height` ranks, `width` apart, over a `u64` backing.
#[must_use]
pub const fn file_a_mask_u64(width: u8, height: u8) -> u64 {
    let mut mask: u64 = 0;
    let mut rank = 0u8;
    while rank < height {
        mask |= 1u64 << (rank * width);
        rank += 1;
    }
    mask
}

/// Builds the first-file mask for a board over a `u128` backing.
#[must_use]
pub const fn file_a_mask_u128(width: u8, height: u8) -> u128 {
    let mut mask: u128 = 0;
    let mut rank = 0u8;
    while rank < height {
        mask |= 1u128 << (rank * width);
        rank += 1;
    }
    mask
}

/// Builds the board mask (the `squares` low bits) over a `u64` backing.
///
/// `squares` must be `<= 64`; passing `64` yields `!0`.
#[must_use]
pub const fn board_mask_u64(squares: u8) -> u64 {
    if squares >= 64 {
        !0u64
    } else {
        (1u64 << squares) - 1
    }
}

/// Builds the board mask (the `squares` low bits) over a `u128` backing.
///
/// `squares` must be `<= 128`; passing `128` yields `!0`.
#[must_use]
pub const fn board_mask_u128(squares: u8) -> u128 {
    if squares >= 128 {
        !0u128
    } else {
        (1u128 << squares) - 1
    }
}

/// Defines a [`Geometry`] marker type with masks derived from its width and
/// height.
///
/// `$bits` selects the backing integer (`u64` or `u128`) and the matching
/// `const`-mask builders. The macro computes `SQUARES = WIDTH * HEIGHT`, the
/// first/last file masks, and the board mask, so callers only supply the
/// dimensions.
///
/// ```
/// use mce::{geometry, geometry::Geometry};
/// geometry!(
///     /// A ten-by-ten board on a `u128` backing.
///     Board10x10, u128, 10, 10
/// );
/// assert_eq!(Board10x10::SQUARES, 100);
/// ```
#[macro_export]
macro_rules! geometry {
    ($(#[$meta:meta])* $name:ident, u64, $width:expr, $height:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name;

        impl $crate::geometry::Geometry for $name {
            type Bits = u64;
            const WIDTH: u8 = $width;
            const HEIGHT: u8 = $height;
            const SQUARES: u8 = $width * $height;
            const FILE_A_MASK: u64 = $crate::geometry::file_a_mask_u64($width, $height);
            const LAST_FILE_MASK: u64 =
                $crate::geometry::file_a_mask_u64($width, $height) << ($width - 1);
            const BOARD_MASK: u64 = $crate::geometry::board_mask_u64($width * $height);
        }
    };
    ($(#[$meta:meta])* $name:ident, u128, $width:expr, $height:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name;

        impl $crate::geometry::Geometry for $name {
            type Bits = u128;
            const WIDTH: u8 = $width;
            const HEIGHT: u8 = $height;
            const SQUARES: u8 = $width * $height;
            const FILE_A_MASK: u128 = $crate::geometry::file_a_mask_u128($width, $height);
            const LAST_FILE_MASK: u128 =
                $crate::geometry::file_a_mask_u128($width, $height) << ($width - 1);
            const BOARD_MASK: u128 = $crate::geometry::board_mask_u128($width * $height);
        }
    };
}

geometry!(
    /// The standard 8x8 chessboard, backed by `u64`.
    ///
    /// This is the generic-layer mirror of the concrete [`crate::Bitboard`] /
    /// [`crate::Square`]; its operations are equivalent to the frozen `u64`
    /// path (see the equivalence tests). It exists so fairy code can be written
    /// once against the generic surface and instantiated at 8x8 with no
    /// regression — the concrete types are not re-parametrised.
    Chess8x8,
    u64,
    8,
    8
);

geometry!(
    /// A ten-files by eight-ranks board (80 squares), backed by `u128`.
    ///
    /// Proves the `u128` backing and a non-power-of-two width: edge-masked
    /// shifts must not wrap past the tenth file even though `10` is not a power
    /// of two.
    Cap10x8,
    u128,
    10,
    8
);

geometry!(
    /// A ten-files by ten-ranks board (100 squares), backed by `u128`.
    ///
    /// The board of Grand chess. It validates a **second** `u128` geometry: ten
    /// ranks as well as ten files, so a square index reaches `99` and a rank
    /// renders as two digits (`a10`). Like [`Cap10x8`] its width is the
    /// non-power-of-two `10`, so edge-masked shifts must not wrap past the tenth
    /// file; unlike it, its `HEIGHT` is also `10`, exercising the rank geometry
    /// at the top of the `u128`.
    Grand10x10,
    u128,
    10,
    10
);

geometry!(
    /// The Xiangqi (Chinese chess) board: nine files by ten ranks (90 squares),
    /// backed by `u128`.
    ///
    /// A **third** `u128` geometry and the first whose width and height differ
    /// with the width an odd non-power-of-two (`9`): a square index reaches `89`,
    /// the longest file spans ten cells, and edge-masked east/west shifts must not
    /// wrap past the ninth file. Pieces sit on the cells (not the intersections of
    /// the traditional board); the engine treats it as a 9-by-10 grid. Files run
    /// a..i, ranks 1..10.
    Xiangqi9x10,
    u128,
    9,
    10
);

geometry!(
    /// The Minixiangqi board: seven files by seven ranks (49 squares), backed by
    /// `u128`.
    ///
    /// A small odd-width (`7`) `u128` geometry hosting Minixiangqi — a 7x7
    /// reduction of Xiangqi with no river, advisors, or elephants. It reuses the
    /// Xiangqi cannon / horse / palace / flying-general machinery on a smaller
    /// grid: a square index reaches `48`, and edge-masked east/west shifts must
    /// not wrap past the seventh file. Files run a..g, ranks 1..7. The 3x3 palace
    /// sits on files c..e (the near three ranks of each side).
    Minixiangqi7x7,
    u128,
    7,
    7
);

geometry!(
    /// The Shogi (Japanese chess) board: nine files by nine ranks (81 squares),
    /// backed by `u128`.
    ///
    /// A square (9-by-9) `u128` geometry with the odd non-power-of-two width `9`: a
    /// square index reaches `80`, and edge-masked east/west shifts must not wrap
    /// past the ninth file. Files run a..i, ranks 1..9. It hosts Shogi, whose
    /// captured pieces enter a persistent hand and are dropped back onto the board.
    Shogi9x9,
    u128,
    9,
    9
);

geometry!(
    /// The Minishogi board: five files by five ranks (25 squares), backed by
    /// `u64`.
    ///
    /// The smallest fairy geometry so far and the first odd non-power-of-two
    /// width (`5`) on a `u64` backing: a square index reaches `24`, and
    /// edge-masked east/west shifts must not wrap past the fifth file. Files run
    /// a..e, ranks 1..5. It hosts Minishogi, which reuses Shogi's persistent
    /// capture-fed hand, drops, and far-rank promotion on a smaller board.
    Minishogi5x5,
    u64,
    5,
    5
);

#[cfg(test)]
mod tests {
    use super::{Bitboard, Cap10x8, Chess8x8, Geometry, Square};
    use crate::{Bitboard as CBitboard, Square as CSquare};
    use alloc::vec::Vec;

    // ----- Geometry constants -------------------------------------------------

    #[test]
    fn chess8x8_constants() {
        assert_eq!(Chess8x8::WIDTH, 8);
        assert_eq!(Chess8x8::HEIGHT, 8);
        assert_eq!(Chess8x8::SQUARES, 64);
        // Derived masks match the concrete frozen path.
        assert_eq!(Chess8x8::FILE_A_MASK, CBitboard::FILE_A.0);
        assert_eq!(Chess8x8::LAST_FILE_MASK, CBitboard::FILE_H.0);
        assert_eq!(Chess8x8::BOARD_MASK, CBitboard::FULL.0);
    }

    #[test]
    fn cap10x8_constants() {
        assert_eq!(Cap10x8::WIDTH, 10);
        assert_eq!(Cap10x8::HEIGHT, 8);
        assert_eq!(Cap10x8::SQUARES, 80);
        // FILE_A has one bit per rank.
        assert_eq!(Cap10x8::FILE_A_MASK.count_ones(), 8);
        assert_eq!(Cap10x8::LAST_FILE_MASK.count_ones(), 8);
        // BOARD_MASK is exactly the 80 low bits.
        assert_eq!(Cap10x8::BOARD_MASK.count_ones(), 80);
        assert_eq!(Bitboard::<Cap10x8>::FULL.count(), 80);
    }

    // ----- 8x8 equivalence with the concrete path -----------------------------

    /// Maps a generic 8x8 bitboard to the concrete one for direct comparison.
    fn c(bb: Bitboard<Chess8x8>) -> CBitboard {
        CBitboard(bb.0)
    }

    #[test]
    fn equivalence_full_and_empty() {
        assert_eq!(c(Bitboard::<Chess8x8>::EMPTY), CBitboard::EMPTY);
        assert_eq!(c(Bitboard::<Chess8x8>::FULL), CBitboard::FULL);
        assert_eq!(Bitboard::<Chess8x8>::FULL.count(), 64);
        assert_eq!(c(!Bitboard::<Chess8x8>::EMPTY), CBitboard::FULL);
        assert_eq!(c(!Bitboard::<Chess8x8>::FULL), CBitboard::EMPTY);
        assert_eq!(c(Bitboard::<Chess8x8>::FILE_A), CBitboard::FILE_A);
        assert_eq!(c(Bitboard::<Chess8x8>::LAST_FILE), CBitboard::FILE_H);
    }

    #[test]
    fn equivalence_set_ops_and_membership() {
        for index in 0..64u8 {
            let gs = Square::<Chess8x8>::new(index);
            let cs = CSquare::new(index);
            let gbb = Bitboard::<Chess8x8>::from_square(gs);
            let cbb = CBitboard::from_square(cs);
            assert_eq!(c(gbb), cbb);
            assert!(gbb.contains(gs));
            assert_eq!(gbb.count(), 1);

            let mut g = Bitboard::<Chess8x8>::EMPTY;
            let mut conc = CBitboard::EMPTY;
            g.set(gs);
            conc.set(cs);
            assert_eq!(c(g), conc);
            g.toggle(gs);
            conc.toggle(cs);
            assert_eq!(c(g), conc);
            assert!(g.is_empty() && conc.is_empty());

            assert_eq!(
                c(Bitboard::<Chess8x8>::EMPTY.with(gs)),
                CBitboard::EMPTY.with(cs)
            );
            assert_eq!(c(gbb.without(gs)), cbb.without(cs));
        }
    }

    #[test]
    fn equivalence_bitwise_and_shift_operators() {
        let ga = Bitboard::<Chess8x8>::FILE_A;
        let gr: Bitboard<Chess8x8> = (0..8).map(Square::<Chess8x8>::new).collect();
        let ca = CBitboard::FILE_A;
        let cr = CBitboard::RANK_1;
        assert_eq!(c(ga & gr), ca & cr);
        assert_eq!(c(ga | gr), ca | cr);
        assert_eq!(c(ga ^ gr), ca ^ cr);

        // Shift operators (masked to the board on the left, plain on the right).
        for s in 0..64u32 {
            let g = Bitboard::<Chess8x8>::FULL << s;
            let conc = CBitboard::FULL << s;
            assert_eq!(c(g), conc, "shl {s}");
            let g2 = Bitboard::<Chess8x8>::FULL >> s;
            let conc2 = CBitboard::FULL >> s;
            assert_eq!(c(g2), conc2, "shr {s}");
        }
    }

    #[test]
    fn equivalence_directional_shifts_over_all_squares() {
        // For every square, all eight edge-masked shifts must match the frozen
        // concrete bitboard exactly.
        for index in 0..64u8 {
            let g = Bitboard::<Chess8x8>::from_square(Square::new(index));
            let conc = CBitboard::from_square(CSquare::new(index));
            assert_eq!(c(g.north()), conc.north(), "north {index}");
            assert_eq!(c(g.south()), conc.south(), "south {index}");
            assert_eq!(c(g.east()), conc.east(), "east {index}");
            assert_eq!(c(g.west()), conc.west(), "west {index}");
            assert_eq!(c(g.north_east()), conc.north_east(), "ne {index}");
            assert_eq!(c(g.north_west()), conc.north_west(), "nw {index}");
            assert_eq!(c(g.south_east()), conc.south_east(), "se {index}");
            assert_eq!(c(g.south_west()), conc.south_west(), "sw {index}");
        }
        // And over a multi-bit pattern (the full board) to catch mask errors.
        let g = Bitboard::<Chess8x8>::FULL;
        let conc = CBitboard::FULL;
        assert_eq!(c(g.north()), conc.north());
        assert_eq!(c(g.east()), conc.east());
        assert_eq!(c(g.north_east()), conc.north_east());
        assert_eq!(c(g.south_west()), conc.south_west());
    }

    #[test]
    fn equivalence_iteration_and_lsb() {
        let indices = [0u8, 2, 27, 63, 36];
        let g: Bitboard<Chess8x8> = indices.iter().map(|&i| Square::new(i)).collect();
        let conc: CBitboard = indices.iter().map(|&i| CSquare::new(i)).collect();
        assert_eq!(c(g), conc);
        assert_eq!(g.lsb().map(|s| s.index()), conc.lsb().map(|s| s.index()));

        let g_iter: Vec<u8> = g.into_iter().map(|s| s.index()).collect();
        let c_iter: Vec<u8> = conc.into_iter().map(|s| s.index()).collect();
        assert_eq!(g_iter, c_iter);
        assert_eq!(g.into_iter().len(), conc.into_iter().len());

        let mut gw = g;
        let mut cw = conc;
        loop {
            match (gw.pop_lsb(), cw.pop_lsb()) {
                (Some(a), Some(b)) => assert_eq!(a.index(), b.index()),
                (None, None) => break,
                _ => panic!("pop_lsb diverged"),
            }
        }
    }

    #[test]
    fn equivalence_square_file_rank_offset() {
        for index in 0..64u8 {
            let g = Square::<Chess8x8>::new(index);
            let conc = CSquare::new(index);
            assert_eq!(g.file(), conc.file().index());
            assert_eq!(g.rank(), conc.rank().index());
            for df in -2i8..=2 {
                for dr in -2i8..=2 {
                    assert_eq!(
                        g.offset(df, dr).map(|s| s.index()),
                        conc.offset(df, dr).map(|s| s.index()),
                        "offset {index} {df} {dr}"
                    );
                }
            }
        }
    }

    // ----- u128 geometry: edge shifts on a non-power-of-two width --------------

    #[test]
    fn cap10x8_square_file_rank() {
        // index 9 = file 9, rank 0; index 10 = file 0, rank 1.
        let s9 = Square::<Cap10x8>::new(9);
        assert_eq!((s9.file(), s9.rank()), (9, 0));
        let s10 = Square::<Cap10x8>::new(10);
        assert_eq!((s10.file(), s10.rank()), (0, 1));
        let s79 = Square::<Cap10x8>::new(79);
        assert_eq!((s79.file(), s79.rank()), (9, 7));
        assert!(Square::<Cap10x8>::try_new(80).is_none());
        assert!(Square::<Cap10x8>::from_file_rank(10, 0).is_none());
    }

    #[test]
    fn cap10x8_east_does_not_leak_off_tenth_file() {
        // East off the last (tenth) file must vanish, not wrap to the next rank.
        let last_file = Bitboard::<Cap10x8>::LAST_FILE;
        assert_eq!(last_file.east(), Bitboard::<Cap10x8>::EMPTY);
        // A single square on the last file: east is empty.
        let s = Square::<Cap10x8>::new(9); // file 9, rank 0
        assert_eq!(
            Bitboard::<Cap10x8>::from_square(s).east(),
            Bitboard::<Cap10x8>::EMPTY
        );
        // West off the first file must vanish too.
        let first = Bitboard::<Cap10x8>::FILE_A;
        assert_eq!(first.west(), Bitboard::<Cap10x8>::EMPTY);

        // Interior moves are clean: file 0 east -> file 1 (index 1).
        let a0 = Square::<Cap10x8>::new(0);
        assert_eq!(
            Bitboard::<Cap10x8>::from_square(a0).east(),
            Bitboard::<Cap10x8>::from_square(Square::new(1))
        );
        // file 8 east -> file 9 (index 8 -> 9), still on rank 0.
        let f8 = Square::<Cap10x8>::new(8);
        assert_eq!(
            Bitboard::<Cap10x8>::from_square(f8).east(),
            Bitboard::<Cap10x8>::from_square(Square::new(9))
        );
    }

    #[test]
    fn cap10x8_north_south_and_diagonals() {
        // Square at file 5, rank 3 -> index 35.
        let s = Square::<Cap10x8>::from_file_rank(5, 3).unwrap();
        assert_eq!(s.index(), 35);
        let bb = Bitboard::<Cap10x8>::from_square(s);
        // North: +WIDTH = +10 -> index 45 (file 5, rank 4).
        assert_eq!(
            bb.north(),
            Bitboard::<Cap10x8>::from_square(Square::new(45))
        );
        // South: -10 -> index 25.
        assert_eq!(
            bb.south(),
            Bitboard::<Cap10x8>::from_square(Square::new(25))
        );
        // North-east: +WIDTH+1 = +11 -> index 46 (file 6, rank 4).
        assert_eq!(
            bb.north_east(),
            Bitboard::<Cap10x8>::from_square(Square::new(46))
        );
        // South-west: -11 -> index 24 (file 4, rank 2).
        assert_eq!(
            bb.south_west(),
            Bitboard::<Cap10x8>::from_square(Square::new(24))
        );
        // North off the top rank vanishes.
        let top = Square::<Cap10x8>::from_file_rank(5, 7).unwrap();
        assert_eq!(
            Bitboard::<Cap10x8>::from_square(top).north(),
            Bitboard::<Cap10x8>::EMPTY
        );
    }

    #[test]
    fn cap10x8_full_complement_stays_on_board() {
        // The complement of EMPTY is FULL and never sets off-board high bits.
        let full = !Bitboard::<Cap10x8>::EMPTY;
        assert_eq!(full, Bitboard::<Cap10x8>::FULL);
        assert_eq!(full.0 & !Cap10x8::BOARD_MASK, 0);
        assert_eq!(full.count(), 80);
        // Shifting FULL left by 1 then masking keeps off-board bits clear.
        let shifted = Bitboard::<Cap10x8>::FULL << 1;
        assert_eq!(shifted.0 & !Cap10x8::BOARD_MASK, 0);
    }

    #[test]
    fn cap10x8_iteration_is_complete_and_ordered() {
        let collected: Vec<u8> = Bitboard::<Cap10x8>::FULL
            .into_iter()
            .map(|s| s.index())
            .collect();
        assert_eq!(collected.len(), 80);
        for (i, idx) in collected.iter().enumerate() {
            assert_eq!(*idx as usize, i);
        }
    }

    // ----- Performance/codegen note (8x8 equivalence by value) ----------------

    /// A value-equivalence sweep standing in for the codegen gate: for a set of
    /// inputs, the generic 8x8 popcount and shifts equal the concrete ones.
    /// Because `Chess8x8::Bits = u64` and `BOARD_MASK = !0`, the generic masks
    /// fold away and the operations are the same `u64` instructions — the
    /// concrete frozen path is untouched, so the `compare/` numbers are
    /// unaffected.
    #[test]
    fn codegen_equivalence_popcount_and_shift_sweep() {
        let mut state: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..256 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let g = Bitboard::<Chess8x8>(state);
            let conc = CBitboard(state);
            assert_eq!(g.count(), conc.count());
            assert_eq!(c(g.north()), conc.north());
            assert_eq!(c(g.south()), conc.south());
            assert_eq!(c(g.east()), conc.east());
            assert_eq!(c(g.west()), conc.west());
            assert_eq!(c(g.north_east()), conc.north_east());
            assert_eq!(c(g.south_west()), conc.south_west());
            assert_eq!(c(!g), !conc);
        }
    }
}
