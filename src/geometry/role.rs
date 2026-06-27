//! The extended role set for the generic fairy-variant layer.
//!
//! This is the parallel generic analogue of the concrete [`crate::Role`]: where
//! the frozen 8x8 path has exactly the six standard roles, the generic layer
//! needs headroom for the fairy pieces the Milestone 10 variants introduce (see
//! `docs/fairy-variants-architecture.md` §1). [`WideRole`] is purely an
//! *identity + index*: the role's **movement is not defined here** — a variant
//! supplies that later. All this type does is name the role, give it a stable
//! board/pocket array index, and map it to and from a FEN character.
//!
//! The set is deliberately open-ended: it starts with the standard six plus the
//! named fairy pieces the architecture census lists, and a small reserved range
//! at the end. It **grows as variants land** — adding a role is a matter of
//! extending the enum (and bumping [`WideRole::COUNT`]); nothing here bakes in a
//! closed taxonomy the way the concrete six-role path does.

use core::fmt;

/// An extended piece role for the generic board.
///
/// The discriminant doubles as the array index used by [`Board<G>`] for its
/// per-role occupancy masks, so the values are stable and contiguous from `0`.
/// The first six match the concrete [`crate::Role`] ordering (pawn first, king
/// last) so an 8x8 board reads identically; the rest are the fairy pieces named
/// in the variant census.
///
/// Movement is intentionally absent — this enum is identity only.
///
/// ```
/// use mce::geometry::WideRole;
/// assert_eq!(WideRole::Pawn.index(), 0);
/// assert_eq!(WideRole::from_char('a'), Some(WideRole::Hawk));
/// assert_eq!(WideRole::Hawk.char(), 'a');
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum WideRole {
    // --- the standard six (same order as the concrete `Role`) ---
    /// A pawn.
    Pawn = 0,
    /// A knight.
    Knight = 1,
    /// A bishop.
    Bishop = 2,
    /// A rook.
    Rook = 3,
    /// A queen.
    Queen = 4,
    /// A king.
    King = 5,

    // --- fairy pieces from the variant census (§1) ---
    /// Met / Ferz — one diagonal step (Makruk, Sittuyin).
    Met = 6,
    /// Khon / silver-general mover — one diagonal step or one straight-forward
    /// step (Makruk, Shogi).
    Silver = 7,
    /// Gold-general mover — the three forward squares, the two sideways, and one
    /// straight back (Shogi).
    Gold = 8,
    /// Wazir — one orthogonal step.
    Wazir = 9,
    /// Hawk — Bishop + Knight compound (a.k.a. Archbishop / Cardinal / Janus;
    /// Seirawan's hawk, Capablanca's archbishop).
    Hawk = 10,
    /// Elephant — Rook + Knight compound (a.k.a. Chancellor / Marshal; Seirawan's
    /// elephant, Capablanca's chancellor). Distinct from the blockable Xiangqi
    /// elephant, which is a separate role.
    Elephant = 11,
    /// Cannon — moves as a rook over empty squares, captures by jumping a single
    /// screen (Xiangqi, Janggi, Shako).
    Cannon = 12,
    /// Lance — a forward-only rook slider (Shogi).
    Lance = 13,

    // --- a small reserved range for variants not yet implemented ---
    /// Reserved role slot 0; meaning assigned when a future variant lands.
    Reserved0 = 14,
    /// Reserved role slot 1.
    Reserved1 = 15,
    /// Reserved role slot 2.
    Reserved2 = 16,
    /// Reserved role slot 3.
    Reserved3 = 17,
}

impl WideRole {
    /// The number of distinct roles, i.e. the length of [`WideRole::ALL`] and
    /// the size of a [`Board<G>`](super::Board)'s per-role mask array.
    ///
    /// This grows as fairy variants land and add roles.
    pub const COUNT: usize = 18;

    /// Every role, in index order (pawn first, reserved last).
    pub const ALL: [WideRole; Self::COUNT] = [
        WideRole::Pawn,
        WideRole::Knight,
        WideRole::Bishop,
        WideRole::Rook,
        WideRole::Queen,
        WideRole::King,
        WideRole::Met,
        WideRole::Silver,
        WideRole::Gold,
        WideRole::Wazir,
        WideRole::Hawk,
        WideRole::Elephant,
        WideRole::Cannon,
        WideRole::Lance,
        WideRole::Reserved0,
        WideRole::Reserved1,
        WideRole::Reserved2,
        WideRole::Reserved3,
    ];

    /// Returns this role's stable array index (`0..COUNT`), the discriminant.
    #[must_use]
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Builds a role from its array index, returning `None` if out of range.
    #[must_use]
    #[inline]
    pub const fn from_index(index: usize) -> Option<WideRole> {
        if index < Self::COUNT {
            Some(Self::ALL[index])
        } else {
            None
        }
    }

    /// Returns the lowercase FEN/SAN character for this role.
    ///
    /// The standard six reuse the concrete letters (`p n b r q k`). The fairy
    /// roles take distinct letters that do not collide with the standard six;
    /// the reserved roles have no character yet and map to `'?'`.
    #[must_use]
    #[inline]
    pub const fn char(self) -> char {
        match self {
            WideRole::Pawn => 'p',
            WideRole::Knight => 'n',
            WideRole::Bishop => 'b',
            WideRole::Rook => 'r',
            WideRole::Queen => 'q',
            WideRole::King => 'k',
            WideRole::Met => 'm',
            WideRole::Silver => 's',
            WideRole::Gold => 'g',
            WideRole::Wazir => 'w',
            WideRole::Hawk => 'a',
            WideRole::Elephant => 'e',
            WideRole::Cannon => 'c',
            WideRole::Lance => 'l',
            WideRole::Reserved0
            | WideRole::Reserved1
            | WideRole::Reserved2
            | WideRole::Reserved3 => '?',
        }
    }

    /// Returns the uppercase FEN/SAN character for this role.
    #[must_use]
    #[inline]
    pub const fn upper_char(self) -> char {
        self.char().to_ascii_uppercase()
    }

    /// Parses a role from its character, accepting either case.
    ///
    /// Returns `None` for any character that is not a defined role letter (the
    /// reserved roles have none, so `'?'` yields `None`).
    ///
    /// ```
    /// use mce::geometry::WideRole;
    /// assert_eq!(WideRole::from_char('N'), Some(WideRole::Knight));
    /// assert_eq!(WideRole::from_char('c'), Some(WideRole::Cannon));
    /// assert_eq!(WideRole::from_char('?'), None);
    /// ```
    #[must_use]
    #[inline]
    pub const fn from_char(ch: char) -> Option<WideRole> {
        match ch.to_ascii_lowercase() {
            'p' => Some(WideRole::Pawn),
            'n' => Some(WideRole::Knight),
            'b' => Some(WideRole::Bishop),
            'r' => Some(WideRole::Rook),
            'q' => Some(WideRole::Queen),
            'k' => Some(WideRole::King),
            'm' => Some(WideRole::Met),
            's' => Some(WideRole::Silver),
            'g' => Some(WideRole::Gold),
            'w' => Some(WideRole::Wazir),
            'a' => Some(WideRole::Hawk),
            'e' => Some(WideRole::Elephant),
            'c' => Some(WideRole::Cannon),
            'l' => Some(WideRole::Lance),
            _ => None,
        }
    }
}

impl fmt::Display for WideRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WideRole::Pawn => "pawn",
            WideRole::Knight => "knight",
            WideRole::Bishop => "bishop",
            WideRole::Rook => "rook",
            WideRole::Queen => "queen",
            WideRole::King => "king",
            WideRole::Met => "met",
            WideRole::Silver => "silver",
            WideRole::Gold => "gold",
            WideRole::Wazir => "wazir",
            WideRole::Hawk => "hawk",
            WideRole::Elephant => "elephant",
            WideRole::Cannon => "cannon",
            WideRole::Lance => "lance",
            WideRole::Reserved0 => "reserved0",
            WideRole::Reserved1 => "reserved1",
            WideRole::Reserved2 => "reserved2",
            WideRole::Reserved3 => "reserved3",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn index_matches_discriminant_and_position() {
        for (i, role) in WideRole::ALL.into_iter().enumerate() {
            assert_eq!(role.index(), i);
            assert_eq!(WideRole::from_index(i), Some(role));
        }
        assert_eq!(WideRole::from_index(WideRole::COUNT), None);
        assert_eq!(WideRole::ALL.len(), WideRole::COUNT);
    }

    #[test]
    fn first_six_match_concrete_role_order() {
        use crate::Role;
        let concrete = Role::ALL;
        let wide = [
            WideRole::Pawn,
            WideRole::Knight,
            WideRole::Bishop,
            WideRole::Rook,
            WideRole::Queen,
            WideRole::King,
        ];
        for i in 0..6 {
            assert_eq!(wide[i].index(), i);
            assert_eq!(wide[i].char(), concrete[i].char());
            assert_eq!(wide[i].upper_char(), concrete[i].upper_char());
        }
    }

    #[test]
    fn char_round_trips_for_named_roles() {
        for role in WideRole::ALL {
            let ch = role.char();
            if ch == '?' {
                // Reserved roles have no character and do not parse.
                assert!(matches!(
                    role,
                    WideRole::Reserved0
                        | WideRole::Reserved1
                        | WideRole::Reserved2
                        | WideRole::Reserved3
                ));
                assert_eq!(WideRole::from_char(ch), None);
            } else {
                assert_eq!(WideRole::from_char(ch), Some(role));
                assert_eq!(WideRole::from_char(role.upper_char()), Some(role));
                assert_eq!(role.char().to_ascii_uppercase(), role.upper_char());
            }
        }
        assert_eq!(WideRole::from_char('x'), None);
        assert_eq!(WideRole::from_char('1'), None);
    }

    #[test]
    fn named_role_chars_are_distinct() {
        let chars: Vec<char> = WideRole::ALL
            .into_iter()
            .map(WideRole::char)
            .filter(|&c| c != '?')
            .collect();
        let mut sorted = chars.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), chars.len(), "role chars must be distinct");
        // Fourteen named roles (the six standard plus eight fairy).
        assert_eq!(chars.len(), 14);
    }
}
