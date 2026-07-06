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
//! use mcr::geometry::{Bitboard, Chess8x8, Geometry, Square};
//!
//! let bb = Bitboard::<Chess8x8>::from_square(Square::new(0));
//! assert_eq!(bb.count(), 1);
//! assert_eq!(Chess8x8::SQUARES, 64);
//! ```

mod analysis;
mod any;
pub mod attacks;
mod backing;
pub mod binary;
mod bitboard;
mod board;
pub mod book;
mod collection;
pub mod epd;
pub mod game;
mod notation;
pub mod position;
mod role;
mod square;
pub mod status;
pub mod variant;
pub mod variants;
mod wide_move;
mod zobrist;

pub use any::{AnyWideVariant, UnknownWideVariant, WideVariantId};
pub use backing::{BitboardBacking, U256};
pub use binary::{decode_game, encode_game, WireError};
pub use bitboard::{Bitboard, Squares};
pub use board::{Board, ParseBoardError, WidePiece};
pub use book::{weighted_pick, WideBook, WideBookEntry};
pub use collection::{WideGameRecord, WidePgnCollection};
pub use epd::{WideEpd, WideEpdError};
pub use game::{GenericGame, WideIllegalMove, COUNTING_LIMIT_PLIES};
pub use notation::{WidePgn, WidePgnError, WidePgnResult, WideSanError};
pub use position::{
    perft, perft_divide, GenericCastling, GenericGating, GenericPlacement, GenericPosition,
    GenericState, Undo, WideFenError, WideMoveList, WideOutcome,
};
pub use role::{WideRole, OVERFLOW_PREFIX, OVERFLOW_PREFIX_3};
pub use square::Square;
pub use status::GameStatus;
pub use variant::{
    DrawHooks, ExtinctionRule, PromotionConfig, RoyalSlider, StandardChess, WideCountingRule,
    WideEndReason, WideRegion, WideVariant,
};
pub use variants::{
    Aiwok, AiwokRules, Alice, AliceRules, Almost, AlmostRules, Amazon, AmazonRules, Asean,
    AseanRules, Bughouse, BughouseRules, Cambodian, CambodianRules, CannonShogi, CannonShogiRules,
    Capablanca, CapablancaRules, Capahouse, CapahouseRules, Caparandom, CaparandomRules, Centaur,
    CentaurRules, Chak, ChakRules, Chancellor, ChancellorRules, CheckShogi, CheckShogiRules,
    Chennis, ChennisRules, Chigorin, ChigorinRules, Chu, ChuRules, Coregal, CoregalRules, Courier,
    CourierRules, Dai, DaiRules, Dobutsu, DobutsuRules, Dragon, DragonRules, Duck, DuckRules,
    Embassy, EmbassyRules, Empire, EmpireRules, EuroShogi, EuroShogiRules, Extinction,
    ExtinctionRules, FogOfWar, FogOfWarRules, Gorogoro, GorogoroRules, Gothic, GothicRules, Grand,
    GrandRules, Grandhouse, GrandhouseRules, HoppelPoppel, HoppelPoppelRules, Janggi, JanggiRules,
    Janus, JanusRules, Jieqi, JieqiRules, Judkins, JudkinsRules, Karouk, KaroukRules, Khans,
    KhansRules, Knightmate, KnightmateRules, Kyotoshogi, KyotoshogiRules, Makpong, MakpongRules,
    Makruk, MakrukRules, Manchu, ManchuRules, Mansindam, MansindamRules, Micro, MicroRules,
    Minishogi, MinishogiRules, Minixiangqi, MinixiangqiRules, Modern, ModernRules, Nocastle,
    NocastleRules, Opulent, OpulentRules, Orda, OrdaRules, Ordamirror, OrdamirrorRules, Placement,
    PlacementRules, Pocketknight, PocketknightRules, Seirawan, SeirawanRules, Shako, ShakoRules,
    Shatar, ShatarRules, Shatranj, ShatranjRules, Shinobi, ShinobiRules, ShoShogi, ShoShogiRules,
    Shogi, ShogiRules, Shogun, ShogunRules, Shouse, ShouseRules, Sittuyin, SittuyinRules, Spartan,
    SpartanRules, Synochess, SynochessRules, Tencubed, TencubedRules, Tenjiku, TenjikuRules, Tori,
    ToriRules, Washogi, WashogiRules, Xiangfu, XiangfuRules, Xiangqi, XiangqiRules,
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
/// `Bits`; the [`geometry!`](crate::geometry!) macro fills them in from `WIDTH`
/// and `HEIGHT` for you, so most implementors never write them by hand.
pub trait Geometry: Copy + 'static {
    /// The integer backing a [`Bitboard<Self>`]: `u64` for 8x8, `u128` for
    /// wider boards up to 128 squares, and [`U256`] beyond that (e.g. 12x12 Chu
    /// Shogi at 144 squares).
    type Bits: BitboardBacking;

    /// The number of files (board width).
    const WIDTH: u8;

    /// The number of ranks (board height).
    const HEIGHT: u8;

    /// The number of squares, `WIDTH * HEIGHT`. Must be `<= Bits::BITS`.
    ///
    /// This is a **`u16`**, not a `u8`: the 16x16 Tenjiku board has `256` squares,
    /// which is exactly one past `u8::MAX`. Square *indices* stay `u8` (they reach
    /// `255`, the last on-board square), but the square *count* needs the wider
    /// type at the boundary.
    const SQUARES: u16;

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

/// Builds the first-file mask for a board over a [`U256`] backing.
///
/// One bit at file `0` of each of `height` ranks, `width` apart. Used by boards
/// wider than 128 squares (e.g. 12x12 Chu Shogi); cannot use the operator impls
/// since those are not `const`, so it accumulates into the limbs directly.
#[must_use]
pub const fn file_a_mask_u256(width: u8, height: u8) -> U256 {
    let mut lo: u128 = 0;
    let mut hi: u128 = 0;
    let mut rank = 0u8;
    while rank < height {
        let n = (rank * width) as u32;
        if n < 128 {
            lo |= 1u128 << n;
        } else {
            hi |= 1u128 << (n - 128);
        }
        rank += 1;
    }
    U256::from_parts(lo, hi)
}

/// Builds the last-file mask (file `width - 1`) for a board over a [`U256`]
/// backing.
#[must_use]
pub const fn last_file_mask_u256(width: u8, height: u8) -> U256 {
    let mut lo: u128 = 0;
    let mut hi: u128 = 0;
    let mut rank = 0u8;
    while rank < height {
        // Compute in u32: at the 16x16 boundary `rank*width + width` reaches 256,
        // which would overflow the u8 intermediate before the `- 1`.
        let n = rank as u32 * width as u32 + width as u32 - 1;
        if n < 128 {
            lo |= 1u128 << n;
        } else {
            hi |= 1u128 << (n - 128);
        }
        rank += 1;
    }
    U256::from_parts(lo, hi)
}

/// Builds the board mask (the `squares` low bits) over a [`U256`] backing.
///
/// `squares` must be `<= 256`; passing `256` yields all ones. Takes a `u16`
/// (not a `u8`) so the 256-square 16x16 Tenjiku board — whose count is exactly one
/// past `u8::MAX` — can request its all-ones mask without overflow.
#[must_use]
pub const fn board_mask_u256(squares: u16) -> U256 {
    let s = squares as u32;
    if s >= 256 {
        U256::from_parts(!0u128, !0u128)
    } else if s >= 128 {
        // All low bits set; the high limb holds the remaining `s - 128` bits.
        let hi = if s == 128 {
            0
        } else {
            (1u128 << (s - 128)) - 1
        };
        U256::from_parts(!0u128, hi)
    } else {
        U256::from_parts((1u128 << s) - 1, 0)
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
/// use mcr::{geometry, geometry::Geometry};
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
            const SQUARES: u16 = $width as u16 * $height as u16;
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
            const SQUARES: u16 = $width as u16 * $height as u16;
            const FILE_A_MASK: u128 = $crate::geometry::file_a_mask_u128($width, $height);
            const LAST_FILE_MASK: u128 =
                $crate::geometry::file_a_mask_u128($width, $height) << ($width - 1);
            const BOARD_MASK: u128 = $crate::geometry::board_mask_u128($width * $height);
        }
    };
    ($(#[$meta:meta])* $name:ident, u256, $width:expr, $height:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name;

        impl $crate::geometry::Geometry for $name {
            type Bits = $crate::geometry::U256;
            const WIDTH: u8 = $width;
            const HEIGHT: u8 = $height;
            const SQUARES: u16 = $width as u16 * $height as u16;
            const FILE_A_MASK: $crate::geometry::U256 =
                $crate::geometry::file_a_mask_u256($width, $height);
            const LAST_FILE_MASK: $crate::geometry::U256 =
                $crate::geometry::last_file_mask_u256($width, $height);
            const BOARD_MASK: $crate::geometry::U256 =
                $crate::geometry::board_mask_u256($width as u16 * $height as u16);
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
    /// A nine-files by nine-ranks chess board (81 squares), backed by `u128`.
    ///
    /// The board of Chancellor chess — standard chess widened to nine files and
    /// nine ranks, with a Rook + Knight Chancellor added to each side's back rank.
    /// It shares the 9x9 shape and `u128` backing with [`Shogi9x9`] but is a
    /// **distinct** geometry, so the western-chess variant never shares its file /
    /// rank / edge masks with the shogi family (mirroring how [`Tori7x7`],
    /// [`Minixiangqi7x7`] and [`Chennis7x7`] keep separate 7x7 geometries). A square
    /// index reaches `80`, and edge-masked east/west shifts must not wrap past the
    /// ninth file. Files run a..i, ranks 1..9.
    Chess9x9,
    u128,
    9,
    9
);

geometry!(
    /// The Courier chess board: twelve files by eight ranks (96 squares), backed
    /// by `u128`.
    ///
    /// A **fourth** `u128` geometry and the widest single-limb board: twelve files
    /// (the non-power-of-two width `12`) by eight ranks fill `12 * 8 = 96 <= 128`
    /// bits, so it stays within a single `u128` (no [`U256`] needed). A square
    /// index reaches `95`, and edge-masked east/west shifts must not wrap past the
    /// twelfth file. Files run a..l, ranks 1..8. It hosts Courier chess — the
    /// medieval widening of chess with the short-range Alfil, Ferz, Wazir, and
    /// (non-royal) Man alongside the modern Rook, Knight, and Bishop.
    Courier12x8,
    u128,
    12,
    8
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

geometry!(
    /// The Judkins Shogi board: six files by six ranks (36 squares), backed by
    /// `u64`.
    ///
    /// A small even-width (`6`) `u64` geometry hosting Judkins Shogi — a 6x6
    /// reduction of Shogi with one each of King, Gold and Silver General, Knight,
    /// Bishop, Rook, and Pawn (no Lance). A square index reaches `35`, and
    /// edge-masked east/west shifts must not wrap past the sixth file. Files run
    /// a..f, ranks 1..6. It reuses Shogi's persistent capture-fed hand, drops, and
    /// promotion on the smaller board, with a two-rank promotion zone.
    Judkins6x6,
    u64,
    6,
    6
);

geometry!(
    /// The Micro Shogi board: four files by five ranks (20 squares), backed by
    /// `u64`.
    ///
    /// A tiny even-width (`4`) `u64` geometry hosting Micro Shogi — a 4x5 Shogi
    /// whose pieces start pre-promoted and flip form (promote / demote) whenever
    /// they capture. A square index reaches `19`, and edge-masked east/west shifts
    /// must not wrap past the fourth file. Files run a..d, ranks 1..5. It reuses
    /// Shogi's persistent capture-fed hand and drops (in either form) on the
    /// smaller board.
    Micro4x5,
    u64,
    4,
    5
);

geometry!(
    /// The Tori Shogi (bird shogi) board: seven files by seven ranks (49 squares),
    /// backed by `u128`.
    ///
    /// A small odd-width (`7`) `u128` geometry hosting Tori Shogi — a 7x7
    /// bird-themed shogi with the full Shogi persistent capture-fed hand, drops,
    /// and far-zone promotion, but a bird army (swallow, goose, falcon, eagle,
    /// crane, two quails, pheasant) in place of the Shogi pieces. The same board
    /// size as [`Minixiangqi7x7`] but a distinct geometry, so the Tori army never
    /// shares masks with the Xiangqi-on-7x7 palace/river machinery. A square index
    /// reaches `48`, and edge-masked east/west shifts must not wrap past the
    /// seventh file. Files run a..g, ranks 1..7.
    Tori7x7,
    u128,
    7,
    7
);

geometry!(
    /// The Dobutsu board: three files by four ranks (12 squares), backed by `u64`.
    ///
    /// The smallest fairy geometry — a 3-by-4 micro board with the odd
    /// non-power-of-two width `3`: a square index reaches `11`, and edge-masked
    /// east/west shifts must not wrap past the third file. Files run a..c, ranks
    /// 1..4. It hosts Dobutsu (3x4 animal shogi), which reuses Shogi's persistent
    /// capture-fed hand, drops, and far-rank chick promotion, and adds a
    /// non-royal Lion that wins by reaching — and being safe on — the far rank.
    Dobutsu3x4,
    u64,
    3,
    4
);

geometry!(
    /// The Gorogoro Shogi board: five files by six ranks (30 squares), backed by
    /// `u64`.
    ///
    /// A small odd-width (`5`) `u64` geometry hosting Gorogoro Shogi Plus — a
    /// compact Shogi played on a five-by-six board: a square index reaches `29`,
    /// and edge-masked east/west shifts must not wrap past the fifth file. Files
    /// run a..e, ranks 1..6. It reuses Shogi's persistent capture-fed hand,
    /// drops, far-zone promotion, Lance, and Shogi Knight on the smaller board,
    /// with a two-rank promotion zone and the Lance/Knight pair starting in hand.
    Gorogoro5x6,
    u64,
    5,
    6
);

geometry!(
    /// The Chennis board: seven files by seven ranks (49 squares), backed by
    /// `u128`.
    ///
    /// A small odd-width (`7`) `u128` geometry hosting Chennis — a tennis-themed
    /// Kyoto-Shogi-like flipping variant with a persistent capture-fed **hand**,
    /// **dual-form drops**, and a **king mobility region** (each side's King is
    /// confined to files b..f on its own and the two central ranks). The same
    /// board size as [`Minixiangqi7x7`] / [`Tori7x7`] but a distinct geometry, so
    /// the Chennis army never shares masks with the Xiangqi-on-7x7 palace/river or
    /// the Tori bird machinery. A square index reaches `48`, and edge-masked
    /// east/west shifts must not wrap past the seventh file. Files run a..g, ranks
    /// 1..7.
    Chennis7x7,
    u128,
    7,
    7
);

geometry!(
    /// The Chu Shogi board: twelve files by twelve ranks (144 squares), backed
    /// by [`U256`].
    ///
    /// This is the first geometry to exceed the 128-square ceiling of a `u128`
    /// backing: `12 * 12 = 144 > 128`, so it uses the two-limb [`U256`] backing,
    /// exercising the limb-boundary shifts and high-square masking. A square
    /// index reaches `143` (in the high limb), and edge-masked east/west shifts
    /// must not wrap past the twelfth file. Files run a..l, ranks 1..12.
    ///
    /// It hosts Chu Shogi — the 12x12 large shogi with no drops, a deep promotion
    /// zone, ~21 piece types including ranging sliders and the double-moving
    /// **Lion**. The geometry is reused by the still-larger shogi variants.
    Chu12x12,
    u256,
    12,
    12
);

geometry!(
    /// The Dai Shogi board: fifteen files by fifteen ranks (225 squares), backed
    /// by [`U256`].
    ///
    /// The largest board yet: `15 * 15 = 225`, comfortably inside the 256-bit
    /// two-limb [`U256`] backing (a square index reaches `224`, in the high limb),
    /// exercising the limb-boundary shifts and high-square masking on an odd width.
    /// Edge-masked east/west shifts must not wrap past the fifteenth file. Files run
    /// a..o, ranks 1..15.
    ///
    /// It hosts Dai Shogi — Chu Shogi widened to 15x15: no drops, a deep five-rank
    /// promotion zone, ~29 piece types including the ranging sliders, the
    /// double-moving **Lion**, and a handful of extra short-range generals (Iron,
    /// Stone, Violent Ox, Flying Dragon, Evil Wolf).
    Dai15x15,
    u256,
    15,
    15
);

geometry!(
    /// The Tenjiku Shogi board: sixteen files by sixteen ranks (256 squares),
    /// backed by [`U256`].
    ///
    /// The largest board in the project and a **boundary geometry**: `16 * 16 =
    /// 256` squares fill the two-limb [`U256`] backing **exactly**, so a square
    /// index reaches `255` — the very top bit of the high limb — and
    /// [`BOARD_MASK`](Geometry::BOARD_MASK) is all-ones across both limbs
    /// (`count_ones() == 256`). This is the one geometry whose square count equals
    /// its backing width, so the edge-masked east/west shifts and the full-board
    /// mask are exercised right at the 256-bit ceiling (Chu at 144 and Dai at 225
    /// never reach it). Edge-masked east/west shifts must not wrap past the
    /// sixteenth file. Files run a..p, ranks 1..16.
    ///
    /// It hosts Tenjiku Shogi — the enormous 16x16 large shogi: no drops, a deep
    /// promotion zone, ~36 piece types including the ranging sliders, the
    /// double-moving **Lion**, the jump-capturing **Generals**, and the
    /// area-burning **Fire Demon**.
    Tenjiku16x16,
    u256,
    16,
    16
);

geometry!(
    /// The Wa Shogi board: eleven files by eleven ranks (121 squares), backed by
    /// `u128`.
    ///
    /// An odd non-power-of-two width `11` on a `u128` backing: `11 * 11 = 121 <=
    /// 128`, so it fits the single-limb `u128` (no [`U256`] needed). A square index
    /// reaches `120`, and edge-masked east/west shifts must not wrap past the
    /// eleventh file. Files run a..k, ranks 1..11. It hosts Wa Shogi, the
    /// animal-themed large shogi, whose captured pieces enter a persistent hand and
    /// are dropped back onto the board.
    Washogi11x11,
    u128,
    11,
    11
);

#[cfg(test)]
mod tests {
    use super::{
        Bitboard, BitboardBacking, Cap10x8, Chess8x8, Chu12x12, Courier12x8, Dai15x15, Geometry,
        Square, Tenjiku16x16,
    };
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

    #[test]
    fn courier12x8_constants() {
        assert_eq!(Courier12x8::WIDTH, 12);
        assert_eq!(Courier12x8::HEIGHT, 8);
        assert_eq!(Courier12x8::SQUARES, 96);
        // One bit per rank on the first/last file.
        assert_eq!(Courier12x8::FILE_A_MASK.count_ones(), 8);
        assert_eq!(Courier12x8::LAST_FILE_MASK.count_ones(), 8);
        // BOARD_MASK is exactly the 96 low bits.
        assert_eq!(Courier12x8::BOARD_MASK.count_ones(), 96);
        assert_eq!(Bitboard::<Courier12x8>::FULL.count(), 96);
    }

    #[test]
    fn courier12x8_east_does_not_leak_off_twelfth_file() {
        // East off the last (twelfth) file must vanish, not wrap to the next rank.
        assert_eq!(
            Bitboard::<Courier12x8>::LAST_FILE.east(),
            Bitboard::<Courier12x8>::EMPTY
        );
        assert_eq!(
            Bitboard::<Courier12x8>::FILE_A.west(),
            Bitboard::<Courier12x8>::EMPTY
        );
        // Interior east: file 10 (index 10, rank 0) -> file 11 (index 11).
        let f10 = Square::<Courier12x8>::from_file_rank(10, 0).unwrap();
        assert_eq!(
            Bitboard::<Courier12x8>::from_square(f10).east(),
            Bitboard::<Courier12x8>::from_square(Square::from_file_rank(11, 0).unwrap())
        );
        // Last square index is 95 (file 11, rank 7); north off the top vanishes.
        let top = Square::<Courier12x8>::new(95);
        assert_eq!((top.file(), top.rank()), (11, 7));
        assert_eq!(
            Bitboard::<Courier12x8>::from_square(top).north(),
            Bitboard::<Courier12x8>::EMPTY
        );
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

    // ----- u256 geometry: 144 squares, crossing the 128-bit limb seam ---------

    #[test]
    fn chu12x12_constants() {
        assert_eq!(Chu12x12::WIDTH, 12);
        assert_eq!(Chu12x12::HEIGHT, 12);
        assert_eq!(Chu12x12::SQUARES, 144);
        // One bit per rank on the first/last file.
        assert_eq!(Chu12x12::FILE_A_MASK.count_ones(), 12);
        assert_eq!(Chu12x12::LAST_FILE_MASK.count_ones(), 12);
        // BOARD_MASK is exactly the 144 low bits, spanning both limbs.
        assert_eq!(Chu12x12::BOARD_MASK.count_ones(), 144);
        assert_eq!(Bitboard::<Chu12x12>::FULL.count(), 144);
        // No off-board high bits survive in FULL.
        assert_eq!(
            !Bitboard::<Chu12x12>::FULL & Bitboard::<Chu12x12>::FULL,
            Bitboard::EMPTY
        );
    }

    #[test]
    fn chu12x12_square_file_rank_high_squares() {
        // Index 131 lives in the high limb: file 11, rank 10.
        let s = Square::<Chu12x12>::from_file_rank(11, 10).unwrap();
        assert_eq!(s.index(), 131);
        assert_eq!((s.file(), s.rank()), (11, 10));
        // The very last square, index 143, also in the high limb.
        let last = Square::<Chu12x12>::new(143);
        assert_eq!((last.file(), last.rank()), (11, 11));
        assert!(Square::<Chu12x12>::try_new(144).is_none());
        assert!(Square::<Chu12x12>::from_file_rank(12, 0).is_none());
    }

    #[test]
    fn chu12x12_north_south_cross_the_limb_seam() {
        // File 5, rank 9 -> index 113 (low limb). North (+12) -> index 125 still
        // low; another north -> index 137, which is in the HIGH limb: the shift
        // must carry across the 128-bit seam.
        let s = Square::<Chu12x12>::from_file_rank(5, 9).unwrap();
        assert_eq!(s.index(), 113);
        let n1 = Bitboard::<Chu12x12>::from_square(s).north();
        assert_eq!(n1, Bitboard::from_square(Square::new(125)));
        let n2 = n1.north();
        assert_eq!(n2.0.hi, 1u128 << (137 - 128)); // landed in the high limb
        assert_eq!(n2.0.lo, 0);
        assert_eq!(n2, Bitboard::from_square(Square::new(137)));
        // South back across the seam returns to the low limb.
        assert_eq!(n2.south(), n1);
        assert_eq!(n1.south(), Bitboard::from_square(s));
        // North off the top rank vanishes (no wrap into off-board high bits).
        let top = Bitboard::<Chu12x12>::from_square(Square::new(143));
        assert_eq!(top.north(), Bitboard::EMPTY);
    }

    #[test]
    fn chu12x12_east_west_do_not_leak_across_files_in_high_limb() {
        // Last file, top ranks (high-limb squares): east must vanish.
        assert_eq!(Bitboard::<Chu12x12>::LAST_FILE.east(), Bitboard::EMPTY);
        assert_eq!(Bitboard::<Chu12x12>::FILE_A.west(), Bitboard::EMPTY);
        // A high-limb last-file square (file 11, rank 11 = index 143): east empty.
        let hi = Square::<Chu12x12>::new(143);
        assert_eq!(
            Bitboard::<Chu12x12>::from_square(hi).east(),
            Bitboard::EMPTY
        );
        // Interior high-limb move: file 0, rank 11 (index 132) east -> index 133.
        let a11 = Square::<Chu12x12>::from_file_rank(0, 11).unwrap();
        assert_eq!(a11.index(), 132);
        assert_eq!(
            Bitboard::<Chu12x12>::from_square(a11).east(),
            Bitboard::from_square(Square::new(133))
        );
    }

    #[test]
    fn dai15x15_constants() {
        assert_eq!(Dai15x15::WIDTH, 15);
        assert_eq!(Dai15x15::HEIGHT, 15);
        assert_eq!(Dai15x15::SQUARES, 225);
        // One bit per rank on the first/last file.
        assert_eq!(Dai15x15::FILE_A_MASK.count_ones(), 15);
        assert_eq!(Dai15x15::LAST_FILE_MASK.count_ones(), 15);
        // BOARD_MASK is exactly the 225 low bits, spanning both limbs.
        assert_eq!(Dai15x15::BOARD_MASK.count_ones(), 225);
        assert_eq!(Bitboard::<Dai15x15>::FULL.count(), 225);
        // No off-board high bits survive in FULL.
        assert_eq!(
            !Bitboard::<Dai15x15>::FULL & Bitboard::<Dai15x15>::FULL,
            Bitboard::EMPTY
        );
    }

    #[test]
    fn dai15x15_square_file_rank_high_squares() {
        // Index 129 lives in the high limb: file 9, rank 8.
        let s = Square::<Dai15x15>::from_file_rank(9, 8).unwrap();
        assert_eq!(s.index(), 129);
        assert_eq!((s.file(), s.rank()), (9, 8));
        // The very last square, index 224, is file 14, rank 14.
        let last = Square::<Dai15x15>::new(224);
        assert_eq!((last.file(), last.rank()), (14, 14));
        assert!(Square::<Dai15x15>::try_new(225).is_none());
        assert!(Square::<Dai15x15>::from_file_rank(15, 0).is_none());
    }

    #[test]
    fn dai15x15_north_south_cross_the_limb_seam() {
        // File 0, rank 7 -> index 105 (low limb). North (+15) -> 120 still low;
        // another north -> 135, which is in the HIGH limb: the shift must carry
        // across the 128-bit seam.
        let s = Square::<Dai15x15>::from_file_rank(0, 7).unwrap();
        assert_eq!(s.index(), 105);
        let n1 = Bitboard::<Dai15x15>::from_square(s).north();
        assert_eq!(n1, Bitboard::from_square(Square::new(120)));
        let n2 = n1.north();
        assert_eq!(n2.0.hi, 1u128 << (135 - 128)); // landed in the high limb
        assert_eq!(n2.0.lo, 0);
        assert_eq!(n2, Bitboard::from_square(Square::new(135)));
        // South back across the seam returns to the low limb.
        assert_eq!(n2.south(), n1);
        // North off the top rank vanishes (no wrap into off-board high bits).
        let top = Bitboard::<Dai15x15>::from_square(Square::new(224));
        assert_eq!(top.north(), Bitboard::EMPTY);
    }

    #[test]
    fn dai15x15_east_west_do_not_leak_across_files_in_high_limb() {
        // Last file, top ranks (high-limb squares): east must vanish.
        assert_eq!(Bitboard::<Dai15x15>::LAST_FILE.east(), Bitboard::EMPTY);
        assert_eq!(Bitboard::<Dai15x15>::FILE_A.west(), Bitboard::EMPTY);
        // The high-limb last-file square (file 14, rank 14 = index 224): east empty.
        let hi = Square::<Dai15x15>::new(224);
        assert_eq!(
            Bitboard::<Dai15x15>::from_square(hi).east(),
            Bitboard::EMPTY
        );
        // Interior high-limb move: file 0, rank 14 (index 210) east -> index 211.
        let a15 = Square::<Dai15x15>::from_file_rank(0, 14).unwrap();
        assert_eq!(a15.index(), 210);
        assert_eq!(
            Bitboard::<Dai15x15>::from_square(a15).east(),
            Bitboard::from_square(Square::new(211))
        );
    }

    // ----- u256 boundary geometry: 256 squares fill both limbs exactly ---------

    #[test]
    fn tenjiku16x16_constants_fill_the_backing_exactly() {
        assert_eq!(Tenjiku16x16::WIDTH, 16);
        assert_eq!(Tenjiku16x16::HEIGHT, 16);
        assert_eq!(Tenjiku16x16::SQUARES, 256);
        // One bit per rank on the first/last file.
        assert_eq!(Tenjiku16x16::FILE_A_MASK.count_ones(), 16);
        assert_eq!(Tenjiku16x16::LAST_FILE_MASK.count_ones(), 16);
        // BOARD_MASK is *all 256 bits* — the whole U256, both limbs all-ones. This
        // is the boundary case Chu (144) and Dai (225) never reach.
        assert_eq!(Tenjiku16x16::BOARD_MASK.count_ones(), 256);
        assert_eq!(Tenjiku16x16::BOARD_MASK.lo, u128::MAX);
        assert_eq!(Tenjiku16x16::BOARD_MASK.hi, u128::MAX);
        assert_eq!(Bitboard::<Tenjiku16x16>::FULL.count(), 256);
        // The complement of FULL is EMPTY — there are no off-board high bits to
        // survive, since every bit is on-board at 256 squares.
        assert_eq!(!Bitboard::<Tenjiku16x16>::FULL, Bitboard::EMPTY);
        assert_eq!(
            !Bitboard::<Tenjiku16x16>::EMPTY,
            Bitboard::<Tenjiku16x16>::FULL
        );
    }

    #[test]
    fn tenjiku16x16_top_square_index_255() {
        // The very last square, index 255, is the top bit of the high limb: file
        // 15, rank 15.
        let last = Square::<Tenjiku16x16>::new(255);
        assert_eq!((last.file(), last.rank()), (15, 15));
        assert!(Square::<Tenjiku16x16>::try_new(255).is_some());
        assert!(Square::<Tenjiku16x16>::from_file_rank(16, 0).is_none());
        // Its bitboard is exactly the top bit of the high limb.
        let bb = Bitboard::<Tenjiku16x16>::from_square(last);
        assert_eq!(bb.0.hi, 1u128 << 127);
        assert_eq!(bb.0.lo, 0);
        // North / east off the top-right corner vanish (no wrap at the 256-bit
        // boundary — the shift-left by WIDTH or +1 must not alias low bits).
        assert_eq!(bb.north(), Bitboard::EMPTY);
        assert_eq!(bb.east(), Bitboard::EMPTY);
        // West and south from the corner stay on board.
        assert_eq!(
            bb.west(),
            Bitboard::from_square(Square::<Tenjiku16x16>::new(254))
        );
        assert_eq!(
            bb.south(),
            Bitboard::from_square(Square::<Tenjiku16x16>::new(255 - 16))
        );
    }

    #[test]
    fn tenjiku16x16_north_south_cross_the_limb_seam() {
        // File 0, rank 7 -> index 112 (low limb). North (+16) -> 128, the FIRST
        // bit of the high limb: the shift must carry across the 128-bit seam.
        let s = Square::<Tenjiku16x16>::from_file_rank(0, 7).unwrap();
        assert_eq!(s.index(), 112);
        let n1 = Bitboard::<Tenjiku16x16>::from_square(s).north();
        assert_eq!(n1.0.hi, 1u128); // landed at bit 128 (high-limb bit 0)
        assert_eq!(n1.0.lo, 0);
        assert_eq!(n1, Bitboard::from_square(Square::new(128)));
        // South back across the seam returns to the low limb.
        assert_eq!(n1.south(), Bitboard::from_square(s));
    }

    #[test]
    fn tenjiku16x16_east_west_do_not_leak_at_the_boundary() {
        // Whole last file east / first file west vanish, even at the very top.
        assert_eq!(Bitboard::<Tenjiku16x16>::LAST_FILE.east(), Bitboard::EMPTY);
        assert_eq!(Bitboard::<Tenjiku16x16>::FILE_A.west(), Bitboard::EMPTY);
        // FULL shifted one bit left then masked keeps no off-board bits: at 256
        // squares there are none, so FULL << 1 masked back is still on-board.
        let shifted = Bitboard::<Tenjiku16x16>::FULL.east();
        assert_eq!(shifted & !Bitboard::<Tenjiku16x16>::FULL, Bitboard::EMPTY);
        // East from the last-but-one file square (index 254, file 14) reaches the
        // top square (index 255), not off-board.
        let f14 = Square::<Tenjiku16x16>::new(254);
        assert_eq!((f14.file(), f14.rank()), (14, 15));
        assert_eq!(
            Bitboard::<Tenjiku16x16>::from_square(f14).east(),
            Bitboard::from_square(Square::<Tenjiku16x16>::new(255))
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
