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
//! `(o * M) >> (64 - bits)`: a perfect hash with no collisions among the
//! occupancies that actually matter for that square. We precompute the attack
//! set for every index once and then answer a query with one multiply, one
//! shift, and one bounds-checked array read.
//!
//! ## Scheme and footprint (fancy magics, per-square exact sizing)
//!
//! We use *fancy* magics: each square reserves exactly `2^bits[s]` slots in one
//! shared flat attack array, with a per-square base offset. Rook squares need
//! `4096 + ...` (variable by square; corners need fewer relevant bits than the
//! center), summing to **102 400** rook slots; bishop squares sum to **5 248**
//! slots. The combined table is
//!
//! ```text
//! (102 400 + 5 248) entries * 8 bytes = 107 648 * 8 = 861 184 bytes ~= 841 KiB
//! ```
//!
//! plus two `[Magic; 64]` descriptor arrays (`mask`, `factor`, `offset`,
//! `shift` = 32 bytes each → `64 * 32 * 2 = 4096` bytes). This is the classic
//! fancy-magic layout. It is larger than shakmaty's overlapping black-magic
//! array (~693 KiB) because we do not pack squares' sub-tables on top of one
//! another, but it is a fully self-contained, clean-room build with no copied
//! constants.
//!
//! ## Generation
//!
//! Magic multipliers are *found* at first use, not hard-coded: a one-time
//! [`LazyLock`] init runs a trial-and-error search seeded by an in-crate
//! [`splitmix64`] PRNG. For each square it draws sparse candidate multipliers
//! (the AND of three random words biases toward few set bits, which magics
//! need) and accepts the first one that hashes every relevant occupancy to a
//! slot whose stored attack set is either empty or already equals the correct
//! one. The search is deterministic given the fixed seed, so the table is
//! reproducible across runs and platforms.
//!
//! All indexing here is bounds-checked safe Rust; the crate denies `unsafe`.

use std::sync::LazyLock;

use crate::{Bitboard, Square};

/// Per-square magic descriptor.
///
/// `attacks[(occupied & mask) wrapping_mul factor >> shift) + offset]` is the
/// attack set for this square under `occupied`.
#[derive(Clone, Copy)]
struct Magic {
    /// Relevant-occupancy mask (rays from the square, edges excluded).
    mask: u64,
    /// Perfect-hash multiplier found by the search.
    factor: u64,
    /// Right-shift `64 - relevant_bits` that compresses the product to an index.
    shift: u32,
    /// Base offset of this square's block within the shared attack table.
    offset: usize,
}

impl Magic {
    /// Empty placeholder used to size the descriptor arrays before the search.
    const EMPTY: Magic = Magic {
        mask: 0,
        factor: 0,
        shift: 0,
        offset: 0,
    };

    /// Index into the shared attack table for this square under `occupied`.
    #[inline]
    fn index(&self, occupied: u64) -> usize {
        let relevant = occupied & self.mask;
        (relevant.wrapping_mul(self.factor) >> self.shift) as usize + self.offset
    }
}

/// The complete magic state: descriptors for both piece kinds plus the single
/// shared attack table they index into.
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
/// because a blocker on the edge cannot stop anything beyond it. This is the set
/// of bits that actually select a distinct attack set.
fn relevant_mask(sq: u32, dirs: &[(i32, i32); 4]) -> u64 {
    let file = (sq % 8) as i32;
    let rank = (sq / 8) as i32;
    let mut bits = 0u64;
    for &(df, dr) in dirs {
        let (mut f, mut r) = (file + df, rank + dr);
        // Stop one short of the board edge in this direction.
        while (1..7).contains(&f) || (1..7).contains(&r) {
            // The next step must also be on-board; otherwise this square is the
            // last interior square and we must not include the edge beyond it.
            let on_board = (0..8).contains(&f) && (0..8).contains(&r);
            if !on_board {
                break;
            }
            // Exclude the edge square in the perpendicular sense: keep the
            // square only if a further step stays on the board.
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

/// Enumerates the `index`-th subset of the set bits of `mask`.
///
/// Treating the `n` set bits of `mask` as bit positions `0..n`, this maps the
/// integer `index` (`0..2^n`) to the occupancy that has exactly those selected
/// bits set. Iterating `index` over `0..2^n` therefore visits every possible
/// blocker configuration on the relevant rays exactly once.
fn occupancy_subset(index: usize, mask: u64) -> u64 {
    let mut result = 0u64;
    let mut bits = mask;
    let mut i = 0;
    while bits != 0 {
        let lsb = bits & bits.wrapping_neg();
        if index & (1 << i) != 0 {
            result |= lsb;
        }
        bits ^= lsb;
        i += 1;
    }
    result
}

/// A tiny `splitmix64` generator — the standard finalizer-based PRNG used to
/// draw candidate magics. It is in-crate and deterministic so the table builds
/// reproducibly.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A sparse candidate magic: ANDing three draws keeps few bits set, which is
    /// what makes a multiplier likely to be a usable perfect hash.
    fn sparse(&mut self) -> u64 {
        self.next_u64() & self.next_u64() & self.next_u64()
    }
}

/// Searches for a magic for one square and fills its block of the shared attack
/// table.
///
/// `attacks` already has `1 << bits` slots reserved at `offset`. On success the
/// returned [`Magic`] hashes every relevant occupancy of `sq` to a distinct,
/// correctly-filled slot.
fn build_square(
    sq: u32,
    dirs: &[(i32, i32); 4],
    offset: usize,
    rng: &mut SplitMix64,
    attacks: &mut [u64],
) -> Magic {
    let mask = relevant_mask(sq, dirs);
    let bits = mask.count_ones();
    let count = 1usize << bits;
    let shift = 64 - bits;

    // Precompute the (occupancy, attack) pairs for every relevant subset.
    let mut occs = Vec::with_capacity(count);
    let mut atts = Vec::with_capacity(count);
    for i in 0..count {
        let occ = occupancy_subset(i, mask);
        occs.push(occ);
        atts.push(ray_attacks(sq, occ, dirs));
    }

    // Scratch slots filled per attempt; on collision we restart.
    let block = &mut attacks[offset..offset + count];
    // `epoch` lets us treat a slot as "unused this attempt" without re-zeroing
    // the whole block on every failed magic: a slot is live only if its tag
    // matches the current attempt number.
    let mut used = vec![0u32; count];
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        let factor = rng.sparse();
        // Reject obviously weak multipliers: a good magic spreads the top byte
        // of the masked-occupancy product, so require enough high bits set.
        if (mask.wrapping_mul(factor) >> 56).count_ones() < 6 {
            continue;
        }

        let mut ok = true;
        for i in 0..count {
            let idx = (occs[i].wrapping_mul(factor) >> shift) as usize;
            if used[idx] != attempt {
                // First time this slot is hit this attempt: claim and fill it.
                used[idx] = attempt;
                block[idx] = atts[i];
            } else if block[idx] != atts[i] {
                // A real collision: two occupancies want different attacks in
                // the same slot. This factor fails; try another.
                ok = false;
                break;
            }
        }

        if ok {
            return Magic {
                mask,
                factor,
                shift,
                offset,
            };
        }
    }
}

/// Builds the full magic state once: searches a magic for every square of both
/// piece kinds and lays their attack blocks end-to-end in one shared table.
fn build() -> Tables {
    // Reserve slots: each square needs `2^relevant_bits` entries. Compute the
    // offsets first so the search can fill directly into the final array.
    let mut rook = [Magic::EMPTY; 64];
    let mut bishop = [Magic::EMPTY; 64];

    let mut total = 0usize;
    let mut rook_off = [0usize; 64];
    let mut bishop_off = [0usize; 64];
    for sq in 0..64u32 {
        rook_off[sq as usize] = total;
        total += 1usize << relevant_mask(sq, &ROOK_DIRS).count_ones();
    }
    for sq in 0..64u32 {
        bishop_off[sq as usize] = total;
        total += 1usize << relevant_mask(sq, &BISHOP_DIRS).count_ones();
    }

    let mut attacks = vec![0u64; total];

    // A fixed seed keeps the generated table identical on every run/platform.
    let mut rng = SplitMix64::new(0x0000_C54D_5347_4D31);

    for sq in 0..64u32 {
        rook[sq as usize] = build_square(
            sq,
            &ROOK_DIRS,
            rook_off[sq as usize],
            &mut rng,
            &mut attacks,
        );
    }
    for sq in 0..64u32 {
        bishop[sq as usize] = build_square(
            sq,
            &BISHOP_DIRS,
            bishop_off[sq as usize],
            &mut rng,
            &mut attacks,
        );
    }

    Tables {
        rook,
        bishop,
        attacks,
    }
}

/// The one-time magic state, found and filled on first slider query.
static TABLES: LazyLock<Tables> = LazyLock::new(build);

/// Magic-bitboard rook attacks. Identical results to the hyperbola version.
#[inline]
pub(crate) fn rook_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let m = &TABLES.rook[sq.index() as usize];
    Bitboard(TABLES.attacks[m.index(occupied.0)])
}

/// Magic-bitboard bishop attacks. Identical results to the hyperbola version.
#[inline]
pub(crate) fn bishop_attacks(sq: Square, occupied: Bitboard) -> Bitboard {
    let m = &TABLES.bishop[sq.index() as usize];
    Bitboard(TABLES.attacks[m.index(occupied.0)])
}

/// The number of `u64` slots in the shared attack table, exposed for the
/// footprint report and tests.
#[must_use]
pub fn attack_table_len() -> usize {
    TABLES.attacks.len()
}
