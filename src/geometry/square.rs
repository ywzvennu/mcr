//! A generic board square over an arbitrary [`Geometry`].
//!
//! This is the parallel generic analogue of the concrete [`crate::Square`]; see
//! the [module docs](super) for why the two hierarchies are separate.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;

use super::Geometry;

/// A square of a board with geometry `G`.
///
/// Squares are numbered `0..G::SQUARES` using the little-endian rank-file
/// mapping: the index is `rank * WIDTH + file`. For an 8x8 geometry this is the
/// same numbering as the concrete [`crate::Square`], and `file` / `rank`
/// const-fold to `& 7` / `>> 3`.
#[repr(transparent)]
pub struct Square<G: Geometry>(u8, PhantomData<G>);

/// The error returned when constructing a [`Square`] from an out-of-range index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidSquareIndex(pub u8);

impl fmt::Display for InvalidSquareIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "square index {} is out of range", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for InvalidSquareIndex {}

// Manual derives so `G` need not implement them (it is a zero-sized marker).
impl<G: Geometry> Clone for Square<G> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<G: Geometry> Copy for Square<G> {}

impl<G: Geometry> PartialEq for Square<G> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<G: Geometry> Eq for Square<G> {}

impl<G: Geometry> PartialOrd for Square<G> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<G: Geometry> Ord for Square<G> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<G: Geometry> Hash for Square<G> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<G: Geometry> Square<G> {
    /// Creates a square from its index, panicking if `index >= G::SQUARES`.
    ///
    /// Prefer [`Square::try_new`] when the index is not statically known.
    ///
    /// # Panics
    ///
    /// Panics if `index >= G::SQUARES`.
    #[must_use]
    #[inline]
    pub const fn new(index: u8) -> Square<G> {
        assert!(index < G::SQUARES, "square index out of range");
        Square(index, PhantomData)
    }

    /// Creates a square from its index, returning `None` if `index >=
    /// G::SQUARES`.
    #[must_use]
    #[inline]
    pub const fn try_new(index: u8) -> Option<Square<G>> {
        if index < G::SQUARES {
            Some(Square(index, PhantomData))
        } else {
            None
        }
    }

    /// Builds a square from a zero-based file and rank, returning `None` if
    /// either is out of range.
    #[must_use]
    #[inline]
    pub const fn from_file_rank(file: u8, rank: u8) -> Option<Square<G>> {
        if file < G::WIDTH && rank < G::HEIGHT {
            Some(Square(rank * G::WIDTH + file, PhantomData))
        } else {
            None
        }
    }

    /// Returns the zero-based index of this square (`0..G::SQUARES`).
    #[must_use]
    #[inline]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Returns the zero-based file of this square (`idx % WIDTH`).
    ///
    /// For an 8x8 geometry this const-folds to `idx & 7`.
    #[must_use]
    #[inline]
    pub const fn file(self) -> u8 {
        self.0 % G::WIDTH
    }

    /// Returns the zero-based rank of this square (`idx / WIDTH`).
    ///
    /// For an 8x8 geometry this const-folds to `idx >> 3`.
    #[must_use]
    #[inline]
    pub const fn rank(self) -> u8 {
        self.0 / G::WIDTH
    }

    /// Returns the square `df` files east and `dr` ranks north of this one, or
    /// `None` if the destination falls off the board.
    ///
    /// Negative deltas move west / south.
    #[must_use]
    #[inline]
    pub const fn offset(self, df: i8, dr: i8) -> Option<Square<G>> {
        let file = self.file() as i16 + df as i16;
        let rank = self.rank() as i16 + dr as i16;
        if file < 0 || file >= G::WIDTH as i16 || rank < 0 || rank >= G::HEIGHT as i16 {
            return None;
        }
        Self::from_file_rank(file as u8, rank as u8)
    }
}

impl<G: Geometry> TryFrom<u8> for Square<G> {
    type Error = InvalidSquareIndex;

    #[inline]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Square::try_new(value).ok_or(InvalidSquareIndex(value))
    }
}

impl<G: Geometry> From<Square<G>> for u8 {
    #[inline]
    fn from(square: Square<G>) -> u8 {
        square.index()
    }
}

impl<G: Geometry> From<Square<G>> for usize {
    #[inline]
    fn from(square: Square<G>) -> usize {
        square.index() as usize
    }
}

impl<G: Geometry> fmt::Debug for Square<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Square({})", self.0)
    }
}

impl<G: Geometry> fmt::Display for Square<G> {
    /// Renders the square as `file,rank` zero-based coordinates (the generic
    /// layer has no fixed algebraic alphabet for widths past 26).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.file(), self.rank())
    }
}
