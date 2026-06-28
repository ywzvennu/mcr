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

    // --- Spartan army (the Spartan/black asymmetric pieces, §4.4) ---
    /// Lieutenant — a Spartan leaper: one step sideways or diagonally (the six
    /// squares one file away) plus a two-square diagonal jump. No straight
    /// forward/backward step. (Spartan chess.)
    Lieutenant = 14,
    /// General — Rook + Ferz: orthogonal slides plus a single diagonal step.
    /// (Spartan chess.)
    General = 15,
    /// Captain — Wazir + Dabbaba: a single orthogonal step plus a two-square
    /// orthogonal jump. (Spartan chess.)
    Captain = 16,
    /// Hoplite — the Spartan Berolina pawn: moves one square diagonally forward
    /// (two from its start rank), captures one square straight forward. (Spartan
    /// chess.) The Warlord (Bishop + Knight) reuses [`WideRole::Hawk`].
    Hoplite = 17,

    /// Fers-Alfil — the Shako elephant: a leaper to the four adjacent diagonal
    /// squares (Ferz) **and** the four squares two diagonal steps away (Alfil),
    /// jumping over the intervening square. (Shako; FSF's `FERS_ALFIL`, Betza
    /// `FA`.) Distinct from the Rook + Knight [`WideRole::Elephant`] (the
    /// Capablanca/Grand marshal), which already claims the `e` letter; this one
    /// takes `v`, and the `compare-fairy` harness maps it to FSF's `e` when
    /// driving Shako.
    FersAlfil = 18,

    // --- Xiangqi (Chinese chess) army (§ Phase 3) ---
    /// Advisor / Guard (仕/士) — a Ferz confined to the palace: one diagonal step.
    /// (Xiangqi.) FSF spells it `a`, but `a` already names the Hawk here, so the
    /// Advisor takes the free letter `u` and the `compare-fairy` harness maps it
    /// to FSF's `a` when driving Xiangqi.
    Advisor = 19,
    /// Horse (馬) — a knight whose leap is **blocked by a hobbling leg**: the
    /// orthogonally-adjacent square in the direction of the leap's long axis. Its
    /// attack set is occupancy-aware (see
    /// [`attacks::horse_attacks`](super::attacks::horse_attacks)). (Xiangqi.) FSF
    /// spells it `n`, but `n` already names the (unobstructed) Knight here, so the
    /// Horse takes the free letter `h`… occupied — it takes `j`, and the harness
    /// maps it to FSF's `n`.
    Horse = 20,
    /// Elephant / Minister (相/象) — a **blockable** two-diagonal leaper confined
    /// to its own river-half: it jumps exactly two squares diagonally unless the
    /// intervening "eye" is occupied (see
    /// [`attacks::elephant_attacks_blockable`](super::attacks::elephant_attacks_blockable)).
    /// (Xiangqi.) Distinct from both the Rook+Knight [`WideRole::Elephant`] (`e`)
    /// and the Shako Fers-Alfil [`WideRole::FersAlfil`] (`v`); FSF spells it `b`,
    /// already the Bishop here, so it takes the free letter `o` and the harness
    /// maps it to FSF's `b`.
    XiangqiElephant = 21,
    /// Soldier / Pawn (兵/卒) — moves one step straight forward; **after crossing
    /// the river** it may also step one square sideways. Never backward, no
    /// double-step, no promotion. (Xiangqi.) FSF spells it `p`, already the Pawn
    /// here, so the Soldier takes the free letter `z` and the harness maps it to
    /// FSF's `p`.
    Soldier = 22,

    // --- Shogi promoted pieces (§ Phase 3, Milestone 10) ---
    //
    // A promoted Shogi piece is a **distinct role** from its base: it keeps its
    // promoted movement on the board but, when captured, reverts to the base role
    // in the captor's hand. Its FEN token is the base letter with a `+` prefix
    // (`+P`, `+L`, `+N`, `+S`, `+R`, `+B`), matching FSF; the board FEN parser /
    // writer handles the prefix, and [`promoted_base`](WideRole::promoted_base)
    // gives the role to bank on capture.
    /// Tokin (と) — a promoted Pawn (`+P`). Moves as a Gold General. Reverts to a
    /// Pawn in hand when captured. (Shogi.)
    Tokin = 23,
    /// Promoted Lance (成香, `+L`) — moves as a Gold General; reverts to a Lance
    /// in hand when captured. (Shogi.)
    PromotedLance = 24,
    /// Promoted Knight (成桂, `+N`) — moves as a Gold General; reverts to a Knight
    /// in hand when captured. (Shogi.)
    PromotedKnight = 25,
    /// Promoted Silver (成銀, `+S`) — moves as a Gold General; reverts to a Silver
    /// in hand when captured. (Shogi.)
    PromotedSilver = 26,
    /// Dragon King (龍, `+R`) — a promoted Rook: rook slides **plus** a single
    /// diagonal step in each direction. Reverts to a Rook in hand when captured.
    /// (Shogi.)
    Dragon = 27,
    /// Dragon Horse (馬, `+B`) — a promoted Bishop: bishop slides **plus** a single
    /// orthogonal step in each direction. Reverts to a Bishop in hand when
    /// captured. (Shogi.) Distinct from the Xiangqi [`WideRole::Horse`] (the
    /// hobbled knight), which already claims the `j` letter.
    DragonHorse = 28,
}

impl WideRole {
    /// The number of distinct roles, i.e. the length of [`WideRole::ALL`] and
    /// the size of a [`Board<G>`](super::Board)'s per-role mask array.
    ///
    /// This grows as fairy variants land and add roles.
    pub const COUNT: usize = 29;

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
        WideRole::Lieutenant,
        WideRole::General,
        WideRole::Captain,
        WideRole::Hoplite,
        WideRole::FersAlfil,
        WideRole::Advisor,
        WideRole::Horse,
        WideRole::XiangqiElephant,
        WideRole::Soldier,
        WideRole::Tokin,
        WideRole::PromotedLance,
        WideRole::PromotedKnight,
        WideRole::PromotedSilver,
        WideRole::Dragon,
        WideRole::DragonHorse,
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
            // Spartan army. FSF's `spartan` uses `l g c w h`, but `g`, `c`, and
            // `l` already name the Gold, Cannon, and Lance here; the Spartan
            // pieces take distinct free letters (`t d i h`), and the
            // `compare-fairy` harness maps them to FSF's letters when driving it.
            WideRole::Lieutenant => 't',
            WideRole::General => 'd',
            WideRole::Captain => 'i',
            WideRole::Hoplite => 'h',
            // Shako Fers-Alfil elephant. FSF's `shako` spells it `e`, but `e`
            // already names the Rook+Knight Elephant (marshal) here; the
            // Fers-Alfil takes the free letter `v`, and the `compare-fairy`
            // harness maps it to FSF's `e` when driving Shako.
            WideRole::FersAlfil => 'v',
            // Xiangqi army. FSF spells these `a n b p`, but those letters already
            // name the Hawk, Knight, Bishop, and Pawn here; the Xiangqi pieces
            // take distinct free letters (`u j o z`), and the `compare-fairy`
            // harness maps them to FSF's letters when driving Xiangqi.
            WideRole::Advisor => 'u',
            WideRole::Horse => 'j',
            WideRole::XiangqiElephant => 'o',
            WideRole::Soldier => 'z',
            // Shogi promoted pieces share their base role's letter: their FEN
            // token is the base letter with a `+` prefix (`+P`, `+L`, `+N`, `+S`,
            // `+R`, `+B`), so the bare `char()` returns the base letter and the
            // board FEN I/O adds the prefix. They are never dropped (drops are
            // always the unpromoted base role), so `char()` is used only for the
            // promoted board-FEN token and display.
            WideRole::Tokin => 'p',
            WideRole::PromotedLance => 'l',
            WideRole::PromotedKnight => 'n',
            WideRole::PromotedSilver => 's',
            WideRole::Dragon => 'r',
            WideRole::DragonHorse => 'b',
        }
    }

    /// Returns `true` if this is a Shogi **promoted** role — a piece that moves
    /// as its promoted form on the board but reverts to a base role in hand when
    /// captured. Its FEN token carries a `+` prefix.
    #[must_use]
    #[inline]
    pub const fn is_promoted(self) -> bool {
        matches!(
            self,
            WideRole::Tokin
                | WideRole::PromotedLance
                | WideRole::PromotedKnight
                | WideRole::PromotedSilver
                | WideRole::Dragon
                | WideRole::DragonHorse
        )
    }

    /// For a Shogi promoted role, the **base** role it reverts to when captured
    /// (and from which it was promoted); for any other role, the role itself.
    #[must_use]
    #[inline]
    pub const fn promoted_base(self) -> WideRole {
        match self {
            WideRole::Tokin => WideRole::Pawn,
            WideRole::PromotedLance => WideRole::Lance,
            WideRole::PromotedKnight => WideRole::Knight,
            WideRole::PromotedSilver => WideRole::Silver,
            WideRole::Dragon => WideRole::Rook,
            WideRole::DragonHorse => WideRole::Bishop,
            other => other,
        }
    }

    /// For a base Shogi role, the **promoted** role it becomes; for a role that
    /// has no Shogi promotion, the role itself.
    #[must_use]
    #[inline]
    pub const fn promoted_form(self) -> WideRole {
        match self {
            WideRole::Pawn => WideRole::Tokin,
            WideRole::Lance => WideRole::PromotedLance,
            WideRole::Knight => WideRole::PromotedKnight,
            WideRole::Silver => WideRole::PromotedSilver,
            WideRole::Rook => WideRole::Dragon,
            WideRole::Bishop => WideRole::DragonHorse,
            other => other,
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
            't' => Some(WideRole::Lieutenant),
            'd' => Some(WideRole::General),
            'i' => Some(WideRole::Captain),
            'h' => Some(WideRole::Hoplite),
            'v' => Some(WideRole::FersAlfil),
            'u' => Some(WideRole::Advisor),
            'j' => Some(WideRole::Horse),
            'o' => Some(WideRole::XiangqiElephant),
            'z' => Some(WideRole::Soldier),
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
            WideRole::Lieutenant => "lieutenant",
            WideRole::General => "general",
            WideRole::Captain => "captain",
            WideRole::Hoplite => "hoplite",
            WideRole::FersAlfil => "fers-alfil",
            WideRole::Advisor => "advisor",
            WideRole::Horse => "horse",
            WideRole::XiangqiElephant => "xiangqi-elephant",
            WideRole::Soldier => "soldier",
            WideRole::Tokin => "tokin",
            WideRole::PromotedLance => "promoted-lance",
            WideRole::PromotedKnight => "promoted-knight",
            WideRole::PromotedSilver => "promoted-silver",
            WideRole::Dragon => "dragon",
            WideRole::DragonHorse => "dragon-horse",
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
        // Every non-promoted role names a distinct letter, so each round-trips
        // through its character. The Shogi promoted roles share their base role's
        // letter (their FEN token is `+`-prefixed and handled by the board parser),
        // so `from_char` maps the bare letter back to the *base* role, not the
        // promoted one — they are excluded from this round-trip.
        for role in WideRole::ALL {
            if role.is_promoted() {
                continue;
            }
            let ch = role.char();
            assert_ne!(ch, '?', "every role has a letter");
            assert_eq!(WideRole::from_char(ch), Some(role));
            assert_eq!(WideRole::from_char(role.upper_char()), Some(role));
            assert_eq!(role.char().to_ascii_uppercase(), role.upper_char());
        }
        assert_eq!(WideRole::from_char('x'), None);
        assert_eq!(WideRole::from_char('1'), None);
    }

    #[test]
    fn promoted_roles_revert_to_base() {
        // Each Shogi promoted role reverts to its base, and the base promotes to
        // it; non-Shogi roles are their own base and promoted form.
        let pairs = [
            (WideRole::Tokin, WideRole::Pawn),
            (WideRole::PromotedLance, WideRole::Lance),
            (WideRole::PromotedKnight, WideRole::Knight),
            (WideRole::PromotedSilver, WideRole::Silver),
            (WideRole::Dragon, WideRole::Rook),
            (WideRole::DragonHorse, WideRole::Bishop),
        ];
        for (promoted, base) in pairs {
            assert!(promoted.is_promoted());
            assert!(!base.is_promoted());
            assert_eq!(promoted.promoted_base(), base);
            assert_eq!(base.promoted_form(), promoted);
            // A promoted role and its base share a FEN letter.
            assert_eq!(promoted.char(), base.char());
        }
        // A role with no Shogi promotion is its own base and form.
        assert_eq!(WideRole::King.promoted_base(), WideRole::King);
        assert_eq!(WideRole::King.promoted_form(), WideRole::King);
    }

    #[test]
    fn named_role_chars_are_distinct() {
        // Every non-promoted role names a distinct letter. The Shogi promoted
        // roles deliberately reuse their base role's letter (FEN `+`-prefix), so
        // they are excluded from the distinctness check.
        let chars: Vec<char> = WideRole::ALL
            .into_iter()
            .filter(|r| !r.is_promoted())
            .map(WideRole::char)
            .filter(|&c| c != '?')
            .collect();
        let mut sorted = chars.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), chars.len(), "role chars must be distinct");
    }
}
