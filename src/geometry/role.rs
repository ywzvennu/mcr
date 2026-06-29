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

/// The FEN prefix marking an **overflow** role — a fairy role added after the
/// single-letter alphabet (`a..=z`) was exhausted. The token is this prefix
/// followed by a recycled base letter whose case carries the colour (e.g. `*U` /
/// `*u` for the Synochess [`WideRole::Commoner`]). It is the overflow analogue of
/// the `+` prefix the Shogi promoted roles use, and is reserved: no role's bare
/// letter is `*`. See [`WideRole::is_overflow`].
pub const OVERFLOW_PREFIX: char = '*';

/// The **third** overflow prefix, for roles added once the single-letter FEN
/// alphabet (`a..=z`), every `*`-prefixed [`OVERFLOW_PREFIX`] base **and** the
/// doubled-`**` second tier ([`WideRole::is_overflow2`], the Sho Shogi royals)
/// were all in play and a distinct, non-colliding bank was needed for the Cannon
/// Shogi cannon army (whose recycled letters `c` / `e` would clash with the Sho
/// Shogi `**` royals). The token is this prefix followed by a recycled base letter
/// whose case carries the colour (e.g. `=A` / `=a` for the Cannon Shogi
/// [`WideRole::RookCannon`]). There is no `OVERFLOW_PREFIX_2` constant: the second
/// tier reuses [`OVERFLOW_PREFIX`] **doubled** (`**`). Distinct from `*` (the first
/// overflow tier), `**` (the second), `+` (Shogi promotions) and `~` (the
/// crazyhouse promoted-mask), it is reserved: no role's bare letter is `=`. See
/// [`WideRole::is_overflow3`].
pub const OVERFLOW_PREFIX_3: char = '=';

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
    /// Wazir — one orthogonal step. Fielded as the **Giraffe** of Dobutsu (3x4
    /// animal shogi), which steps one square orthogonally in any direction. Its
    /// bare letter `w` was reclaimed for the Orda [`WideRole::Kheshig`], so — like
    /// the [`WideRole::Commoner`] and the Empire/Chak pieces past the exhausted
    /// single-letter alphabet — the Wazir is an **overflow role**: its FEN token is
    /// the [`OVERFLOW_PREFIX`] (`*`) followed by the recycled base letter `j` (the
    /// [`WideRole::Horse`]'s, distinct by the `*` prefix; chosen clear of the Tori
    /// [`WideRole::Goose`] which already recycles `g` as `*g`), so
    /// `char()` returns the bare base letter and the board FEN I/O adds the `*`
    /// prefix. The `compare-fairy` harness maps `*j` to FSF's `g` (the Giraffe) when
    /// driving Dobutsu.
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

    // --- Janggi (Korean chess) elephant (§ Phase 3, Milestone 10) ---
    /// Janggi Elephant (象) — moves one square orthogonally then **two** squares
    /// diagonally outward (a `(±2,±3)` / `(±3,±2)` leap, longer than the Xiangqi
    /// elephant's `(±2,±2)`), **blockable** at each intervening square and **not**
    /// river-bound (see
    /// [`attacks::janggi_elephant_attacks`](super::attacks::janggi_elephant_attacks)).
    /// FSF spells it `b`, already the Bishop here, and the Xiangqi elephant already
    /// took `o`, so the Janggi elephant takes the free letter `x` and the
    /// `compare-fairy` harness maps it to FSF's `b` when driving Janggi.
    JanggiElephant = 29,

    // --- Orda (Mongolian) army (§ Milestone 10) --------------------------
    //
    // The Black "Orda" cavalry pieces: every one **moves like a knight** to an
    // empty square but **captures along a slider line** (Lancer = knight-move /
    // rook-capture; Archer = knight-move / bishop-capture), plus the Kheshig, a
    // King+Knight leaper that moves and captures alike. The Yurt is a plain
    // silver-general and reuses [`WideRole::Silver`], so it needs no new role. All
    // were confirmed against Fairy-Stockfish `UCI_Variant orda`.
    /// Lancer (FSF `kniroo`, letter `l`) — **moves like a knight** to an empty
    /// square but **captures like a rook** (an orthogonal slider capture). Its
    /// quiet jumps ride [`WideVariant::quiet_only_targets`](super::WideVariant::quiet_only_targets)
    /// (the knight pattern); its [`role_attacks`](super::WideVariant::role_attacks)
    /// is the rook slide (the only squares it captures / checks on). (Orda.) FSF's
    /// `l` already names the Lance here, so the Lancer takes the free letter `f`
    /// and the `compare-fairy` harness maps it to FSF's `l` when driving Orda.
    Lancer = 30,
    /// Kheshig (FSF `centaur`, letter `h`) — a **King + Knight** leaper: it moves
    /// and captures to the eight king-adjacent squares **and** the eight knight
    /// squares (sixteen targets), all as a non-sliding leaper. (Orda; also the
    /// piece a promoting pawn may become, see the Orda promotion targets.) FSF's
    /// `h` already names the Hoplite here; the single-letter alphabet is otherwise
    /// exhausted, so the Kheshig reclaims the letter `w` — freed from the
    /// [`WideRole::Wazir`], which now carries the overflow token `*j` as Dobutsu's
    /// Giraffe (see its doc) rather than a bare letter — and the `compare-fairy`
    /// harness maps the Kheshig to FSF's `h` when driving Orda.
    Kheshig = 31,
    /// Archer (FSF `knibis`, letter `a`) — **moves like a knight** to an empty
    /// square but **captures like a bishop** (a diagonal slider capture). Like the
    /// Lancer its quiet jumps ride
    /// [`quiet_only_targets`](super::WideVariant::quiet_only_targets) (the knight
    /// pattern) and its [`role_attacks`](super::WideVariant::role_attacks) is the
    /// bishop slide (the only squares it captures / checks on). (Orda.) FSF's `a`
    /// already names the Hawk here, so the Archer takes the free letter `y` and the
    /// `compare-fairy` harness maps it to FSF's `a` when driving Orda.
    Archer = 32,

    // --- Synochess (§ Milestone 10, Fairy variants) ---
    /// Commoner / Man — a non-royal piece that **moves and captures exactly like a
    /// king** (one step in any of the eight directions) but may itself be captured.
    /// This is Synochess's Black "Advisor", which — unlike the palace-confined
    /// Xiangqi [`WideRole::Advisor`] (`u`) — roams the whole board. FSF spells it
    /// `a` (already the Hawk here).
    ///
    /// The Commoner is the **first role past the single-letter alphabet**: by the
    /// time it lands the Orda army has claimed the last free letters (`f y` plus
    /// the reclaimed `w`), so every one of `a..=z` already names a role. Rather
    /// than reshuffle the exhausted alphabet, the Commoner takes an **overflow FEN
    /// token** — the prefix [`OVERFLOW_PREFIX`] (`*`) followed by a recycled base
    /// letter that carries the colour via its case, exactly mirroring how the Shogi
    /// promoted roles spell themselves with the `+` prefix (see
    /// [`is_overflow`](WideRole::is_overflow) / [`overflow_base_char`](WideRole::overflow_base_char)).
    /// The Commoner recycles the Advisor's base letter `u`, so its token is `*U`
    /// (white) / `*u` (black); the `compare-fairy` harness maps `*u → a` when
    /// driving Synochess.
    Commoner = 33,

    // --- Shinobi clan pieces (§ Phase 3, Milestone 10) ---
    //
    // Shinobi's Black "clan" reuses the existing Commoner (the king-stepping
    // non-royal of Synochess, role 33 / token `*u`) for its own commoner, and
    // its Bers reuses [`WideRole::General`] (Rook + Ferz) and its Archbishop
    // reuses [`WideRole::Hawk`] (Bishop + Knight). The only genuinely new piece
    // is the forward Shogi Knight.
    /// Shogi Knight — a forward-only 2-1 leaper: it jumps two squares forward and
    /// one to the side (two targets), leaping over any piece, and never moves
    /// backward or sideways. (Shinobi; FSF's `shogiKnight`.) Distinct from the
    /// standard [`WideRole::Knight`] (Black's army keeps ordinary knights), so it
    /// is a separate role. It promotes into a standard Knight.
    ///
    /// Landing past the single-letter alphabet (every `a..=z` already names a
    /// role), the Shogi Knight is an **overflow role** like the Commoner: it has
    /// no bare letter and spells itself with the [`OVERFLOW_PREFIX`] (`*`)
    /// followed by a recycled base letter whose case carries the colour. FSF
    /// spells it `h`, but the recycled token reuses the forward-leap mnemonic `n`
    /// (the Knight's letter, free for recycling), so its token is `*N` (white) /
    /// `*n` (black); the `compare-fairy` harness maps `*n → h` when driving
    /// Shinobi.
    ShogiKnight = 34,

    // --- Ordamirror Falcon (§ Phase 3, Milestone 10) ---
    //
    // Ordamirror's symmetric horde reuses the Orda [`WideRole::Lancer`] /
    // [`WideRole::Kheshig`] / [`WideRole::Archer`] for both armies; its one
    // genuinely-new piece is the Falcon.
    /// Falcon (FSF `customPiece1 = f:mQcN`) — the inverse of the Lancer/Archer:
    /// it **moves like a queen** (any number of squares along a rank, file, or
    /// diagonal, to an **empty** square) but **captures like a knight** (a 2-1
    /// leap). Its quiet queen slides are non-capturing and its only attacking /
    /// checking squares are the eight knight jumps. (Ordamirror.)
    ///
    /// Landing past the single-letter alphabet (every `a..=z` already names a
    /// role), the Falcon is an **overflow role** like the Commoner and Shogi
    /// Knight: it has no bare letter and spells itself with the
    /// [`OVERFLOW_PREFIX`] (`*`) followed by a recycled base letter whose case
    /// carries the colour. FSF spells it `f`, but `f` already names the Lancer
    /// here, so the Falcon recycles that same mnemonic as its overflow base: its
    /// token is `*F` (white) / `*f` (black), distinct from the bare Lancer `f`.
    /// The `compare-fairy` harness maps `*f → f` when driving Ordamirror.
    Falcon = 35,

    // --- Empire (Roman) army (§ Milestone 10, Fairy variants) ------------
    //
    // The White "Empire" pieces are four long-range "move-far / capture-close"
    // compounds: each **moves like a Queen** to an empty square but **captures
    // only on a short fixed pattern**. They are the long-range analogue of the
    // Orda cavalry (knight-move / slider-capture). Confirmed square-for-square
    // against Fairy-Stockfish `UCI_Variant empire` (its `variants.ini` custom
    // pieces `e:mQcN`, `c:mQcB`, `t:mQcR`, `d:mQcK`). The Empire Soldier reuses
    // the existing [`WideRole::Soldier`] (forward / sideways stepper) and the
    // Emperor is a plain royal [`WideRole::King`], so neither needs a new role.
    //
    // All four land **past the exhausted single-letter alphabet**, so — like the
    // Commoner and Shogi Knight — they are **overflow roles** with no bare letter:
    // each spells itself with the [`OVERFLOW_PREFIX`] (`*`) plus a distinct
    // recycled base letter (the FSF mnemonic) whose case carries the colour. The
    // `compare-fairy` harness strips the `*` (e.g. `*e → e`) when driving FSF.
    /// Eagle (FSF custom `e:mQcN`) — **moves like a Queen** to an empty square but
    /// **captures like a Knight** (the eight 2-1 leaps). Its quiet Queen slides
    /// ride [`quiet_only_targets`](super::WideVariant::quiet_only_targets); its
    /// [`role_attacks`](super::WideVariant::role_attacks) is the knight pattern
    /// (its only capturing / checking squares). (Empire.) An **overflow role**: its
    /// FEN token is `*E` (white) / `*e` (black), recycling the Empire Eagle's FSF
    /// letter `e` (already the Rook+Knight Elephant's bare letter here), and the
    /// `compare-fairy` harness maps `*e → e` when driving Empire.
    Eagle = 36,
    /// Cardinal (FSF custom `c:mQcB`) — **moves like a Queen** to an empty square
    /// but **captures like a Bishop** (a diagonal slider capture). Like the Eagle
    /// its quiet Queen slides ride
    /// [`quiet_only_targets`](super::WideVariant::quiet_only_targets) and its
    /// [`role_attacks`](super::WideVariant::role_attacks) is the bishop slide.
    /// (Empire.) An **overflow role**: its FEN token is `*C` (white) / `*c`
    /// (black), recycling the FSF letter `c` (already the Cannon's bare letter
    /// here), and the harness maps `*c → c` when driving Empire.
    Cardinal = 37,
    /// Tower (FSF custom `t:mQcR`) — **moves like a Queen** to an empty square but
    /// **captures like a Rook** (an orthogonal slider capture). Like the Eagle its
    /// quiet Queen slides ride
    /// [`quiet_only_targets`](super::WideVariant::quiet_only_targets) and its
    /// [`role_attacks`](super::WideVariant::role_attacks) is the rook slide.
    /// (Empire.) An **overflow role**: its FEN token is `*T` (white) / `*t`
    /// (black), recycling the FSF letter `t` (already the Spartan Lieutenant's bare
    /// letter here), and the harness maps `*t → t` when driving Empire.
    Tower = 38,
    /// Duke (FSF custom `d:mQcK`) — **moves like a Queen** to an empty square but
    /// **captures like a King** (the eight one-step squares). Like the Eagle its
    /// quiet Queen slides ride
    /// [`quiet_only_targets`](super::WideVariant::quiet_only_targets) and its
    /// [`role_attacks`](super::WideVariant::role_attacks) is the king pattern.
    /// (Empire.) An **overflow role**: its FEN token is `*D` (white) / `*d`
    /// (black), recycling the FSF letter `d` (already the Spartan/Shinobi General's
    /// bare letter here), and the harness maps `*d → d` when driving Empire.
    Duke = 39,

    // --- Hoppel-Poppel move≠capture pieces (§ Phase 3, Milestone 10) ---
    //
    // Hoppel-Poppel keeps the standard chess army except its "knight" and "bishop"
    // swap *capture* methods: the knight captures like a bishop, the bishop captures
    // like a knight (each still *moves* like its own piece). They are two genuinely
    // distinct move≠capture roles, separate from the standard Knight / Bishop AND
    // from Orda's Lancer / Archer (which are knight-MOVE / slider-capture). Like the
    // Commoner they are **overflow roles** past the exhausted single-letter alphabet.
    /// Knight-Bishop (FSF `KNIBIS`, Betza `mNcB`) — Hoppel-Poppel's "knight":
    /// **moves like a knight** (the eight 2-1 leaps) to an empty square but
    /// **captures like a bishop** (a diagonal slide). Its quiet knight jumps ride
    /// [`quiet_only_targets`](super::WideVariant::quiet_only_targets) and its
    /// [`role_attacks`](super::WideVariant::role_attacks) (the attack / capture /
    /// check relation) is the bishop slide. (Hoppel-Poppel.) An **overflow role**:
    /// its FEN token is `*H` (white) / `*h` (black) — the base letter `h` (the
    /// "**H**oppel" mnemonic, since the FSF letter `n` is already the ShogiKnight's
    /// recycled base), distinct from the bare Hoplite `h`. The `compare-fairy`
    /// harness maps `*h → n` when driving Hoppel-Poppel.
    KnightBishop = 40,
    /// Bishop-Knight (FSF `BISKNI`, Betza `mBcN`) — Hoppel-Poppel's "bishop":
    /// **moves like a bishop** (a diagonal slide) to an empty square but **captures
    /// like a knight** (a 2-1 leap). The inverse of the Knight-Bishop: its quiet
    /// bishop slides ride [`quiet_only_targets`](super::WideVariant::quiet_only_targets)
    /// and its [`role_attacks`](super::WideVariant::role_attacks) is the knight
    /// pattern. (Hoppel-Poppel.) An **overflow role**: its FEN token is `*B` (white)
    /// / `*b` (black), recycling the FSF `BISKNI` letter `b` (distinct from the bare
    /// Bishop `b` by the `*` prefix). The `compare-fairy` harness maps `*b → b` when
    /// driving Hoppel-Poppel.
    BishopKnight = 41,

    // --- Manchu super-piece (§ Phase 3, Milestone 10) ---
    //
    // Manchu (yipaisanxianqi) is an asymmetric Xiangqi: one side keeps a full
    // Xiangqi army, the other replaces its rook/cannon/horse cluster with a single
    // SUPER-PIECE — the "Banner" (FSF `BANNER`, Betza `RcpRnN`), which combines the
    // Chariot's rook slide, the Cannon's over-screen capture, and the Horse's
    // hobbled knight leap in one piece. It is an **overflow role** like the
    // Commoner: it has no bare letter and spells itself with the [`OVERFLOW_PREFIX`]
    // (`*`) plus the recycled base letter `m` (FSF's `m`, distinct from the bare
    // Makruk Met `m` by the `*` prefix). The `compare-fairy` harness maps `*m → m`
    // when driving Manchu.
    /// Banner (FSF `BANNER`, Betza `RcpRnN`) — the Manchu super-piece: it **moves
    /// and captures like a Chariot** (a rook slide), **captures like a Cannon** (a
    /// jump over exactly one screen onto the next piece), and **moves and captures
    /// like a Horse** (a knight leap hobbled by an occupied leg). Its full
    /// occupancy-dependent attack-and-move set is computed from the live board via
    /// [`role_attacks_board`](super::WideVariant::role_attacks_board) (the cannon
    /// part lands only on an occupied square, so the set is occupancy-asymmetric).
    /// (Manchu.) An **overflow role**: its FEN token is `*M` (white) / `*m` (black),
    /// recycling FSF's Banner letter `m` (distinct from the bare Met `m` by the `*`
    /// prefix). The `compare-fairy` harness maps `*m → m` when driving Manchu.
    Banner = 42,

    // --- Chak (9x9 Mayan) army (§ Milestone 10, Fairy variants) ----------
    //
    // Chak (Couch Tomato, https://www.pychess.org/variants/chak) is a 9x9 Mayan
    // variant on the [`Shogi9x9`](super::Shogi9x9) geometry. Confirmed
    // square-for-square against Fairy-Stockfish `UCI_Variant chak` (its
    // `variants.ini` custom pieces). Two of its eight piece kinds reuse existing
    // roles — the **Vulture** (`v`) is a plain [`WideRole::Knight`] and the
    // **Jaguar** (`j`) is a King + Knight centaur, exactly the Orda
    // [`WideRole::Kheshig`]; the **King** (`k`) is a plain royal
    // [`WideRole::King`] (it promotes to the Divine Lord) — so neither needs a new
    // role. The remaining six pieces are genuinely new and, landing **past the
    // exhausted single-letter alphabet**, are all **overflow roles** spelled with
    // the [`OVERFLOW_PREFIX`] (`*`) plus a recycled base letter whose case carries
    // the colour. The `compare-fairy` harness strips the `*` (e.g. `*s → s`) when
    // driving FSF.
    /// Serpent (FSF `customPiece1 = s:FvW`) — a leaper to the **four diagonals**
    /// (Ferz) **and** one step straight forward or backward (vertical Wazir): six
    /// targets, no sideways step. (Chak.) An **overflow role**: its FEN token is
    /// `*S` (white) / `*s` (black), recycling the FSF letter `s` (already the
    /// Silver's bare letter here), and the `compare-fairy` harness maps `*s → s`
    /// when driving Chak.
    Serpent = 43,
    /// Quetzal (FSF `customPiece2 = q:pQ`) — an **eight-direction cannon**: it
    /// moves and captures like a Queen but **only by hopping exactly one screen**
    /// (a piece of either colour) along a rank, file, or diagonal, landing on any
    /// empty square or the first enemy beyond the screen; it has no move on an
    /// unobstructed line and cannot land on the screen. Its full occupancy-aware
    /// set is computed from the live board via
    /// [`role_attacks_board`](super::WideVariant::role_attacks_board) (the capture
    /// part lands only beyond a screen, so the relation is occupancy-asymmetric).
    /// (Chak.) An **overflow role**: its FEN token is `*Q` (white) / `*q` (black),
    /// recycling the FSF letter `q` (already the Queen's bare letter here), and the
    /// `compare-fairy` harness maps `*q → q` when driving Chak.
    Quetzal = 44,
    /// Shaman (FSF `customPiece6 = w:FvW`) — moves exactly like the Serpent (four
    /// diagonals plus a vertical Wazir step) but is **confined to its own half of
    /// the board** (White ranks 5-9, Black ranks 1-5; FSF `mobilityRegion…`), so
    /// it never moves or captures across the centre line. It is the **promoted
    /// form of the Soldier**. (Chak.) An **overflow role**: its FEN token is `*W`
    /// (white) / `*w` (black), recycling the FSF letter `w` (already the Kheshig's
    /// bare letter here, distinct by the `*` prefix), and the `compare-fairy`
    /// harness maps `*w → w` when driving Chak.
    Shaman = 45,
    /// Divine Lord (FSF `customPiece3 = d:mQ2cQ2`) — moves and captures like a
    /// **Queen limited to two squares** (a blockable range-2 slider), **confined
    /// to its own half** (White ranks 5-9, Black ranks 1-5) exactly like the
    /// Shaman, and is the **promoted form of the King**. It is **royal** (the
    /// promoted King): a side that loses *both* its King and its Lord has lost
    /// (FSF `extinctionPieceTypes = kd`, `extinctionPseudoRoyal`), and a Lord
    /// reaching the enemy temple square wins (FSF `flagPiece = d`). (Chak.) An
    /// **overflow role**: its FEN token is `*L` (white) / `*l` (black), the base
    /// letter `l` (the "**L**ord" mnemonic, since the FSF letter `d` is already the
    /// General's recycled base), and the `compare-fairy` harness maps `*l → d` when
    /// driving Chak.
    DivineLord = 46,
    /// Chak Soldier (FSF `customPiece4 = p:fsmWfceF`) — **moves** one step
    /// forward or to either side (a forward/sideways Wazir, never backward) but
    /// **captures** only one step diagonally forward (a forward Ferz), unlike the
    /// Xiangqi [`WideRole::Soldier`] (`z`) which moves and captures alike. It
    /// **promotes to a Shaman** on reaching its own half. A move≠capture piece:
    /// its quiet forward/sideways steps ride
    /// [`quiet_targets_board`](super::WideVariant::quiet_targets_board) and its
    /// [`role_attacks`](super::WideVariant::role_attacks) is the forward-diagonal
    /// capture pattern. (Chak.) An **overflow role**: its FEN token is `*P` (white)
    /// / `*p` (black), recycling the FSF letter `p` (already the Pawn's bare letter
    /// here), and the `compare-fairy` harness maps `*p → p` when driving Chak.
    ChakSoldier = 47,
    /// Temple (FSF `immobile = o`) — the pyramid that sits on each side's central
    /// rank-2/rank-8 square: it **never moves**, but it can be **captured** like
    /// any other piece, and the square it sits on is the goal a Divine Lord wins
    /// by reaching. Its [`role_attacks`](super::WideVariant::role_attacks) set is
    /// always empty (it neither moves nor threatens). (Chak.) An **overflow role**:
    /// its FEN token is `*O` (white) / `*o` (black), recycling the FSF letter `o`
    /// (already the Xiangqi Elephant's bare letter here, distinct by the `*`
    /// prefix), and the `compare-fairy` harness maps `*o → o` when driving Chak.
    Temple = 48,

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

    // --- Tori Shogi (bird shogi, 7x7) army (§ Milestone 10, Fairy variants) ---
    //
    // Tori Shogi replaces the Shogi army with seven bird pieces on a 7x7 board.
    // Every one is a genuinely-new movement (confirmed square-for-square against
    // Fairy-Stockfish `UCI_Variant torishogi`), so each lands **past the
    // exhausted single-letter alphabet** as an **overflow role** like the
    // Commoner / Empire pieces: it has no bare letter and spells itself with the
    // [`OVERFLOW_PREFIX`] (`*`) plus a **distinct** recycled base letter whose
    // case carries the colour. FSF spells Tori pieces `s` (swallow), `f` (falcon,
    // already the Ordamirror Falcon's overflow base here), `c` (crane, the
    // Cannon's bare letter and the Empire Cardinal's overflow base), `l`/`r`
    // (quails) and `p` (pheasant); the promoted swallow and falcon are the `+S`
    // (goose) and `+F` (eagle) tokens. mce gives each its own overflow base,
    // chosen distinct from every other overflow role (the Chak army already
    // recycles `s`, `o`, `l` and `p`, so Tori cannot reuse them) — `y` swallow,
    // `g` goose, `a` falcon, `i` eagle, `k` crane, `v` left quail, `r` right
    // quail, `z` pheasant — and the `compare-fairy` harness rewrites each
    // `*<base>` to FSF's letter (`*y → s`, `*g → +S`, `*a → f`, `*i → +F`,
    // `*k → c`, `*v → l`, `*r → r`, `*z → p`) when driving FSF. The goose and
    // eagle are the *promoted* forms of the swallow and falcon: they move as a
    // distinct piece on the board but revert to the base (swallow / falcon) in
    // hand when captured. That base reversion is expressed by the variant's
    // [`role_hand_base`](super::WideVariant::role_hand_base) hook (as for Shogun's
    // role-reusing promotions), not by the global Shogi `+`-token machinery, so no
    // Tori role is [`is_promoted`](WideRole::is_promoted).
    /// Swallow (燕, FSF `s`) — moves one square straight **forward** (it both moves
    /// and captures there, like the Shogi pawn). Promotes to a Goose. (Tori
    /// Shogi.) An **overflow role**: its FEN token is `*Y` (white) / `*y` (black) —
    /// the base `s` being already claimed by the Chak Serpent — and the
    /// `compare-fairy` harness maps `*y → s` when driving Tori Shogi.
    Swallow = 49,
    /// Goose (雁, FSF `+S`) — the promoted Swallow: leaps two squares diagonally
    /// **forward** (a forward Alfil, jumping the intervening square) or two squares
    /// straight **backward** (a backward Dabbaba jump). Reverts to a Swallow in
    /// hand when captured. (Tori Shogi.) An **overflow role**: its FEN token is
    /// `*G` (white) / `*g` (black); the harness maps `*g → +S` when driving Tori.
    Goose = 50,
    /// Falcon (鷹, FSF `f`) — the Tori falcon: steps to all four diagonals (a Ferz)
    /// and one square **forward** or **sideways** orthogonally (every King step
    /// except the backward orthogonal one). Promotes to an Eagle. (Tori Shogi.)
    /// Distinct from the Ordamirror [`WideRole::Falcon`] (`mQcN`), which already
    /// claims the `f` overflow base; this one takes `a`. An **overflow role**: its
    /// FEN token is `*A` (white) / `*a` (black); the harness maps `*a → f`.
    ToriFalcon = 51,
    /// Eagle (鵰, FSF `+F`) — the promoted Falcon: a King step in every direction,
    /// a **backward** Rook slide, a **forward** Bishop slide, and a **backward**
    /// diagonal slide of up to two squares. Reverts to a Falcon in hand when
    /// captured. (Tori Shogi.) Distinct from the Empire [`WideRole::Eagle`]
    /// (`mQcN`), which already claims the `e` overflow base; this one takes `i`. An
    /// **overflow role**: its FEN token is `*I` (white) / `*i` (black); the harness
    /// maps `*i → +F`.
    ToriEagle = 52,
    /// Crane (鶴, FSF `c`) — steps to all four diagonals (a Ferz) and one square
    /// straight **forward** or **backward** orthogonally (every King step except
    /// the two sideways ones). (Tori Shogi.) Distinct from the Cannon (`c`), the
    /// Empire [`WideRole::Cardinal`] (`c` overflow base) and the Chak Temple (`o`);
    /// this one takes `k`. An **overflow role**: its FEN token is `*K` (white) /
    /// `*k` (black); the harness maps `*k → c`.
    Crane = 53,
    /// Left Quail (鶉, FSF `l`) — an **asymmetric** bird: a **forward** Rook slide,
    /// a **right-backward** Bishop slide, and one square **left-backward**
    /// diagonally. Its move set is *not* left-right symmetric (it is the mirror of
    /// the Right Quail). (Tori Shogi.) Distinct from the Chak Divine Lord, which
    /// already claims the `l` overflow base; this one takes `v`. An **overflow
    /// role**: its FEN token is `*V` (white) / `*v` (black); the harness maps
    /// `*v → l`.
    LeftQuail = 54,
    /// Right Quail (鶉, FSF `r`) — the mirror of the Left Quail: a **forward** Rook
    /// slide, a **left-backward** Bishop slide, and one square **right-backward**
    /// diagonally. Its move set is *not* left-right symmetric. (Tori Shogi.) An
    /// **overflow role**: its FEN token is `*R` (white) / `*r` (black); the harness
    /// maps `*r → r`.
    RightQuail = 55,
    /// Pheasant (雉, FSF `p`) — leaps two squares straight **forward** (a forward
    /// Dabbaba jump) and steps one square **backward** diagonally (a backward
    /// Ferz). (Tori Shogi.) Distinct from the Chak Soldier, which already claims
    /// the `p` overflow base; this one takes `z`. An **overflow role**: its FEN
    /// token is `*Z` (white) / `*z` (black); the harness maps `*z → p`.
    Pheasant = 56,

    // --- Shatranj (medieval chess) elephant (§ Milestone 10, Fairy variants) ---
    //
    // Shatranj is the medieval 8x8 ancestor of chess (FSF `UCI_Variant shatranj`).
    // Its Ferz (counselor) reuses the Makruk [`WideRole::Met`] (one diagonal step)
    // and its King / Knight / Rook are standard, so the only genuinely-new piece is
    // the Alfil (elephant).
    /// Alfil (FSF `b`, Betza `A`) — the Shatranj elephant: a **pure** two-square
    /// diagonal leaper that jumps to the four squares two diagonal steps away,
    /// over any intervening piece. Unlike the Shako [`WideRole::FersAlfil`] (`FA`),
    /// it has **no** one-step (Ferz) component, so it is a distinct, colour-bound
    /// leaper reaching only eight squares of the board. (Shatranj.) Landing **past
    /// the exhausted single-letter alphabet** (every `a..=z` already names a role),
    /// the Alfil is an **overflow role** like the Commoner: it has no bare letter
    /// and spells itself with the [`OVERFLOW_PREFIX`] (`*`) followed by a recycled
    /// base letter whose case carries the colour. FSF spells it `b` (already the
    /// Bishop here), and every FSF mnemonic letter is already claimed as an overflow
    /// base, so the Alfil recycles the one free letter `x` (the Janggi Elephant's
    /// bare letter, distinct by the `*` prefix): its token is `*X` (white) / `*x`
    /// (black), and the `compare-fairy` harness maps `*x → b` when driving Shatranj.
    Alfil = 57,

    // --- Sho Shogi (old 9x9 Shogi without drops) royals (§ Milestone 10) ---
    //
    // Sho Shogi reuses the whole Shogi army and its `+`-promotions, and adds the
    // **Drunk Elephant** (酔象) and its promoted form the **Crown Prince** (太子,
    // a SECOND royal piece). FSF (`UCI_Variant shoshogi`) spells the Drunk
    // Elephant `e` and the Crown Prince `+E`. Both are genuinely-new movements,
    // so each needs its own role — but the single-letter `*` overflow bank is
    // **exhausted** (every `a..=z` already names a `*<letter>` overflow role).
    // They therefore land in a **second overflow bank** spelled with the prefix
    // [`OVERFLOW_PREFIX`] **doubled** (`**`) plus a recycled base letter whose
    // case carries the colour (see [`is_overflow2`](WideRole::is_overflow2) /
    // [`overflow2_from_base`](WideRole::overflow2_from_base)). The promotion
    // Drunk Elephant → Crown Prince is the variant's
    // [`role_promoted_to`](super::WideVariant::role_promoted_to) (like Chak's King
    // → Divine Lord), not the global `+`-token machinery, so neither role is
    // [`is_promoted`](WideRole::is_promoted).
    /// Drunk Elephant (酔象, FSF `e`) — steps one square to any of **seven**
    /// directions: the four diagonals (Ferz) plus one step forward or sideways
    /// (every King step except the straight-backward one). Promotes to a Crown
    /// Prince. (Sho Shogi.) A **second-bank overflow role**: its FEN token is the
    /// doubled prefix `**` plus the recycled Elephant letter `e`, `**E` (white) /
    /// `**e` (black); the `compare-fairy` harness maps `**e → e` when driving Sho
    /// Shogi.
    DrunkElephant = 58,
    /// Crown Prince (太子, FSF `+E`) — the promoted Drunk Elephant: a full one-step
    /// King in every direction, and a **second royal** piece (a side is lost only
    /// when **both** its King and Crown Prince are captured / mated, FSF
    /// `extinctionPseudoRoyal` with `extinctionPieceCount = 0` — while a side holds
    /// both, neither is royal). (Sho Shogi.) A **second-bank overflow role**: its
    /// FEN token is the doubled prefix `**` plus the recycled Cannon letter `c`
    /// ("Crown"), `**C` (white) / `**c` (black); the harness maps `**c → +E`.
    CrownPrince = 59,

    // --- Cannon Shogi (大砲将棋) cannon army (§ Milestone 10, Fairy variants) ---
    //
    // Cannon Shogi adds five CANNON-type pieces to the 9x9 Shogi army (confirmed
    // square-for-square against Fairy-Stockfish `UCI_Variant cannonshogi`). The
    // Xiangqi rook-cannon reuses the existing [`WideRole::Cannon`] (`mRcpR`) and
    // the soldier reuses the [`WideRole::Pawn`] (a forward/sideways stepper), so
    // only three genuinely-new movers and their four promoted forms need roles.
    //
    // Every single-letter FEN base (`a..=z`), every `*`-prefixed overflow base and
    // the doubled-`**` second tier (the Sho Shogi royals, whose letters `c` / `e`
    // would clash) are already in play, so these roles spell themselves with the
    // **third** overflow prefix [`OVERFLOW_PREFIX_3`] (`=`) followed by a recycled
    // base letter whose case carries the colour, resolved by
    // [`is_overflow3`](WideRole::is_overflow3) /
    // [`overflow3_from_base`](WideRole::overflow3_from_base). They are **not**
    // [`is_promoted`] (the four promoted forms revert to their base via the
    // variant's [`role_hand_base`](super::WideVariant::role_hand_base) hook, as
    // Tori Shogi's birds do, not via the global `+`-token machinery). The
    // `compare-fairy` harness rewrites each `=<base>` to FSF's spelling.
    /// Rook-cannon (砲, FSF `a`, Betza `pR`) — moves **and** captures only by
    /// leaping over exactly one screen on a rook line, then sliding any distance
    /// beyond it (to an empty square or an enemy). Unlike the Xiangqi
    /// [`WideRole::Cannon`] (`mRcpR`) it has **no** non-jumping quiet slide.
    /// Promotes to a [`WideRole::PromotedRookCannon`]. (Cannon Shogi.) An
    /// **overflow-2 role**: its FEN token is `=A` (white) / `=a` (black); the
    /// harness maps `=a → a`.
    RookCannon = 60,
    /// Bishop-cannon (砲, FSF `c`, Betza `mBcpB`) — the diagonal Xiangqi cannon:
    /// slides quietly like a bishop and captures by leaping over exactly one
    /// diagonal screen onto the first piece beyond it. Promotes to a
    /// [`WideRole::PromotedBishopCannon`]. (Cannon Shogi.) An **overflow-2 role**:
    /// its FEN token is `=C` / `=c`; the harness maps `=c → c`.
    BishopCannon = 61,
    /// Bishop-hopper (砲, FSF `i`, Betza `pB`) — moves **and** captures only by
    /// leaping over exactly one diagonal screen, then sliding any distance beyond
    /// it. The diagonal analogue of the [`WideRole::RookCannon`]. Promotes to a
    /// [`WideRole::PromotedBishopHopper`]. (Cannon Shogi.) An **overflow-2 role**:
    /// its FEN token is `=I` / `=i`; the harness maps `=i → i`.
    BishopHopper = 62,
    /// Promoted Cannon (FSF `+U`, Betza `mRpRmFpB2`) — a promoted Xiangqi
    /// [`WideRole::Cannon`]: a full rook line (quiet slide **plus** unlimited
    /// over-screen hop, move and capture) together with a one-step diagonal quiet
    /// move and a range-2 diagonal hop (a screen one diagonal step away, landing
    /// two away). Reverts to a [`WideRole::Cannon`] in hand when captured. (Cannon
    /// Shogi.) An **overflow-2 role**: its FEN token is `=U` / `=u`; the harness
    /// maps `=u → +U`.
    PromotedCannon = 63,
    /// Promoted Rook-cannon (FSF `+A`) — moves identically to the
    /// [`WideRole::PromotedCannon`] (`mRpRmFpB2`) but reverts to a
    /// [`WideRole::RookCannon`] in hand when captured (its distinct base identity
    /// must survive promotion, exactly as FSF banks a captured `+A` as an `a`).
    /// (Cannon Shogi.) An **overflow-2 role**: its FEN token is `=W` / `=w`; the
    /// harness maps `=w → +A`.
    PromotedRookCannon = 64,
    /// Promoted Bishop-cannon (FSF `+C`, Betza `mBpBmWpR2`) — a promoted
    /// [`WideRole::BishopCannon`]: a full bishop line (quiet slide **plus**
    /// unlimited over-screen hop, move and capture) together with a one-step
    /// orthogonal quiet move and a range-2 orthogonal hop. Reverts to a
    /// [`WideRole::BishopCannon`] in hand when captured. (Cannon Shogi.) An
    /// **overflow-2 role**: its FEN token is `=F` / `=f`; the harness maps
    /// `=f → +C`.
    PromotedBishopCannon = 65,
    /// Promoted Bishop-hopper (FSF `+I`) — moves identically to the
    /// [`WideRole::PromotedBishopCannon`] (`mBpBmWpR2`) but reverts to a
    /// [`WideRole::BishopHopper`] in hand when captured. (Cannon Shogi.) An
    /// **overflow-2 role**: its FEN token is `=E` / `=e`; the harness maps
    /// `=e → +I`.
    PromotedBishopHopper = 66,

    // --- Mansindam (9x9 Korean "Pantheon tale") army (§ Milestone 10) ----
    //
    // Mansindam (Couch Tomato, https://www.pychess.org/variants/mansindam) is a
    // 9x9 shogi-chess hybrid on the [`Shogi9x9`](super::Shogi9x9) geometry: a
    // crazyhouse-style **captures-to-hand** with drops, a **mandatory** far-three-
    // ranks promotion zone, and a **campmate** flag win (a King reaching the
    // opponent's back rank). Confirmed square-for-square against Fairy-Stockfish
    // `UCI_Variant mansindam`. Most of its army reuses existing roles — the
    // Cardinal (Bishop + Knight) is the [`WideRole::Hawk`], the Marshal (Rook +
    // Knight) the [`WideRole::Elephant`], the promoted Guard (King-step) the
    // [`WideRole::Commoner`], the promoted Centaur (King + Knight) the
    // [`WideRole::Kheshig`], the promoted Archer (Bishop + Wazir) the
    // [`WideRole::DragonHorse`] (`+B`) and the promoted Tiger (Rook + Ferz) the
    // [`WideRole::Dragon`] (`+R`) — so only three pieces are genuinely new. The
    // single-`*` overflow bank is exhausted (every `a..=z` already names a `*`
    // base), so all three land in the **second** overflow bank, spelled with the
    // doubled prefix [`OVERFLOW_PREFIX`] (`**`) plus a recycled base letter whose
    // case carries the colour (see [`is_overflow2`](WideRole::is_overflow2) /
    // [`overflow2_from_base`](WideRole::overflow2_from_base)), exactly like the Sho
    // Shogi royals. The `compare-fairy` harness rewrites each token to FSF's
    // spelling when driving `UCI_Variant mansindam`.
    /// Angel (天, FSF `amazon`, letter `a`) — moves and captures like a **Queen +
    /// Knight** (rook + bishop slides plus the eight 2-1 leaps). Does not promote.
    /// (Mansindam.) A **second-bank overflow role**: its FEN token is the doubled
    /// prefix `**` plus the recycled Hawk letter `a` (FSF's amazon letter), `**A`
    /// (white) / `**a` (black); the `compare-fairy` harness maps `**a → a` when
    /// driving Mansindam.
    Angel = 67,
    /// Rhino (聖, FSF `customPiece1 = i:BNW`) — the promoted Cardinal: moves and
    /// captures like a **Bishop + Knight + Wazir** (bishop slides, the eight knight
    /// leaps, and one orthogonal step). Reverts to a Cardinal ([`WideRole::Hawk`])
    /// in hand when captured. (Mansindam.) A **second-bank overflow role**: its FEN
    /// token is the doubled prefix `**` plus the recycled Captain letter `i` (FSF's
    /// custom-piece letter `i`), `**I` (white) / `**i` (black); the `compare-fairy`
    /// harness maps `**i → +C` (FSF's promoted Cardinal) when driving Mansindam.
    Rhino = 68,
    /// Ship (名, FSF `customPiece2 = s:RNF`) — the promoted Marshal: moves and
    /// captures like a **Rook + Knight + Ferz** (rook slides, the eight knight
    /// leaps, and one diagonal step). Reverts to a Marshal
    /// ([`WideRole::Elephant`]) in hand when captured. (Mansindam.) A **second-bank
    /// overflow role**: its FEN token is the doubled prefix `**` plus the recycled
    /// Silver letter `s` (FSF's custom-piece letter `s`), `**S` (white) / `**s`
    /// (black); the `compare-fairy` harness maps `**s → +M` (FSF's promoted
    /// Marshal) when driving Mansindam.
    Ship = 69,
}

impl WideRole {
    /// The number of distinct roles, i.e. the length of [`WideRole::ALL`] and
    /// the size of a [`Board<G>`](super::Board)'s per-role mask array.
    ///
    /// This grows as fairy variants land and add roles.
    pub const COUNT: usize = 70;

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
        WideRole::JanggiElephant,
        WideRole::Lancer,
        WideRole::Kheshig,
        WideRole::Archer,
        WideRole::Commoner,
        WideRole::ShogiKnight,
        WideRole::Falcon,
        WideRole::Eagle,
        WideRole::Cardinal,
        WideRole::Tower,
        WideRole::Duke,
        WideRole::KnightBishop,
        WideRole::BishopKnight,
        WideRole::Banner,
        WideRole::Serpent,
        WideRole::Quetzal,
        WideRole::Shaman,
        WideRole::DivineLord,
        WideRole::ChakSoldier,
        WideRole::Temple,
        WideRole::Swallow,
        WideRole::Goose,
        WideRole::ToriFalcon,
        WideRole::ToriEagle,
        WideRole::Crane,
        WideRole::LeftQuail,
        WideRole::RightQuail,
        WideRole::Pheasant,
        WideRole::Alfil,
        WideRole::DrunkElephant,
        WideRole::CrownPrince,
        WideRole::RookCannon,
        WideRole::BishopCannon,
        WideRole::BishopHopper,
        WideRole::PromotedCannon,
        WideRole::PromotedRookCannon,
        WideRole::PromotedBishopCannon,
        WideRole::PromotedBishopHopper,
        WideRole::Angel,
        WideRole::Rhino,
        WideRole::Ship,
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
    /// the overflow roles return a recycled base letter (the board FEN I/O adds
    /// the `*` prefix). No role maps to the sentinel `'?'`.
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
            // Wazir (Dobutsu Giraffe) is an overflow role: its FEN token is the `*`
            // prefix plus the recycled base letter `j` (the Horse's, chosen clear of
            // the Tori Goose, which already recycles `g` as `*g`), so `char()` returns
            // the bare base letter and the board FEN I/O adds the `*` prefix (its
            // `w` was reclaimed by the Orda Kheshig). The `compare-fairy` harness
            // maps `*j` to FSF's `g` (the Giraffe) when driving Dobutsu.
            WideRole::Wazir => 'j',
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
            // Janggi elephant. FSF spells it `b` (the Bishop here) and the Xiangqi
            // elephant already took `o`, so it takes the free letter `x`; the
            // `compare-fairy` harness maps it to FSF's `b` when driving Janggi.
            WideRole::JanggiElephant => 'x',
            // Orda cavalry. FSF spells these `l h a` (kniroo, centaur, knibis),
            // but `l`, `h`, and `a` already name the Lance, Hoplite, and Hawk here;
            // the Orda pieces take the free letters `f` and `y` plus the `w`
            // reclaimed from the Wazir (now Dobutsu's `*j` Giraffe), and the
            // `compare-fairy` harness maps them to FSF's letters when driving Orda.
            // The Yurt is a plain Silver
            // (`s`) and needs no letter of its own.
            WideRole::Lancer => 'f',
            WideRole::Kheshig => 'w',
            WideRole::Archer => 'y',
            // Synochess commoner ("Advisor") — an overflow role past the exhausted
            // single-letter alphabet. Its FEN token is the `*` prefix plus the
            // recycled base letter `u` (the Advisor's), so `char()` returns the
            // bare base letter and the board FEN I/O adds the `*` prefix — exactly
            // as the Shogi promoted roles share a base letter under their `+`
            // prefix. The `compare-fairy` harness maps `*u` to FSF's `a` when
            // driving Synochess.
            WideRole::Commoner => 'u',
            // Shinobi's Shogi Knight — an overflow role past the exhausted
            // single-letter alphabet. Like the Commoner its FEN token is the `*`
            // prefix plus a recycled base letter (here `n`, the Knight's), so
            // `char()` returns the bare base letter and the board FEN I/O adds the
            // `*` prefix. The `compare-fairy` harness maps `*n` to FSF's `h` when
            // driving Shinobi.
            WideRole::ShogiKnight => 'n',
            // Ordamirror's Falcon — an overflow role past the exhausted
            // single-letter alphabet. Like the Commoner / Shogi Knight its FEN
            // token is the `*` prefix plus a recycled base letter (here `f`, the
            // FSF Falcon mnemonic, distinct from the bare Lancer `f` because of
            // the `*` prefix), so `char()` returns the bare base letter and the
            // board FEN I/O adds the `*` prefix. The `compare-fairy` harness maps
            // `*f` to FSF's `f` when driving Ordamirror.
            WideRole::Falcon => 'f',
            // Empire (Roman) army — four overflow roles past the exhausted
            // single-letter alphabet. Like the Commoner / Shogi Knight, each FEN
            // token is the `*` prefix plus a recycled base letter (the FSF
            // mnemonic `e`/`c`/`t`/`d`), so `char()` returns the bare base letter
            // and the board FEN I/O adds the `*` prefix. The `compare-fairy`
            // harness maps `*e → e`, `*c → c`, `*t → t`, `*d → d` when driving
            // Empire. (Each base letter is already a single-letter role here — the
            // Elephant `e`, Cannon `c`, Lieutenant `t`, General `d` — so the `*`
            // prefix is what distinguishes the Empire piece.)
            WideRole::Eagle => 'e',
            WideRole::Cardinal => 'c',
            WideRole::Tower => 't',
            WideRole::Duke => 'd',
            // Hoppel-Poppel move≠capture pieces — two overflow roles past the
            // exhausted single-letter alphabet. Like the Commoner / Empire pieces,
            // each FEN token is the `*` prefix plus a recycled base letter, so
            // `char()` returns the bare base letter and the board FEN I/O adds the
            // `*` prefix. The Knight-Bishop recycles `h` (the "Hoppel" mnemonic,
            // since the FSF letter `n` is already the ShogiKnight's base) and the
            // Bishop-Knight recycles the FSF `BISKNI` letter `b`. The `compare-fairy`
            // harness maps `*h → n`, `*b → b` when driving Hoppel-Poppel.
            WideRole::KnightBishop => 'h',
            WideRole::BishopKnight => 'b',
            // Manchu super-piece — an overflow role past the exhausted single-letter
            // alphabet. Like the Commoner / Empire pieces, its FEN token is the `*`
            // prefix plus a recycled base letter, so `char()` returns the bare base
            // letter and the board FEN I/O adds the `*` prefix. The Banner recycles
            // FSF's letter `m` (the bare Met's letter, distinguished by the `*`
            // prefix). The `compare-fairy` harness maps `*m → m` when driving Manchu.
            WideRole::Banner => 'm',
            // Chak (9x9 Mayan) army — six overflow roles past the exhausted
            // single-letter alphabet. Like the Commoner / Empire pieces, each FEN
            // token is the `*` prefix plus a recycled base letter, so `char()`
            // returns the bare base letter and the board FEN I/O adds the `*`
            // prefix. The Serpent / Quetzal / Soldier recycle the FSF mnemonics
            // `s` / `q` / `p`; the Shaman recycles the FSF letter `w` (the
            // Kheshig's bare letter, distinct by the `*` prefix); the Divine Lord
            // takes `l` (the FSF letter `d` being already the General's recycled
            // base); the Temple recycles the FSF letter `o` (the Xiangqi
            // Elephant's bare letter, distinct by the `*` prefix). The
            // `compare-fairy` harness maps `*s → s`, `*q → q`, `*w → w`,
            // `*l → d`, `*p → p`, `*o → o` when driving Chak.
            WideRole::Serpent => 's',
            WideRole::Quetzal => 'q',
            WideRole::Shaman => 'w',
            WideRole::DivineLord => 'l',
            WideRole::ChakSoldier => 'p',
            WideRole::Temple => 'o',
            // Tori Shogi birds — overflow roles past the exhausted single-letter
            // alphabet. Like the Commoner / Empire pieces, each FEN token is the
            // `*` prefix plus a distinct recycled base letter, so `char()` returns
            // the bare base letter and the board FEN I/O adds the `*` prefix. The
            // bases (`y`/`g`/`a`/`i`/`k`/`v`/`r`/`z`) are distinct from every other
            // overflow base — the Chak army already claims `s`, `o`, `l` and `p`,
            // so the swallow / crane / left-quail / pheasant take `y` / `k` / `v` /
            // `z` instead; the `compare-fairy` harness rewrites each to FSF's
            // letter (`*y → s`, `*g → +S`, `*a → f`, `*i → +F`, `*k → c`, `*v → l`,
            // `*r → r`, `*z → p`) when driving Tori Shogi.
            WideRole::Swallow => 'y',
            WideRole::Goose => 'g',
            WideRole::ToriFalcon => 'a',
            WideRole::ToriEagle => 'i',
            WideRole::Crane => 'k',
            WideRole::LeftQuail => 'v',
            WideRole::RightQuail => 'r',
            WideRole::Pheasant => 'z',
            // Shatranj Alfil (elephant) — an overflow role past the exhausted
            // single-letter alphabet. Like the Commoner / Empire pieces, its FEN
            // token is the `*` prefix plus a recycled base letter, so `char()`
            // returns the bare base letter and the board FEN I/O adds the `*`
            // prefix. FSF spells it `b` (the Bishop here), so the Alfil recycles the
            // one free overflow base `x` (the Janggi Elephant's bare letter, distinct
            // by the `*` prefix). The `compare-fairy` harness maps `*x → b` when
            // driving Shatranj.
            WideRole::Alfil => 'x',
            // Sho Shogi royals — **second-bank** overflow roles past the exhausted
            // single-`*` alphabet. Each FEN token is the doubled prefix `**` plus a
            // recycled base letter (returned here), the board FEN I/O adding the
            // prefix. The Drunk Elephant recycles the Elephant's `e` (FSF's Drunk
            // Elephant letter); the Crown Prince recycles the Cannon's `c`
            // ("Crown"). The `compare-fairy` harness maps `**e → e`, `**c → +E`
            // when driving Sho Shogi.
            WideRole::DrunkElephant => 'e',
            WideRole::CrownPrince => 'c',
            // Mansindam new pieces — **second-bank** overflow roles. Each FEN token
            // is the doubled prefix `**` plus a recycled base letter (returned
            // here), the board FEN I/O adding the prefix. The Angel recycles the
            // Hawk's `a` (FSF's amazon letter), the Rhino the Captain's `i` (FSF's
            // custom-piece `i`), the Ship the Silver's `s` (FSF's custom-piece `s`).
            // The `compare-fairy` harness maps `**a → a`, `**i → +C`, `**s → +M`
            // when driving Mansindam.
            WideRole::Angel => 'a',
            WideRole::Rhino => 'i',
            WideRole::Ship => 's',
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
            // Cannon Shogi cannon army — overflow-3 roles past the exhausted
            // single-letter alphabet, the exhausted `*`-overflow bases and the
            // doubled-`**` second tier (the Sho Shogi royals). Each FEN token is the
            // `=` ([`OVERFLOW_PREFIX_3`]) prefix plus a
            // recycled base letter, so `char()` returns the bare base letter and the
            // board FEN I/O adds the prefix. The bases recycle FSF's mnemonics
            // (`a`/`c`/`i` for the three new movers; `u`/`w`/`f`/`e` for the four
            // promoted forms, distinct from one another within the `=` tier). The
            // `compare-fairy` harness maps `=a → a`, `=c → c`, `=i → i`,
            // `=u → +U`, `=w → +A`, `=f → +C`, `=e → +I` when driving Cannon Shogi.
            WideRole::RookCannon => 'a',
            WideRole::BishopCannon => 'c',
            WideRole::BishopHopper => 'i',
            WideRole::PromotedCannon => 'u',
            WideRole::PromotedRookCannon => 'w',
            WideRole::PromotedBishopCannon => 'f',
            WideRole::PromotedBishopHopper => 'e',
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

    /// Returns `true` if this is an **overflow** role — a fairy role added after
    /// the single-letter FEN alphabet (`a..=z`) was exhausted. Like a Shogi
    /// promoted role it has **no bare letter of its own**: its FEN token is the
    /// [`OVERFLOW_PREFIX`] (`*`) followed by a recycled base letter (returned by
    /// [`char`](WideRole::char)) whose **case carries the colour**, and the board
    /// FEN parser / writer handle the prefix (see [`overflow_base_char`] and
    /// [`overflow_from_base`]).
    ///
    /// [`overflow_base_char`]: WideRole::overflow_base_char
    /// [`overflow_from_base`]: WideRole::overflow_from_base
    #[must_use]
    #[inline]
    pub const fn is_overflow(self) -> bool {
        matches!(
            self,
            WideRole::Wazir
                | WideRole::Commoner
                | WideRole::ShogiKnight
                | WideRole::Falcon
                | WideRole::Eagle
                | WideRole::Cardinal
                | WideRole::Tower
                | WideRole::Duke
                | WideRole::KnightBishop
                | WideRole::BishopKnight
                | WideRole::Banner
                | WideRole::Serpent
                | WideRole::Quetzal
                | WideRole::Shaman
                | WideRole::DivineLord
                | WideRole::ChakSoldier
                | WideRole::Temple
                | WideRole::Swallow
                | WideRole::Goose
                | WideRole::ToriFalcon
                | WideRole::ToriEagle
                | WideRole::Crane
                | WideRole::LeftQuail
                | WideRole::RightQuail
                | WideRole::Pheasant
                | WideRole::Alfil
        )
    }

    /// For an overflow role, the **recycled base letter** its FEN token reuses
    /// (the same value [`char`](WideRole::char) returns); for any other role,
    /// `None`. The full token is [`OVERFLOW_PREFIX`] + this letter, the letter's
    /// case encoding the colour.
    #[must_use]
    #[inline]
    pub const fn overflow_base_char(self) -> Option<char> {
        if self.is_overflow() {
            Some(self.char())
        } else {
            None
        }
    }

    /// Maps a recycled base letter (after an [`OVERFLOW_PREFIX`]) back to its
    /// overflow role, returning `None` if the letter does not name one. The
    /// inverse of [`overflow_base_char`](WideRole::overflow_base_char); used by
    /// the board FEN parser when it sees a `*`-prefixed token. Accepts either
    /// case (the case carries colour, handled by the caller).
    #[must_use]
    #[inline]
    pub const fn overflow_from_base(ch: char) -> Option<WideRole> {
        match ch.to_ascii_lowercase() {
            // Dobutsu Giraffe: recycles the Horse's letter `j` (distinct by `*`,
            // chosen clear of the Tori Goose which already recycles `g`).
            'j' => Some(WideRole::Wazir),
            'u' => Some(WideRole::Commoner),
            'n' => Some(WideRole::ShogiKnight),
            'f' => Some(WideRole::Falcon),
            // Empire (Roman) army: recycled FSF mnemonics `e`/`c`/`t`/`d`.
            'e' => Some(WideRole::Eagle),
            'c' => Some(WideRole::Cardinal),
            't' => Some(WideRole::Tower),
            'd' => Some(WideRole::Duke),
            // Hoppel-Poppel move≠capture pieces: the Knight-Bishop recycles `h`
            // (the "Hoppel" mnemonic), the Bishop-Knight the FSF `BISKNI` letter `b`.
            'h' => Some(WideRole::KnightBishop),
            'b' => Some(WideRole::BishopKnight),
            // Manchu super-piece: recycles FSF's Banner letter `m`.
            'm' => Some(WideRole::Banner),
            // Chak (9x9 Mayan) army: recycled FSF mnemonics `s`/`q`/`p`, the
            // Kheshig's letter `w` (Shaman), the "Lord" letter `l` (Divine Lord),
            // and the Xiangqi Elephant's letter `o` (Temple).
            's' => Some(WideRole::Serpent),
            'q' => Some(WideRole::Quetzal),
            'w' => Some(WideRole::Shaman),
            'l' => Some(WideRole::DivineLord),
            'p' => Some(WideRole::ChakSoldier),
            'o' => Some(WideRole::Temple),
            // Tori Shogi birds — distinct recycled bases chosen clear of every
            // other overflow role (the Chak army already claims `s`/`o`/`l`/`p`, so
            // the swallow / crane / left-quail / pheasant take `y`/`k`/`v`/`z`); the
            // `compare-fairy` harness rewrites each `*<base>` to FSF's spelling.
            'y' => Some(WideRole::Swallow),
            'g' => Some(WideRole::Goose),
            'a' => Some(WideRole::ToriFalcon),
            'i' => Some(WideRole::ToriEagle),
            'k' => Some(WideRole::Crane),
            'v' => Some(WideRole::LeftQuail),
            'r' => Some(WideRole::RightQuail),
            'z' => Some(WideRole::Pheasant),
            // Shatranj Alfil (elephant): recycles the one free overflow base `x`
            // (the Janggi Elephant's bare letter, distinct by the `*` prefix); the
            // harness maps `*x → b` (FSF's Alfil letter) when driving Shatranj.
            'x' => Some(WideRole::Alfil),
            _ => None,
        }
    }

    /// Returns `true` if this is a **second-bank** overflow role — a fairy role
    /// added after *both* the single-letter FEN alphabet (`a..=z`) and the first
    /// `*<letter>` overflow bank were exhausted. Like a single-bank overflow role
    /// it has no bare letter; its FEN token is the [`OVERFLOW_PREFIX`] **doubled**
    /// (`**`) followed by a recycled base letter (returned by
    /// [`char`](WideRole::char)) whose case carries the colour, and the board FEN
    /// parser / writer handle the doubled prefix. The two Sho Shogi royals (the
    /// Drunk Elephant and its promoted Crown Prince) are the first such roles.
    #[must_use]
    #[inline]
    pub const fn is_overflow2(self) -> bool {
        matches!(
            self,
            WideRole::DrunkElephant
                | WideRole::CrownPrince
                | WideRole::Angel
                | WideRole::Rhino
                | WideRole::Ship
        )
    }

    /// Maps a recycled base letter (after a doubled [`OVERFLOW_PREFIX`], `**`) back
    /// to its second-bank overflow role, returning `None` if the letter does not
    /// name one. The inverse of [`char`](WideRole::char) for the second bank; used
    /// by the board FEN parser when it sees a `**`-prefixed token. Accepts either
    /// case (the case carries colour, handled by the caller).
    #[must_use]
    #[inline]
    pub const fn overflow2_from_base(ch: char) -> Option<WideRole> {
        match ch.to_ascii_lowercase() {
            // Drunk Elephant: recycles the Elephant's letter `e` (FSF's Drunk
            // Elephant letter, distinct by the `**` prefix).
            'e' => Some(WideRole::DrunkElephant),
            // Crown Prince: recycles the Cannon's letter `c` ("Crown").
            'c' => Some(WideRole::CrownPrince),
            // Mansindam: the Angel recycles the Hawk's `a` (FSF's amazon letter),
            // the Rhino the Captain's `i`, the Ship the Silver's `s`. All distinct
            // from the Sho Shogi royals' `e` / `c`.
            'a' => Some(WideRole::Angel),
            'i' => Some(WideRole::Rhino),
            's' => Some(WideRole::Ship),
            _ => None,
        }
    }

    /// Returns `true` if this is a **third-tier overflow** role — a fairy role added
    /// after the single-letter alphabet, every `*`-prefixed [`OVERFLOW_PREFIX`] base
    /// **and** the doubled-`**` second tier ([`is_overflow2`]) were all in play (the
    /// Cannon Shogi cannon army, whose recycled letters `c` / `e` would clash with
    /// the Sho Shogi `**` royals). Like the lower tiers it has **no bare letter of
    /// its own**: its FEN token is the [`OVERFLOW_PREFIX_3`] (`=`) followed by a
    /// recycled base letter (returned by [`char`](WideRole::char)) whose case
    /// carries the colour, and the board FEN parser / writer handle the prefix (see
    /// [`overflow3_from_base`](WideRole::overflow3_from_base)).
    ///
    /// [`is_overflow2`]: WideRole::is_overflow2
    #[must_use]
    #[inline]
    pub const fn is_overflow3(self) -> bool {
        matches!(
            self,
            WideRole::RookCannon
                | WideRole::BishopCannon
                | WideRole::BishopHopper
                | WideRole::PromotedCannon
                | WideRole::PromotedRookCannon
                | WideRole::PromotedBishopCannon
                | WideRole::PromotedBishopHopper
        )
    }

    /// Maps a recycled base letter (after an [`OVERFLOW_PREFIX_3`]) back to its
    /// third-tier overflow role, returning `None` if the letter does not name one.
    /// The inverse of [`char`](WideRole::char) for an [`is_overflow3`] role; used by
    /// the board FEN parser when it sees a `=`-prefixed token. Accepts either case
    /// (the case carries colour, handled by the caller).
    ///
    /// [`is_overflow3`]: WideRole::is_overflow3
    #[must_use]
    #[inline]
    pub const fn overflow3_from_base(ch: char) -> Option<WideRole> {
        match ch.to_ascii_lowercase() {
            // Cannon Shogi cannon army: the three new movers recycle FSF's
            // mnemonics `a` / `c` / `i`; the four promoted forms recycle distinct
            // letters `u` / `w` / `f` / `e` within the `=` tier.
            'a' => Some(WideRole::RookCannon),
            'c' => Some(WideRole::BishopCannon),
            'i' => Some(WideRole::BishopHopper),
            'u' => Some(WideRole::PromotedCannon),
            'w' => Some(WideRole::PromotedRookCannon),
            'f' => Some(WideRole::PromotedBishopCannon),
            'e' => Some(WideRole::PromotedBishopHopper),
            _ => None,
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
            // 'w' is the Orda Kheshig (reclaimed from the Wazir, now `*j`).
            'w' => Some(WideRole::Kheshig),
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
            'x' => Some(WideRole::JanggiElephant),
            'f' => Some(WideRole::Lancer),
            'y' => Some(WideRole::Archer),
            // The Commoner and Shinobi's Shogi Knight have no bare single letter:
            // they are overflow roles whose FEN tokens are `*u` / `*n` (see
            // `is_overflow` / `overflow_from_base`). Their base letters `u` / `n`
            // deliberately still parse to the Advisor / Knight here; the board FEN
            // parser resolves the `*` prefix to the overflow role.
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
            WideRole::JanggiElephant => "janggi-elephant",
            WideRole::Lancer => "lancer",
            WideRole::Kheshig => "kheshig",
            WideRole::Archer => "archer",
            WideRole::Commoner => "commoner",
            WideRole::ShogiKnight => "shogi-knight",
            WideRole::Falcon => "falcon",
            WideRole::Eagle => "eagle",
            WideRole::Cardinal => "cardinal",
            WideRole::Tower => "tower",
            WideRole::Duke => "duke",
            WideRole::KnightBishop => "knight-bishop",
            WideRole::BishopKnight => "bishop-knight",
            WideRole::Banner => "banner",
            WideRole::Serpent => "serpent",
            WideRole::Quetzal => "quetzal",
            WideRole::Shaman => "shaman",
            WideRole::DivineLord => "divine-lord",
            WideRole::ChakSoldier => "chak-soldier",
            WideRole::Temple => "temple",
            WideRole::Swallow => "swallow",
            WideRole::Goose => "goose",
            WideRole::ToriFalcon => "tori-falcon",
            WideRole::ToriEagle => "tori-eagle",
            WideRole::Crane => "crane",
            WideRole::LeftQuail => "left-quail",
            WideRole::RightQuail => "right-quail",
            WideRole::Pheasant => "pheasant",
            WideRole::Alfil => "alfil",
            WideRole::DrunkElephant => "drunk-elephant",
            WideRole::CrownPrince => "crown-prince",
            WideRole::RookCannon => "rook-cannon",
            WideRole::BishopCannon => "bishop-cannon",
            WideRole::BishopHopper => "bishop-hopper",
            WideRole::PromotedCannon => "promoted-cannon",
            WideRole::PromotedRookCannon => "promoted-rook-cannon",
            WideRole::PromotedBishopCannon => "promoted-bishop-cannon",
            WideRole::PromotedBishopHopper => "promoted-bishop-hopper",
            WideRole::Angel => "angel",
            WideRole::Rhino => "rhino",
            WideRole::Ship => "ship",
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
            // The Shogi promoted roles share a base letter (handled by the `+`
            // FEN prefix) and the overflow roles (including the Wazir / Dobutsu
            // Giraffe, whose `w` was reclaimed by the Orda Kheshig) share a recycled
            // base letter (handled by the `*` prefix); all are excluded from the
            // bare-letter round-trip.
            if role.is_promoted()
                || role.is_overflow()
                || role.is_overflow2()
                || role.is_overflow3()
            {
                continue;
            }
            let ch = role.char();
            assert_ne!(ch, '?', "every fielded role has a letter");
            assert_eq!(WideRole::from_char(ch), Some(role));
            assert_eq!(WideRole::from_char(role.upper_char()), Some(role));
            assert_eq!(role.char().to_ascii_uppercase(), role.upper_char());
        }
        // The Wazir (Dobutsu Giraffe) is an overflow role: its bare letter `j`
        // parses to the Horse (its recycled base role), and the `*j` token resolves
        // to the Wazir via `overflow_from_base`.
        assert_eq!(WideRole::Wazir.char(), 'j');
        assert_eq!(WideRole::from_char('j'), Some(WideRole::Horse));
        assert_eq!(WideRole::overflow_from_base('j'), Some(WideRole::Wazir));
        assert_eq!(WideRole::from_char('?'), None);
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
        // Every non-promoted, non-overflow role names a distinct letter. The Shogi
        // promoted roles reuse their base role's letter (FEN `+`-prefix) and the
        // overflow roles reuse a recycled base letter (FEN `*`-prefix), so both are
        // excluded from the distinctness check.
        let chars: Vec<char> = WideRole::ALL
            .into_iter()
            .filter(|r| {
                !r.is_promoted() && !r.is_overflow() && !r.is_overflow2() && !r.is_overflow3()
            })
            .map(WideRole::char)
            .filter(|&c| c != '?')
            .collect();
        let mut sorted = chars.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), chars.len(), "role chars must be distinct");
    }

    #[test]
    fn overflow_roles_round_trip_through_the_prefix_token() {
        // An overflow role has no bare letter: its `char()` is a recycled base
        // letter that still parses to the base role, while `overflow_from_base`
        // maps that base letter back to the overflow role (what the board FEN
        // parser does after a `*` prefix). The Commoner recycles the Advisor's `u`.
        for role in WideRole::ALL.into_iter().filter(|r| r.is_overflow()) {
            let base = role.overflow_base_char().expect("overflow role has a base");
            assert_eq!(role.char(), base);
            assert_ne!(base, '?', "overflow base letter is real");
            // The bare base letter parses to the *base* role, not the overflow one.
            assert_ne!(WideRole::from_char(base), Some(role));
            // The prefix-resolver maps the base letter (either case) to the role.
            assert_eq!(WideRole::overflow_from_base(base), Some(role));
            assert_eq!(
                WideRole::overflow_from_base(base.to_ascii_uppercase()),
                Some(role)
            );
        }
        // The Commoner is the original overflow role, spelled `*u`; the Wazir
        // (Dobutsu Giraffe) is spelled `*j`.
        assert!(WideRole::Commoner.is_overflow());
        assert_eq!(WideRole::Commoner.char(), 'u');
        assert_eq!(WideRole::overflow_from_base('u'), Some(WideRole::Commoner));
        assert!(WideRole::Wazir.is_overflow());
        assert_eq!(WideRole::overflow_from_base('j'), Some(WideRole::Wazir));
        // The Shatranj Alfil recycles the last free overflow base `x` (the Janggi
        // Elephant's bare letter, distinct by the `*` prefix).
        assert_eq!(WideRole::overflow_from_base('x'), Some(WideRole::Alfil));
        // A character that names no overflow role yields `None`.
        assert_eq!(WideRole::overflow_from_base('?'), None);
    }

    #[test]
    fn second_bank_overflow_roles_round_trip_through_the_doubled_prefix() {
        // The Sho Shogi royals are second-bank overflow roles (`is_overflow2`):
        // they have no bare letter and are *not* single-`*` overflow roles, so their
        // FEN token is the doubled prefix `**` plus a recycled base letter resolved
        // by `overflow2_from_base`.
        for role in WideRole::ALL.into_iter().filter(|r| r.is_overflow2()) {
            assert!(!role.is_overflow(), "a second-bank role is not single-bank");
            assert!(
                !role.is_promoted(),
                "a second-bank role is not `+`-promoted"
            );
            let base = role.char();
            assert_ne!(base, '?', "second-bank base letter is real");
            assert_eq!(WideRole::overflow2_from_base(base), Some(role));
            assert_eq!(
                WideRole::overflow2_from_base(base.to_ascii_uppercase()),
                Some(role)
            );
        }
        // The Drunk Elephant recycles the Elephant's `e`; the Crown Prince the
        // Cannon's `c` ("Crown"). Both are distinct from every single-`*` base.
        assert_eq!(WideRole::DrunkElephant.char(), 'e');
        assert_eq!(
            WideRole::overflow2_from_base('e'),
            Some(WideRole::DrunkElephant)
        );
        assert_eq!(WideRole::CrownPrince.char(), 'c');
        assert_eq!(
            WideRole::overflow2_from_base('c'),
            Some(WideRole::CrownPrince)
        );
        // A character that names no second-bank role yields `None`.
        assert_eq!(WideRole::overflow2_from_base('z'), None);
    }

    #[test]
    fn overflow3_roles_round_trip_through_the_prefix_token() {
        // A third-tier overflow role (the Cannon Shogi cannon army) has no bare
        // letter: its `char()` is a recycled base letter, and `overflow3_from_base`
        // maps that base letter back to the role (what the board FEN parser does
        // after a `=` prefix). None of them is a first-tier `*` or second-tier `**`
        // overflow role.
        for role in WideRole::ALL.into_iter().filter(|r| r.is_overflow3()) {
            assert!(!role.is_overflow());
            assert!(!role.is_overflow2());
            assert!(!role.is_promoted());
            let base = role.char();
            assert_ne!(base, '?', "overflow-3 base letter is real");
            assert_eq!(WideRole::overflow3_from_base(base), Some(role));
            assert_eq!(
                WideRole::overflow3_from_base(base.to_ascii_uppercase()),
                Some(role)
            );
        }
        // The three new movers recycle FSF's `a` / `c` / `i`; the promoted Cannon
        // recycles `u`.
        assert_eq!(WideRole::RookCannon.char(), 'a');
        assert_eq!(
            WideRole::overflow3_from_base('a'),
            Some(WideRole::RookCannon)
        );
        assert_eq!(
            WideRole::overflow3_from_base('u'),
            Some(WideRole::PromotedCannon)
        );
        // A character that names no third-tier overflow role yields `None`.
        assert_eq!(WideRole::overflow3_from_base('?'), None);
        assert_eq!(WideRole::overflow3_from_base('z'), None);
    }
}
