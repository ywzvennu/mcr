//! Optional magic-bitboard slider attacks (`magic` cargo feature).
//!
//! This module is compiled only when the non-default `magic` feature is on. It
//! provides drop-in replacements for the hyperbola-quintessence
//! [`bishop_attacks`](crate::attacks::bishop_attacks) and
//! [`rook_attacks`](crate::attacks::rook_attacks): the public signatures and
//! results are byte-for-byte identical, only the lookup mechanism differs.
//!
//! ## How magic bitboards work
//!
//! For a slider on square `s`, the attack set depends only on the *relevant*
//! occupancy — the blockers on the rays from `s`, excluding the board edges
//! (an edge square never blocks anything beyond it). That relevant-occupancy
//! mask has at most 12 bits for a rook and 9 for a bishop, so there are at most
//! `2^12` / `2^9` distinct inputs per square.
//!
//! A *magic* multiplier `M` maps the masked occupancy `o` to a dense index via
//! `(o * M) >> (64 - bits)`: a hash that, across the occupancies that matter for
//! that square, never sends two *different* attack sets to the same slot. We
//! precompute the attack set for every index once and then answer a query with
//! one multiply, one shift, one add, and one bounds-checked array read.
//!
//! ## Scheme and footprint (fixed-shift "black magic", overlap-packed)
//!
//! Earlier this module searched its own per-square magics and reserved exactly
//! `2^bits[s]` private slots per square, summing to **107 648** slots (~841 KiB)
//! — larger than shakmaty's overlapping array, because independently-found dense
//! magics leave no matching slots to overlap on.
//!
//! Instead we now use a *fixed-shift* scheme with a known, public-domain magic
//! set (the fixed-shift "white magics" found by Volker Annuss and published on
//! talkchess; the same constants shakmaty uses). Every rook square shifts by
//! `64 - 12` and every bishop square by `64 - 9`, so a square's index range is
//! often shorter than `2^bits` and, crucially, the per-square base `offset`s are
//! chosen so the squares' index ranges **overlap** inside one shared flat array:
//! wherever two squares would store the same attack set at the same physical
//! slot, that slot is shared. The combined rook+bishop table is exactly
//!
//! ```text
//! 88 772 entries * 8 bytes = 710 176 bytes ~= 693.5 KiB
//! ```
//!
//! matching shakmaty's array and well under the old 841 KiB. The lookup shape is
//! unchanged from the previous scheme — one multiply, one shift, one `+ offset`,
//! one bounds-checked read — so query cost does not move; only the buffer shrinks.
//!
//! The descriptors are two `[Magic; 64]` arrays (`mask`, `factor`, `offset` =
//! 24 bytes each → `64 * 24 * 2 = 3072` bytes).
//!
//! ## On eliding the read's bounds check
//!
//! The single read above is bounds-checked. That check *can* be removed in safe
//! Rust by reading through a fixed-length per-square window — form
//! `&attacks[offset..offset + size]` (`size = 1 << (64 - shift)`) and index it
//! with the in-window hash, which the fixed shift makes provably `< size`; the
//! compiler then drops the per-read check (verified in the emitted assembly: no
//! `panic_bounds_check` on the hot load). It was tried and measured. It is *not*
//! used here because it did not pay off: across paired perft runs throughput was
//! flat-to-slightly-worse (≈1% slower), since the original check sits on a
//! never-taken, perfectly predicted branch (effectively free) while the window
//! form adds a slice-creation bound on `offset` and — because the largest rook
//! `offset + size` runs one window past the packed table — needs a small dead
//! tail on the backing buffer that grows its cache footprint. The slider read is
//! not the movegen bottleneck, so trading one free check for another is a wash.
//! The lookup is therefore kept as the plain `attacks[index]` read.
//!
//! ## Constants and clean-room status
//!
//! The `mask` / `factor` / `offset` triples are public-domain magic constants.
//! The masks are the standard relevant-occupancy rays; a debug assertion in the
//! builder re-derives each mask from first principles and checks it. Everything
//! that *uses* the constants — the carry-rippler table fill, the index formula,
//! and the lookup — is our own code. All indexing here is bounds-checked safe
//! Rust; the crate denies `unsafe`.

use std::sync::LazyLock;

use crate::{Bitboard, Square};

/// Right-shift applied to the rook magic product: `64 - 12` relevant bits.
const ROOK_SHIFT: u32 = 64 - 12;
/// Right-shift applied to the bishop magic product: `64 - 9` relevant bits.
const BISHOP_SHIFT: u32 = 64 - 9;
/// Total length of the shared, overlap-packed attack table.
const ATTACKS_LEN: usize = 88772;

/// Per-square magic descriptor.
///
/// The attack set for this square under `occupied` lives at
/// `attacks[((occupied & mask) * factor >> shift) + offset]`, where `shift` is
/// the fixed [`ROOK_SHIFT`] / [`BISHOP_SHIFT`] for the piece kind.
#[derive(Clone, Copy)]
struct Magic {
    /// Relevant-occupancy mask (rays from the square, edges excluded).
    mask: u64,
    /// Fixed-shift magic multiplier (public-domain constant).
    factor: u64,
    /// Base offset of this square's index range within the shared attack table.
    offset: usize,
}

impl Magic {
    /// Empty placeholder used to size the descriptor arrays before the fill.
    const EMPTY: Magic = Magic {
        mask: 0,
        factor: 0,
        offset: 0,
    };

    /// Index into the shared attack table for this square under `occupied`.
    #[inline]
    fn index(&self, occupied: u64, shift: u32) -> usize {
        let relevant = occupied & self.mask;
        (relevant.wrapping_mul(self.factor) >> shift) as usize + self.offset
    }
}

/// The complete magic state: descriptors for both piece kinds plus the single
/// shared, overlap-packed attack table they index into.
struct Tables {
    rook: [Magic; 64],
    bishop: [Magic; 64],
    attacks: Vec<u64>,
}

/// The four rook ray directions as `(file_delta, rank_delta)`.
const ROOK_DIRS: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
/// The four bishop ray directions.
const BISHOP_DIRS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Walks the rays in `dirs` from `sq`, stopping at the first blocker in
/// `occupied` on each ray (inclusive). This is the ground-truth attack set the
/// magic table must reproduce; it mirrors the brute-force scan used in the
/// hyperbola tests.
fn ray_attacks(sq: u32, occupied: u64, dirs: &[(i32, i32); 4]) -> u64 {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    let mut bits = 0u64;
    for &(df, dr) in dirs {
        let (mut f, mut r) = (file + df, rank + dr);
        while (0..8).contains(&f) && (0..8).contains(&r) {
            let bit = 1u64 << (r * 8 + f);
            bits |= bit;
            if occupied & bit != 0 {
                break;
            }
            f += df;
            r += dr;
        }
    }
    bits
}

/// The relevant-occupancy mask for `sq`: every ray square *except* the far edge,
/// because a blocker on the edge cannot stop anything beyond it. Used only by a
/// debug assertion that re-derives the constants' masks from first principles.
#[cfg(debug_assertions)]
fn relevant_mask(sq: u32, dirs: &[(i32, i32); 4]) -> u64 {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    let mut bits = 0u64;
    for &(df, dr) in dirs {
        let (mut f, mut r) = (file + df, rank + dr);
        while (1..7).contains(&f) || (1..7).contains(&r) {
            let on_board = (0..8).contains(&f) && (0..8).contains(&r);
            if !on_board {
                break;
            }
            let (nf, nr) = (f + df, r + dr);
            if !((0..8).contains(&nf) && (0..8).contains(&nr)) {
                break;
            }
            bits |= 1u64 << (r * 8 + f);
            f = nf;
            r = nr;
        }
    }
    bits
}

/// One public-domain magic entry as `(mask, factor, offset)`.
type MagicConst = (u64, u64, usize);

/// Fixed-shift rook magics (mask, factor, offset). Public-domain "white magics"
/// found by Volker Annuss; the offsets overlap-pack into [`ATTACKS_LEN`] slots.
#[rustfmt::skip]
const ROOK_MAGICS: [MagicConst; 64] = [
    (0x0001_0101_0101_017e, 0x0028_0077_ffeb_fffe, 26304),
    (0x0002_0202_0202_027c, 0x2004_0102_0109_7fff, 35520),
    (0x0004_0404_0404_047a, 0x0010_0200_1005_3fff, 38592),
    (0x0008_0808_0808_0876, 0x0040_0400_0800_4002, 8026),
    (0x0010_1010_1010_106e, 0x7fd0_0441_ffff_d003, 22196),
    (0x0020_2020_2020_205e, 0x4020_0088_87df_fffe, 80870),
    (0x0040_4040_4040_403e, 0x0040_0088_8847_ffff, 76747),
    (0x0080_8080_8080_807e, 0x0068_00fb_ff75_fffd, 30400),
    (0x0001_0101_0101_7e00, 0x0000_2801_0113_ffff, 11115),
    (0x0002_0202_0202_7c00, 0x0020_0402_01fc_ffff, 18205),
    (0x0004_0404_0404_7a00, 0x007f_e800_42ff_ffe8, 53577),
    (0x0008_0808_0808_7600, 0x0000_1800_217f_ffe8, 62724),
    (0x0010_1010_1010_6e00, 0x0000_1800_073f_ffe8, 34282),
    (0x0020_2020_2020_5e00, 0x0000_1800_e05f_ffe8, 29196),
    (0x0040_4040_4040_3e00, 0x0000_1800_602f_ffe8, 23806),
    (0x0080_8080_8080_7e00, 0x0000_3000_2fff_ffa0, 49481),
    (0x0001_0101_017e_0100, 0x0030_0018_010b_ffff, 2410),
    (0x0002_0202_027c_0200, 0x0003_000c_0085_fffb, 36498),
    (0x0004_0404_047a_0400, 0x0004_0008_0201_0008, 24478),
    (0x0008_0808_0876_0800, 0x0004_0020_2002_0004, 10074),
    (0x0010_1010_106e_1000, 0x0001_0020_0200_2001, 79315),
    (0x0020_2020_205e_2000, 0x0001_0010_0080_1040, 51779),
    (0x0040_4040_403e_4000, 0x0000_0040_4000_8001, 13586),
    (0x0080_8080_807e_8000, 0x0000_0068_00cd_fff4, 19323),
    (0x0001_0101_7e01_0100, 0x0040_2000_1008_0010, 70612),
    (0x0002_0202_7c02_0200, 0x0000_0800_1004_0010, 83652),
    (0x0004_0404_7a04_0400, 0x0004_0100_0802_0008, 63110),
    (0x0008_0808_7608_0800, 0x0000_0400_2020_0200, 34496),
    (0x0010_1010_6e10_1000, 0x0002_0080_1010_0100, 84966),
    (0x0020_2020_5e20_2000, 0x0000_0080_2001_0020, 54341),
    (0x0040_4040_3e40_4000, 0x0000_0080_2020_0040, 60421),
    (0x0080_8080_7e80_8000, 0x0000_8200_2000_4020, 86402),
    (0x0001_017e_0101_0100, 0x00ff_fd18_0030_0030, 50245),
    (0x0002_027c_0202_0200, 0x007f_ff7f_bfd4_0020, 76622),
    (0x0004_047a_0404_0400, 0x003f_ffbd_0018_0018, 84676),
    (0x0008_0876_0808_0800, 0x001f_ffde_8018_0018, 78757),
    (0x0010_106e_1010_1000, 0x000f_ffe0_bfe8_0018, 37346),
    (0x0020_205e_2020_2000, 0x0001_0000_8020_2001, 370),
    (0x0040_403e_4040_4000, 0x0003_fffb_ff98_0180, 42182),
    (0x0080_807e_8080_8000, 0x0001_fffd_ff90_00e0, 45385),
    (0x0001_7e01_0101_0100, 0x00ff_fefe_ebff_d800, 61659),
    (0x0002_7c02_0202_0200, 0x007f_fff7_ffc0_1400, 12790),
    (0x0004_7a04_0404_0400, 0x003f_ffbf_e4ff_e800, 16762),
    (0x0008_7608_0808_0800, 0x001f_fff0_1fc0_3000, 0),
    (0x0010_6e10_1010_1000, 0x000f_ffe7_f8bf_e800, 38380),
    (0x0020_5e20_2020_2000, 0x0007_ffdf_df3f_f808, 11098),
    (0x0040_3e40_4040_4000, 0x0003_fff8_5fff_a804, 21803),
    (0x0080_7e80_8080_8000, 0x0001_fffd_75ff_a802, 39189),
    (0x007e_0101_0101_0100, 0x00ff_ffd7_ffeb_ffd8, 58628),
    (0x007c_0202_0202_0200, 0x007f_ff75_ff7f_bfd8, 44116),
    (0x007a_0404_0404_0400, 0x003f_ff86_3fbf_7fd8, 78357),
    (0x0076_0808_0808_0800, 0x001f_ffbf_dfd7_ffd8, 44481),
    (0x006e_1010_1010_1000, 0x000f_fff8_1028_0028, 64134),
    (0x005e_2020_2020_2000, 0x0007_ffd7_f7fe_ffd8, 41759),
    (0x003e_4040_4040_4000, 0x0003_fffc_0c48_0048, 1394),
    (0x007e_8080_8080_8000, 0x0001_ffff_afd7_ffd8, 40910),
    (0x7e01_0101_0101_0100, 0x00ff_ffe4_ffdf_a3ba, 66516),
    (0x7c02_0202_0202_0200, 0x007f_ffef_7ff3_d3da, 3897),
    (0x7a04_0404_0404_0400, 0x003f_ffbf_dfef_f7fa, 3930),
    (0x7608_0808_0808_0800, 0x001f_ffef_f7fb_fc22, 72934),
    (0x6e10_1010_1010_1000, 0x0000_0204_0800_1001, 72662),
    (0x5e20_2020_2020_2000, 0x0007_fffe_ffff_77fd, 56325),
    (0x3e40_4040_4040_4000, 0x0003_ffff_bf7d_feec, 66501),
    (0x7e80_8080_8080_8000, 0x0001_ffff_9dff_a333, 14826),
];

/// Fixed-shift bishop magics (mask, factor, offset). Public-domain "white
/// magics"; their index ranges nest into the same shared attack array.
#[rustfmt::skip]
const BISHOP_MAGICS: [MagicConst; 64] = [
    (0x0040_2010_0804_0200, 0x007f_bfbf_bfbf_bfff, 5378),
    (0x0000_4020_1008_0400, 0x0000_a060_4010_07fc, 4093),
    (0x0000_0040_2010_0a00, 0x0001_0040_0802_0000, 4314),
    (0x0000_0000_4022_1400, 0x0000_8060_0400_0000, 6587),
    (0x0000_0000_0244_2800, 0x0000_1004_0000_0000, 6491),
    (0x0000_0002_0408_5000, 0x0000_21c1_00b2_0000, 6330),
    (0x0000_0204_0810_2000, 0x0000_0400_4100_8000, 5609),
    (0x0002_0408_1020_4000, 0x0000_0fb0_203f_ff80, 22236),
    (0x0020_1008_0402_0000, 0x0000_0401_0040_1004, 6106),
    (0x0040_2010_0804_0000, 0x0000_0200_8020_0802, 5625),
    (0x0000_4020_100a_0000, 0x0000_0040_1020_2000, 16785),
    (0x0000_0040_2214_0000, 0x0000_0080_6004_0000, 16817),
    (0x0000_0002_4428_0000, 0x0000_0044_0200_0000, 6842),
    (0x0000_0204_0850_0000, 0x0000_0008_0100_8000, 7003),
    (0x0002_0408_1020_0000, 0x0000_07ef_e0bf_ff80, 4197),
    (0x0004_0810_2040_0000, 0x0000_0008_2082_0020, 7356),
    (0x0010_0804_0200_0200, 0x0000_4000_8080_8080, 4602),
    (0x0020_1008_0400_0400, 0x0002_1f01_0040_0808, 4538),
    (0x0040_2010_0a00_0a00, 0x0001_8000_c06f_3fff, 29531),
    (0x0000_4022_1400_1400, 0x0000_2582_0080_1000, 45393),
    (0x0000_0244_2800_2800, 0x0000_2400_8084_0000, 12420),
    (0x0002_0408_5000_5000, 0x0000_1800_0c03_fff8, 15763),
    (0x0004_0810_2000_2000, 0x0000_0a58_4020_8020, 5050),
    (0x0008_1020_4000_4000, 0x0000_0200_0820_8020, 4346),
    (0x0008_0402_0002_0400, 0x0000_8040_0081_0100, 6074),
    (0x0010_0804_0004_0800, 0x0001_0119_0080_2008, 7866),
    (0x0020_100a_000a_1000, 0x0000_8040_0081_0100, 32139),
    (0x0040_2214_0014_2200, 0x0001_0040_3c04_03ff, 57673),
    (0x0002_4428_0028_4400, 0x0007_8402_a880_2000, 55365),
    (0x0004_0850_0050_0800, 0x0000_1010_0080_4400, 15818),
    (0x0008_1020_0020_1000, 0x0000_0808_0010_4100, 5562),
    (0x0010_2040_0040_2000, 0x0000_4004_c008_2008, 6390),
    (0x0004_0200_0204_0800, 0x0001_0101_2000_8020, 7930),
    (0x0008_0400_0408_1000, 0x0000_8080_9a00_4010, 13329),
    (0x0010_0a00_0a10_2000, 0x0007_fefe_0881_0010, 7170),
    (0x0022_1400_1422_4000, 0x0003_ff0f_833f_c080, 27267),
    (0x0044_2800_2844_0200, 0x007f_e080_1900_3042, 53787),
    (0x0008_5000_5008_0400, 0x003f_ffef_ea00_3000, 5097),
    (0x0010_2000_2010_0800, 0x0000_1010_1000_2080, 6643),
    (0x0020_4000_4020_1000, 0x0000_8020_0508_0804, 6138),
    (0x0002_0002_0408_1000, 0x0000_8080_80a8_0040, 7418),
    (0x0004_0004_0810_2000, 0x0000_1041_0020_0040, 7898),
    (0x000a_000a_1020_4000, 0x0003_ffdf_7f83_3fc0, 42012),
    (0x0014_0014_2240_0000, 0x0000_0088_4045_0020, 57350),
    (0x0028_0028_4402_0000, 0x0000_7ffc_8018_0030, 22813),
    (0x0050_0050_0804_0200, 0x007f_ffdd_8014_0028, 56693),
    (0x0020_0020_1008_0400, 0x0002_0080_200a_0004, 5818),
    (0x0040_0040_2010_0800, 0x0000_1010_1010_0020, 7098),
    (0x0000_0204_0810_2000, 0x0007_ffdf_c180_5000, 4451),
    (0x0000_0408_1020_4000, 0x0003_ffef_e0c0_2200, 4709),
    (0x0000_0a10_2040_0000, 0x0000_0008_2080_6000, 4794),
    (0x0000_1422_4000_0000, 0x0000_0000_0840_3000, 13364),
    (0x0000_2844_0200_0000, 0x0000_0001_0020_2000, 4570),
    (0x0000_5008_0402_0000, 0x0000_0040_4080_2000, 4282),
    (0x0000_2010_0804_0200, 0x0004_0100_4010_0400, 14964),
    (0x0000_4020_1008_0400, 0x0000_6020_6018_03f4, 4026),
    (0x0002_0408_1020_4000, 0x0003_ffdf_dfc2_8048, 4826),
    (0x0004_0810_2040_0000, 0x0000_0008_2082_0020, 7354),
    (0x000a_1020_4000_0000, 0x0000_0000_0820_8060, 4848),
    (0x0014_2240_0000_0000, 0x0000_0000_0080_8020, 15946),
    (0x0028_4402_0000_0000, 0x0000_0000_0100_2020, 14932),
    (0x0050_0804_0200_0000, 0x0000_0004_0100_2008, 16588),
    (0x0020_1008_0402_0000, 0x0000_0040_4040_4040, 6905),
    (0x0040_2010_0804_0200, 0x007f_ff9f_df7f_f813, 16076),
];

/// Fills the shared attack table for one piece kind from its magic constants.
///
/// For each square it enumerates every relevant occupancy with the carry-rippler
/// trick — `subset = (subset.wrapping_sub(mask)) & mask` walks all subsets of the
/// mask's set bits — computes the true attack set, and writes it at the magic's
/// index. Because the offsets overlap, a slot may be written by more than one
/// square; the debug assertion guarantees they always agree, so the shared array
/// is well-defined. Returns the per-square descriptors.
fn fill(
    consts: &[MagicConst; 64],
    dirs: &[(i32, i32); 4],
    shift: u32,
    attacks: &mut [u64],
) -> [Magic; 64] {
    let mut magics = [Magic::EMPTY; 64];
    for (sq, &(mask, factor, offset)) in consts.iter().enumerate() {
        // The published mask must be the relevant-occupancy mask we derive
        // independently; this catches any transcription error in debug builds.
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            mask,
            relevant_mask(sq as u32, dirs),
            "magic mask mismatch at square {sq}",
        );

        let magic = Magic {
            mask,
            factor,
            offset,
        };

        // Carry-rippler enumeration of every subset of the mask's bits.
        let mut subset = 0u64;
        loop {
            let attack = ray_attacks(sq as u32, subset, dirs);
            let idx = magic.index(subset, shift);
            // Overlap is allowed only where the stored value already agrees.
            debug_assert!(
                attacks[idx] == 0 || attacks[idx] == attack,
                "overlap conflict at slot {idx} for square {sq}",
            );
            attacks[idx] = attack;
            subset = subset.wrapping_sub(mask) & mask;
            if subset == 0 {
                break;
            }
        }

        magics[sq] = magic;
    }
    magics
}

/// Builds the full magic state once: fills the single overlap-packed attack
/// table from the public-domain magic constants for both piece kinds.
fn build() -> Tables {
    let mut attacks = vec![0u64; ATTACKS_LEN];
    let bishop = fill(&BISHOP_MAGICS, &BISHOP_DIRS, BISHOP_SHIFT, &mut attacks);
    let rook = fill(&ROOK_MAGICS, &ROOK_DIRS, ROOK_SHIFT, &mut attacks);
    Tables {
        rook,
        bishop,
        attacks,
    }
}

/// The one-time magic state, filled on first slider query.
static TABLES: LazyLock<Tables> = LazyLock::new(build);

/// Magic-bitboard rook attacks. Identical results to the hyperbola version.
#[inline]
pub(crate) fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let m = &TABLES.rook[sq.index() as usize];
    Bitboard(TABLES.attacks[m.index(occupied.0, ROOK_SHIFT)])
}

/// Magic-bitboard bishop attacks. Identical results to the hyperbola version.
#[inline]
pub(crate) fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let m = &TABLES.bishop[sq.index() as usize];
    Bitboard(TABLES.attacks[m.index(occupied.0, BISHOP_SHIFT)])
}

/// The number of `u64` slots in the shared attack table, exposed for the
/// footprint report and tests.
#[must_use]
pub fn attack_table_len() -> usize {
    TABLES.attacks.len()
}
