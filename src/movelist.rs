//! A fixed-capacity, stack-backed move buffer that avoids the per-node heap
//! allocation of a `Vec<Move>` on the hot move-generation paths.
//!
//! Move generation runs once per perft node and, in the engine's intended use,
//! once per search node. Collecting each node's pseudo-legal or legal moves into
//! a fresh `Vec<Move>` means a heap allocation (and free) at every node, which
//! dominates the cost of the cheap, branch-light generators on the small-node
//! variants. [`MoveList`] replaces that with an inline `[Move; N]` buffer plus a
//! length cursor: pushes land in the array with no allocation until the rare
//! case that a position produces more than `N` moves, at which point the buffer
//! *spills* the overflow to a heap `Vec`. Because a [`Move`] is a packed `u16`
//! (two bytes), the inline `[Move; 256]` buffer is 512 bytes — half the size it
//! would be with a four-byte move — so the whole list lives in fewer cache lines
//! and copies faster. The spill keeps the type total and
//! safe for any position (notably crazyhouse, whose pocket drops can in
//! adversarial placements exceed any fixed bound) while the common path stays
//! allocation-free.
//!
//! # Capacity bound
//!
//! The inline capacity is [`MoveList::INLINE`] = 256. Standard chess has a
//! proven maximum of 218 legal moves in any position, so every standard and
//! standard-king-safety variant position fits inline with margin. Crazyhouse is
//! the one variant whose move count is not bounded by 218: each of up to five
//! pocketed roles can be dropped onto (almost) every empty square, so a position
//! with a large pocket and many empty squares can in principle produce more
//! moves than any fixed array would hold. Rather than pick a larger-but-still-
//! finite bound that a constructed FEN could still exceed, [`MoveList`] spills
//! such positions to the heap: correctness never depends on the capacity, only
//! performance does, and real crazyhouse play stays far under 256.
//!
//! # No `unsafe`
//!
//! The inline array is value-initialized with a sentinel [`Move`]
//! ([`NULL_MOVE`]); only the first `len` entries are ever read, and they are
//! always overwritten by a real push before being read. There is no
//! `MaybeUninit`, no `unsafe`, and the type relies only on `Move: Copy`.

use crate::{Move, MoveKind, Square};

/// A throwaway sentinel move used to value-initialize the unused tail of the
/// inline array. It is never read: only the first `len` slots are exposed, and
/// each is overwritten by a real [`MoveList::push`] before any read.
const NULL_MOVE: Move = Move::new(Square::A1, Square::A1, MoveKind::Quiet);

/// A fixed-capacity, stack-backed list of [`Move`]s with heap spill on overflow.
///
/// Behaves like a `Vec<Move>` for the operations move generation needs — push,
/// length, iteration, indexing, `retain`, `extend`, `clear` — but stores its
/// first [`MoveList::INLINE`] elements inline with no allocation. See the module
/// docs for the capacity rationale.
#[derive(Clone, Debug)]
pub struct MoveList {
    /// Inline storage for the first `INLINE` moves. Slots at index `>= len`
    /// (within the inline region) hold [`NULL_MOVE`] and are never read.
    inline: [Move; Self::INLINE],
    /// The number of moves stored inline (`<= INLINE`).
    inline_len: usize,
    /// Overflow moves beyond the inline capacity. Empty for every position that
    /// fits inline (all standard and standard-king-safety variants, and all
    /// realistic crazyhouse positions).
    spill: Vec<Move>,
}

impl MoveList {
    /// The inline capacity, chosen to cover the 218-move standard-chess maximum
    /// with margin. See the module docs for the crazyhouse spill rationale.
    pub const INLINE: usize = 256;

    /// Creates an empty list.
    #[inline]
    #[must_use]
    pub fn new() -> MoveList {
        MoveList {
            inline: [NULL_MOVE; Self::INLINE],
            inline_len: 0,
            spill: Vec::new(),
        }
    }

    /// The number of moves in the list (inline plus spill).
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.inline_len + self.spill.len()
    }

    /// Whether the list holds no moves.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inline_len == 0
    }

    /// Removes all moves, keeping the allocated spill capacity for reuse.
    #[inline]
    pub fn clear(&mut self) {
        self.inline_len = 0;
        self.spill.clear();
    }

    /// Appends `mv`, landing inline until the inline capacity is exhausted, then
    /// spilling to the heap.
    #[inline]
    pub fn push(&mut self, mv: Move) {
        if self.inline_len < Self::INLINE {
            self.inline[self.inline_len] = mv;
            self.inline_len += 1;
        } else {
            self.spill.push(mv);
        }
    }

    /// Returns the move at `index` (inline then spill), or panics if out of
    /// bounds — mirroring `Vec` indexing.
    #[inline]
    #[must_use]
    fn get(&self, index: usize) -> &Move {
        if index < self.inline_len {
            &self.inline[index]
        } else {
            &self.spill[index - self.inline_len]
        }
    }

    /// Iterates over the moves in push order.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Move> {
        self.inline[..self.inline_len]
            .iter()
            .chain(self.spill.iter())
    }

    /// Calls `f` on each move in push order. On the overwhelmingly common path
    /// where no position has spilled past the inline capacity, this is a tight
    /// loop over a single contiguous slice with no chained-iterator branch — the
    /// shape the perft inner loop wants. The rare spilled case appends the heap
    /// overflow afterwards.
    #[inline]
    pub fn for_each(&self, mut f: impl FnMut(Move)) {
        for &mv in &self.inline[..self.inline_len] {
            f(mv);
        }
        for &mv in &self.spill {
            f(mv);
        }
    }

    /// Keeps only the moves for which `f` returns `true`, preserving order — the
    /// in-place filter used by the variant forced-move and king-safety passes.
    pub fn retain(&mut self, mut f: impl FnMut(&Move) -> bool) {
        // Compact the inline region in place, then fold any spill back in.
        let mut write = 0;
        for read in 0..self.inline_len {
            if f(&self.inline[read]) {
                self.inline[write] = self.inline[read];
                write += 1;
            }
        }
        self.inline_len = write;
        if self.spill.is_empty() {
            return;
        }
        // Rebuild from the retained spill: push it back through `push` so any
        // freed inline slots are refilled before spilling again.
        let spill = core::mem::take(&mut self.spill);
        for mv in spill {
            if f(&mv) {
                self.push(mv);
            }
        }
    }

    /// Collects the moves into a freshly allocated `Vec<Move>`, the boundary
    /// conversion used by the public `legal_moves` APIs.
    #[must_use]
    pub fn into_vec(self) -> Vec<Move> {
        let mut v = Vec::with_capacity(self.len());
        v.extend_from_slice(&self.inline[..self.inline_len]);
        v.extend(self.spill);
        v
    }
}

impl Default for MoveList {
    #[inline]
    fn default() -> MoveList {
        MoveList::new()
    }
}

impl core::ops::Index<usize> for MoveList {
    type Output = Move;

    #[inline]
    fn index(&self, index: usize) -> &Move {
        self.get(index)
    }
}

impl Extend<Move> for MoveList {
    #[inline]
    fn extend<I: IntoIterator<Item = Move>>(&mut self, iter: I) {
        for mv in iter {
            self.push(mv);
        }
    }
}

impl<'a> Extend<&'a Move> for MoveList {
    #[inline]
    fn extend<I: IntoIterator<Item = &'a Move>>(&mut self, iter: I) {
        for &mv in iter {
            self.push(mv);
        }
    }
}

impl FromIterator<Move> for MoveList {
    #[inline]
    fn from_iter<I: IntoIterator<Item = Move>>(iter: I) -> MoveList {
        let mut list = MoveList::new();
        list.extend(iter);
        list
    }
}

/// Consuming iterator over a [`MoveList`], yielding owned [`Move`]s in push
/// order.
#[derive(Debug)]
pub struct IntoIter {
    inline: [Move; MoveList::INLINE],
    inline_len: usize,
    pos: usize,
    spill: std::vec::IntoIter<Move>,
}

impl Iterator for IntoIter {
    type Item = Move;

    #[inline]
    fn next(&mut self) -> Option<Move> {
        if self.pos < self.inline_len {
            let mv = self.inline[self.pos];
            self.pos += 1;
            Some(mv)
        } else {
            self.spill.next()
        }
    }
}

impl IntoIterator for MoveList {
    type Item = Move;
    type IntoIter = IntoIter;

    #[inline]
    fn into_iter(self) -> IntoIter {
        IntoIter {
            inline: self.inline,
            inline_len: self.inline_len,
            pos: 0,
            spill: self.spill.into_iter(),
        }
    }
}

impl<'a> IntoIterator for &'a MoveList {
    type Item = &'a Move;
    type IntoIter = core::iter::Chain<core::slice::Iter<'a, Move>, core::slice::Iter<'a, Move>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inline[..self.inline_len]
            .iter()
            .chain(self.spill.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(file_to: u8) -> Move {
        // Distinct moves keyed by destination file index for ordering checks.
        let to = Square::new(file_to % 64);
        Move::new(Square::A1, to, MoveKind::Quiet)
    }

    #[test]
    fn push_len_iter_order() {
        let mut list = MoveList::new();
        assert!(list.is_empty());
        for i in 0..10u8 {
            list.push(mv(i));
        }
        assert_eq!(list.len(), 10);
        assert!(!list.is_empty());
        let collected: Vec<_> = list.iter().copied().collect();
        assert_eq!(collected.len(), 10);
        for (i, m) in collected.iter().enumerate() {
            assert_eq!(*m, mv(i as u8));
        }
    }

    #[test]
    fn indexing() {
        let mut list = MoveList::new();
        for i in 0..5u8 {
            list.push(mv(i));
        }
        for i in 0..5 {
            assert_eq!(list[i], mv(i as u8));
        }
    }

    #[test]
    fn clear_resets() {
        let mut list = MoveList::new();
        list.push(mv(1));
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn retain_inline() {
        let mut list = MoveList::new();
        for i in 0..10u8 {
            list.push(mv(i));
        }
        list.retain(|m| m.to().index() % 2 == 0);
        let kept: Vec<_> = list.iter().map(|m| m.to().index() % 2).collect();
        assert!(kept.iter().all(|r| *r == 0));
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn spill_beyond_inline_capacity() {
        let mut list = MoveList::new();
        let total = MoveList::INLINE + 50;
        for i in 0..total {
            list.push(mv(i as u8));
        }
        assert_eq!(list.len(), total);
        // Reading across the inline/spill boundary stays in order.
        for i in 0..total {
            assert_eq!(list[i], mv(i as u8));
        }
        let v = list.clone().into_vec();
        assert_eq!(v.len(), total);
        let consumed: Vec<_> = list.into_iter().collect();
        assert_eq!(consumed.len(), total);
        assert_eq!(consumed, v);
    }

    #[test]
    fn retain_with_spill_refills_inline() {
        let mut list = MoveList::new();
        let total = MoveList::INLINE + 20;
        for i in 0..total {
            list.push(mv(i as u8));
        }
        // Drop everything: both inline and spill must end empty.
        list.retain(|_| false);
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());

        let mut list = MoveList::new();
        for i in 0..total {
            list.push(mv(i as u8));
        }
        // Keep all: order and count preserved across the boundary.
        list.retain(|_| true);
        assert_eq!(list.len(), total);
    }

    #[test]
    fn from_iter_and_extend() {
        let src: Vec<Move> = (0..7u8).map(mv).collect();
        let list: MoveList = src.iter().copied().collect();
        assert_eq!(list.len(), 7);
        let mut list2 = MoveList::new();
        list2.extend(src.iter());
        assert_eq!(list2.len(), 7);
    }
}
