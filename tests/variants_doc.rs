//! Generator + drift-check for `docs/variants.md`, the human-readable variant
//! reference.
//!
//! The reference table is generated **from the registries** so it cannot drift
//! from the code: the display/canonical names come from
//! [`VariantId`]/[`WideVariantId`], the board size from
//! [`AnyWideVariant::dimensions`], and the start FEN from the position's
//! `to_fen()`. Only the prose that is not derivable — notable pieces, special
//! rules, and the validation oracle — is hand-authored in the [`Meta`] tables
//! below, keyed by variant through an **exhaustive** match, so adding a variant
//! to the registry without documenting it fails to compile.
//!
//! Two entry points share one renderer:
//!
//! * [`render`] builds the whole markdown document as a `String`.
//! * The [`variants_doc_is_up_to_date`] test asserts the committed
//!   `docs/variants.md` equals a freshly rendered document (the golden-file /
//!   `insta` pattern, done with `std` only). Regenerate the committed file with
//!   `REGEN=1 cargo test --test variants_doc` (or set `BLESS=1`).

use mcr::geometry::{AnyWideVariant, WideVariantId};
use mcr::{AnyVariant, VariantId};

/// The hand-authored, non-derivable columns for one variant.
struct Meta {
    /// Human display name (e.g. `"Xiangqi (Chinese chess)"`).
    display: &'static str,
    /// Notable / added pieces beyond the standard army, or the army of a
    /// non-chess game. One line, no `|`.
    pieces: &'static str,
    /// Distinguishing rules: drops, gating, promotion, win condition, castling,
    /// counting endgame. One line, no `|`.
    rules: &'static str,
    /// Validation oracle and parity status. One line, no `|`.
    oracle: &'static str,
}

/// Path to the committed reference document.
fn doc_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/variants.md")
}

/// Escapes a cell for a GitHub-flavoured markdown table: the only structural
/// character is the pipe, which no FEN or authored string contains, but we guard
/// it anyway so a stray pipe cannot silently corrupt the table.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Renders one markdown table row for a variant.
fn row(display: &str, canonical: &str, board: &str, fen: &str, meta: &Meta) -> String {
    format!(
        "| {} | `{}` | {} | `{}` | {} | {} | {} |\n",
        cell(display),
        cell(canonical),
        cell(board),
        cell(fen),
        cell(meta.pieces),
        cell(meta.rules),
        cell(meta.oracle),
    )
}

const TABLE_HEADER: &str = "| Variant | Canonical | Board | Start FEN | Notable pieces | Special rules | Validation oracle |\n|---|---|---|---|---|---|---|\n";

/// Builds the whole `docs/variants.md` document from the registries.
fn render() -> String {
    let mut out = String::new();
    out.push_str("<!-- GENERATED FILE — do not edit by hand. -->\n");
    out.push_str(
        "<!-- Regenerate with: REGEN=1 cargo test --test variants_doc (see tests/variants_doc.rs). -->\n\n",
    );
    out.push_str("# mcr variant reference\n\n");
    out.push_str(
        "Every variant mcr registers, generated straight from the code so it stays in \
sync. The **display name**, **canonical name**, **board size**, and **start FEN** \
columns are pulled programmatically from the registries \
(`VariantId`/`WideVariantId`, `AnyWideVariant::dimensions`, and each position's \
`to_fen()`); the **pieces**, **special rules**, and **validation oracle** columns \
are hand-authored in `tests/variants_doc.rs` and kept honest by an exhaustive \
per-variant match. A drift-check test regenerates this file and asserts it equals \
the committed copy, so it can never fall behind the code.\n\n",
    );
    out.push_str(
        "Board size is `files`x`ranks`. Start FENs are in mcr's own piece dialect; \
where that differs from Fairy-Stockfish's spelling the `compare-fairy/` harness \
reconciles the two (see each variant's module docs).\n\n",
    );

    // -- Concrete 8x8 engine ------------------------------------------------
    out.push_str("## Concrete 8x8 engine\n\n");
    out.push_str(&format!(
        "The frozen, hand-tuned 8x8 engine reached through `mcr::AnyVariant` / \
`mcr::VariantId` — **{}** variants.\n\n",
        VariantId::ALL.len()
    ));
    out.push_str(TABLE_HEADER);
    for &id in VariantId::ALL {
        let pos = AnyVariant::startpos(id);
        let meta = concrete_meta(id);
        row_into(
            &mut out,
            meta.display,
            id.as_str(),
            "8x8",
            &pos.to_fen(),
            &meta,
        );
    }
    out.push('\n');

    // -- Fairy / geometry-layer variants ------------------------------------
    out.push_str("## Fairy / geometry-layer variants\n\n");
    out.push_str(&format!(
        "The generic geometry engine reached through `mcr::geometry::AnyWideVariant` / \
`WideVariantId` — **{}** variants, spanning 3x4 Dobutsu to 12x8 Courier and \
10x10 Opulent / Ten-Cubed.\n\n",
        WideVariantId::ALL.len()
    ));
    out.push_str(TABLE_HEADER);
    for &id in WideVariantId::ALL {
        let pos = AnyWideVariant::startpos(id);
        let (w, h) = pos.dimensions();
        let meta = wide_meta(id);
        row_into(
            &mut out,
            meta.display,
            id.as_str(),
            &format!("{w}x{h}"),
            &pos.to_fen(),
            &meta,
        );
    }

    out
}

/// Appends a rendered row to `out` (helper so [`render`] reads cleanly).
fn row_into(out: &mut String, display: &str, canonical: &str, board: &str, fen: &str, meta: &Meta) {
    out.push_str(&row(display, canonical, board, fen, meta));
}

/// Hand-authored metadata for each concrete 8x8 variant. Exhaustive: a new
/// `VariantId` will not compile until it is documented here.
fn concrete_meta(id: VariantId) -> Meta {
    match id {
        VariantId::Standard => Meta {
            display: "Standard chess",
            pieces: "The standard chess army: pawn, knight, bishop, rook, queen, king.",
            rules: "FIDE rules: double pawn push, en passant, kingside/queenside castling, last-rank promotion to N/B/R/Q; win by checkmate.",
            oracle: "Reference perft (published node counts); the 8x8 baseline the whole engine is pinned against.",
        },
        VariantId::Chess960 => Meta {
            display: "Chess960 (Fischer random)",
            pieces: "Standard chess army; the back rank is one of 960 shuffles (bishops on opposite colours, king between the rooks).",
            rules: "As standard chess but castling generalizes to arbitrary king/rook start files (standard g/c and f/d destinations); Shredder/X-FEN castling field.",
            oracle: "Reference Chess960 perft; identical movement to standard chess apart from castling.",
        },
        VariantId::Atomic => Meta {
            display: "Atomic chess",
            pieces: "Standard chess army.",
            rules: "Every capture detonates a 3x3 blast (pawns immune) centred on the capturer's square; win by exploding the enemy king, so a king may never capture.",
            oracle: "Published atomic perft; validated against Fairy-Stockfish `UCI_Variant atomic`.",
        },
        VariantId::Antichess => Meta {
            display: "Antichess (Giveaway / Losing chess)",
            pieces: "Standard chess army; the king is a non-royal ordinary piece.",
            rules: "Captures are forced; no check/checkmate/castling; pawns may also promote to king; a side with no legal move (zero pieces or stalemate) WINS.",
            oracle: "Published antichess perft; validated against Fairy-Stockfish `UCI_Variant giveaway`.",
        },
        VariantId::Crazyhouse => Meta {
            display: "Crazyhouse",
            pieces: "Standard chess army plus a pocket of captured pieces (flipped to the captor's colour).",
            rules: "Captured pieces may be dropped onto any empty square (no pawn drop on rank 1/8); promoted pieces revert to pawns when captured; drop-mate allowed; FEN carries a `[...]` pocket and `~` promoted marks.",
            oracle: "Published crazyhouse perft; validated against Fairy-Stockfish `UCI_Variant crazyhouse`.",
        },
        VariantId::KingOfTheHill => Meta {
            display: "King of the Hill",
            pieces: "Standard chess army.",
            rules: "Standard chess plus an immediate win when a king reaches a central hill square (d4, e4, d5, or e5); move generation is identical to standard chess.",
            oracle: "Move set identical to standard chess (bulk-countable); validated against Fairy-Stockfish `UCI_Variant kingofthehill`.",
        },
        VariantId::ThreeCheck => Meta {
            display: "Three-check",
            pieces: "Standard chess army.",
            rules: "Standard chess plus a win for the first side to give check three times; FEN carries a remaining-checks field `W+B` (starts `3+3`).",
            oracle: "Move set identical to standard chess; validated against Fairy-Stockfish `UCI_Variant 3check`.",
        },
        VariantId::RacingKings => Meta {
            display: "Racing Kings",
            pieces: "Both armies (queens, rooks, bishops, knights, kings) with no pawns.",
            rules: "Pawnless, castle-less race to reach rank 8; no move may leave OR give check; if White reaches rank 8 and Black can too on the reply it is a draw, else White wins.",
            oracle: "Reference Racing Kings perft; validated against Fairy-Stockfish `UCI_Variant racingkings`.",
        },
        VariantId::Horde => Meta {
            display: "Horde",
            pieces: "Black: full standard army. White: 36 pawns and no king.",
            rules: "Kingless White (never in check) versus a royal Black; White pawns may sit and double-push from rank 1 (no en passant target); only Black castles; Black wins by capturing all White material, White by checkmate.",
            oracle: "Reference Horde perft; validated against Fairy-Stockfish `UCI_Variant horde`.",
        },
    }
}

/// Hand-authored metadata for each fairy / geometry-layer variant. Exhaustive:
/// a new `WideVariantId` will not compile until it is documented here.
fn wide_meta(id: WideVariantId) -> Meta {
    match id {
        WideVariantId::Aiwok => Meta {
            display: "Ai-Wok",
            pieces: "Makruk army with the Met replaced by an Ai-Wok (Rook + Knight + Ferz super-piece): Rook, Knight, Khon (silver), Ai-Wok, King, single-step promote-to-Ai-Wok pawns.",
            rules: "Makruk with the Met upgraded to the Ai-Wok (rook slides, knight leaps, and one diagonal step); pawns promote to an Ai-Wok. No castling; counting endgame. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant ai-wok`).",
        },
        WideVariantId::Alice => Meta {
            display: "Alice chess",
            pieces: "Standard chess army on two mirror 8x8 boards (A and B).",
            rules: "Each move is legal on the current board, then the piece transfers to the same square on the other board (must be empty there); no en passant. Win by checkmate.",
            oracle: "Rules-only (no FSF perft oracle): FSF has no alice variant; pinned by hand-derived perft and a brute-force cross-check.",
        },
        WideVariantId::Almost => Meta {
            display: "Almost Chess (8x8)",
            pieces: "Standard chess army with the Queen replaced by a Chancellor (Rook + Knight, mcr Elephant).",
            rules: "Standard 8x8 chess with castling, double step, and en passant; with no Queen a pawn promotes to Chancellor, Rook, Bishop, or Knight. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant almost`).",
        },
        WideVariantId::Amazon => Meta {
            display: "Amazon Chess (8x8)",
            pieces: "Standard chess army with the Queen replaced by an Amazon (Queen + Knight, mcr Angel).",
            rules: "Standard 8x8 chess with castling, double step, and en passant; with no Queen a pawn promotes to Amazon, Rook, Bishop, or Knight. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant amazon`).",
        },
        WideVariantId::Asean => Meta {
            display: "ASEAN chess (modern Makruk)",
            pieces: "Makruk army: Rook, Knight, Khon (silver, forward + diagonals), Met (ferz), King, and single-step pawns.",
            rules: "FIDE-style symmetric setup, no castling; pawns promote on the last rank to Met, Rook, Silver, or Knight; a pieces-honour counting endgame. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant asean`).",
        },
        WideVariantId::Berolina => Meta {
            display: "Berolina chess",
            pieces: "Standard chess army with an inverted Berolina pawn (`p`/`P`).",
            rules: "Standard chess, but the pawn is the mirror of the ordinary pawn: it moves one square diagonally forward (two along the diagonal from its start rank, a lame jump blocked by an occupied intervening square) and captures one square straight forward. En passant applies to the diagonal double step; promotion is standard (Q/R/B/N). Castling, check, and the fifty-move rule are standard. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant berolina`).",
        },
        WideVariantId::Bughouse => Meta {
            display: "Bughouse (single-board rules)",
            pieces: "Standard chess army plus an externally-fed drop hand (pocket).",
            rules: "Standard 8x8 chess plus crazyhouse-style drops (a pawn only on ranks 2-7), but captures do NOT bank the piece (it crosses to the partner board); the 2-board team linkage is server-side.",
            oracle: "Fairy-Stockfish (`UCI_Variant bughouse`): single-board perft-matches FSF; the 2-board team linkage is out of scope.",
        },
        WideVariantId::Cambodian => Meta {
            display: "Cambodian chess (Ouk Chaktrang)",
            pieces: "Makruk army: Rook, Knight, Khon (silver), Met (ferz), King, single-step promote-to-Met pawns.",
            rules: "Makruk plus a one-time first-move leap for the King (forward-knight jump) and the Met (two-square advance), tracked like castling rights; counting endgame. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant cambodian`).",
        },
        WideVariantId::CannonShogi => Meta {
            display: "Cannon Shogi (Ohzutsu Shogi, 9x9)",
            pieces: "Shogi army with the Pawn replaced by a Soldier (forward/sideways) plus five cannon-type pieces (Cannon, Rook-cannon, Bishop-cannon, Bishop-hopper) that also drop from hand.",
            rules: "9x9 Shogi geometry with a hand and drops and far-three-ranks promotion; the cannon pieces slide quietly or capture by hopping one screen. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant cannonshogi`).",
        },
        WideVariantId::Capablanca => Meta {
            display: "Capablanca chess (10x8)",
            pieces: "Standard army plus an Archbishop (Bishop + Knight) and a Chancellor (Rook + Knight).",
            rules: "Ten-file board, standard pawns and en passant; pawns promote to Queen, Rook, Bishop, Knight, Archbishop, or Chancellor; castling on the Capablanca files (king f to i/c). Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant capablanca`).",
        },
        WideVariantId::Capahouse => Meta {
            display: "Capahouse (10x8)",
            pieces: "Capablanca army (Archbishop, Chancellor) plus a crazyhouse drop hand.",
            rules: "Capablanca chess with captures banking to hand for later drops (a pawn only on ranks 2-7, promoted pieces demote to Pawn); castling on the Capablanca files. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant capahouse`).",
        },
        WideVariantId::Caparandom => Meta {
            display: "Capablanca-Random (10x8)",
            pieces: "Capablanca army (Archbishop, Chancellor) shuffled on the back rank.",
            rules: "Chess960-style shuffled Capablanca setup with Shredder-notation castling to the fixed Capablanca destinations (king i/c); standard pawns and promotion. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant caparandom`).",
        },
        WideVariantId::Centaur => Meta {
            display: "Centaur Chess (10x8)",
            pieces: "Standard army with the Archbishop/Chancellor replaced by two Centaurs (King + Knight, mcr Kheshig).",
            rules: "Capablanca board and castle geometry (king f to i/c); standard pawns, en passant, and last-rank promotion to Queen, Rook, Bishop, Knight, or Centaur. Win by checkmate.",
            oracle: "Fairy-Stockfish (INI `centaur`).",
        },
        WideVariantId::Chak => Meta {
            display: "Chak (9x9 Mayan chess)",
            pieces: "Rook, Vulture (knight), Jaguar (King + Knight), Serpent, Quetzal (eight-direction cannon), King, Soldier, Divine Lord, Shaman, and an immobile Temple.",
            rules: "King and Soldier mandatorily promote (to Divine Lord / Shaman) on reaching their own half; a royal Divine Lord reaching the enemy temple square wins; losing both King and Lord loses.",
            oracle: "Fairy-Stockfish (`UCI_Variant chak`).",
        },
        WideVariantId::Chancellor => Meta {
            display: "Chancellor chess (9x9)",
            pieces: "Standard army plus a Chancellor (Rook + Knight) per side.",
            rules: "Standard chess on a 9x9 board with castling on the standard files, double step, and en passant; pawns promote to Queen, Rook, Bishop, Knight, or Chancellor. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant chancellor`).",
        },
        WideVariantId::CheckShogi => Meta {
            display: "Checkshogi (Check Shogi, 9x9)",
            pieces: "The standard 9x9 Shogi army: King, Rook (Dragon), Bishop (Horse), Gold and Silver Generals, Knight, Lance, Pawn, and their promoted forms.",
            rules: "Standard 9x9 Shogi — hand, drops, far-three-ranks promotion, nifu and dead-piece drop rules — except that giving check wins the game outright (a checked side has no reply). Win by delivering check (or by checkmate/stalemate).",
            oracle: "Fairy-Stockfish (`UCI_Variant checkshogi`).",
        },
        WideVariantId::Chennis => Meta {
            display: "Chennis (7x7 flipping variant)",
            pieces: "Four flipping pairs — Pawn/Rook, Ferz/Cannon, Soldier/Bishop, Commoner/Knight — plus a region-confined King.",
            rules: "7x7 board where each non-royal piece flips to its alternate form on every move; captures go to hand and drop in either form; the King is confined to a 5x4 mobility region. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant chennis`).",
        },
        WideVariantId::Chigorin => Meta {
            display: "Chigorin Chess (8x8)",
            pieces: "Asymmetric: a White knight army (Knights + a Chancellor, no bishops or queen) vs a Black bishop army (Bishops + a Queen, no knights).",
            rules: "Standard 8x8 chess with standard castling; colour-restricted promotion — White to Chancellor/Rook/Knight, Black to Queen/Rook/Bishop. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant chigorin`).",
        },
        WideVariantId::Chu => Meta {
            display: "Chu Shogi (12x12)",
            pieces: "The 21-type Chu Shogi army: King, Free King, Lion, Dragon King/Horse, Kirin, Phoenix, Side/Vertical Mover, Copper/Silver/Gold/Ferocious Leopard/Blind Tiger/Drunk Elephant, Lance, Reverse Chariot, Go-Between, Pawn, and their promoted forms.",
            rules: "No hand or drops; mandatory promotion on entering the far four ranks (HaChu's promote-on-entry); two royals (King and the promoted Prince). The Lion and lion-power promoted pieces (Horned Falcon, Soaring Eagle) have their full move set: igui, double capture, two-step area move, and jitto pass. Lion-trading restrictions are not enforced (HaChu does not enforce them either).",
            oracle: "HaChu (H. G. Muller) external move-list tree-walk: start-position perft(1)=36 (byte-identical move set) and perft(2)=1296 match node-for-node; perft(3) mcr=48319 vs HaChu=48317, agreeing at every node but one where HaChu 0.23 misses two legal anti-diagonal Lion captures (a HaChu bug; mcr is correct).",
        },
        WideVariantId::Coregal => Meta {
            display: "Coregal chess (8x8)",
            pieces: "Standard chess army.",
            rules: "Standard 8x8 chess in which the queen is royal as well as the king — a side loses if either its king or its queen is checkmated (or captured), so the queen may not move onto, or be left on, an attacked square. Castling, double step, en passant, and promotion to Queen/Rook/Bishop/Knight are standard; every queen (including a promoted one) is royal. Win by checkmate of either royal.",
            oracle: "Fairy-Stockfish (`UCI_Variant coregal`).",
        },
        WideVariantId::Courier => Meta {
            display: "Courier chess (12x8)",
            pieces: "Rook, Knight, Bishop, King plus the medieval Courier (Alfil, two-square diagonal leap), Man (non-royal king-mover), Wazir, and Ferz.",
            rules: "Twelve-file board, no castling, non-standard array with advanced pawns and a Ferz; single-step pawns promote only to Ferz; bare-king loss and stalemate-is-loss. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant courier`).",
        },
        WideVariantId::Dai => Meta {
            display: "Dai Shogi (15x15)",
            pieces: "Chu Shogi widened to 15x15: the whole Chu army (King, Free King, Lion, Dragon King/Horse, Kirin, Phoenix, the ranging movers, the generals, Drunk Elephant/Prince, Lance, Reverse Chariot, Go-Between, Pawn and their promoted forms) plus five short-range movers — Violent Ox (range-2 rook), Flying Dragon (range-2 bishop), Evil Wolf, Iron General, Stone General — and the reused Angry Boar (Wazir), Cat Sword (Ferz) and forward Knight.",
            rules: "No hand or drops; mandatory promotion on entering the far **five** ranks (HaChu's promote-on-entry). Unlike Chu, the Kirin, Phoenix and Gold do not promote; every weak piece promotes to Gold, the rest as in Chu. Two royals (King and the promoted Prince). The Lion and the lion-power promoted pieces keep their full move set (igui, double capture, area move, jitto pass).",
            oracle: "HaChu (H. G. Muller) external move-list tree-walk: start-position perft(1)=71 (node-for-node identical move set, pinning the Kirin/Phoenix chirality) and perft(2)=5041 match node-for-node; perft(3)=357836 validated at the subtree/leaf level with zero real mismatches (a full depth-3 node-by-node walk is intractable; a few nodes are unreachable due to HaChu 0.23 segfaults).",
        },
        WideVariantId::Dobutsu => Meta {
            display: "Dobutsu (Animal Shogi, 3x4)",
            pieces: "Lion (non-royal king-stepper), Giraffe (wazir), Elephant (ferz), and a Chick (pawn) that promotes to a Hen (gold mover).",
            rules: "3x4 board with a hand and unrestricted drops; the Chick force-promotes on the far rank; lose when your Lion is captured, or win by moving your Lion safely onto the far rank (try).",
            oracle: "Fairy-Stockfish (`UCI_Variant dobutsu`).",
        },
        WideVariantId::Dragon => Meta {
            display: "Dragon chess (8x8)",
            pieces: "Standard chess army plus one Dragon (a Bishop + Knight compound) held in a fixed pocket.",
            rules: "Standard 8x8 chess; the pocketed Dragon may be dropped onto an empty square of the player's own back rank; pawns promote to N/B/R/Q or a Dragon. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant dragon`).",
        },
        WideVariantId::Duck => Meta {
            display: "Duck chess (8x8)",
            pieces: "Standard chess army plus a neutral Duck blocker belonging to neither side.",
            rules: "Each ply is a normal move then relocating the Duck (a universal blocker) to a different empty square; the king is non-royal (no check) and the game is won by capturing the enemy king.",
            oracle: "Fairy-Stockfish (`UCI_Variant duck`).",
        },
        WideVariantId::Embassy => Meta {
            display: "Embassy Chess (10x8)",
            pieces: "Capablanca army: a Chancellor (Rook + Knight) and an Archbishop (Bishop + Knight).",
            rules: "Capablanca board with the king on the e-file and its own castle files (king e to h/b); standard pawns, en passant, and six-target last-rank promotion. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant embassy`).",
        },
        WideVariantId::Empire => Meta {
            display: "Empire (8x8)",
            pieces: "Standard Black army vs a White Empire: Eagle, Cardinal, Tower, Duke (each moves like a Queen, captures on a short pattern) plus Soldiers.",
            rules: "Asymmetric; only Black castles; a pawn of either side promotes only to a Queen; flag-win when a king reaches the far rank, broad flying-general, and stalemate-is-loss.",
            oracle: "Fairy-Stockfish (`UCI_Variant empire`).",
        },
        WideVariantId::EuroShogi => Meta {
            display: "EuroShogi (European Shogi, 8x8)",
            pieces: "King, Gold General, Rook (Dragon), Bishop (Horse), Pawn, and a modified Knight — the two Shogi forward 2-1 jumps plus one step straight sideways. No Silver General and no Lance.",
            rules: "Shogi on the 8x8 board with a hand and drops; promotion in the far three ranks is compulsory. The sideways-stepping Knight is never immobile, so it may be dropped anywhere; the Pawn keeps the last-rank and nifu drop restrictions. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant euroshogi`).",
        },
        WideVariantId::Extinction => Meta {
            display: "Extinction chess (8x8)",
            pieces: "Standard chess army with a non-royal Commoner king.",
            rules: "Standard chess movement, castling, en passant, and promotion but no check or checkmate — the king is an ordinary capturable Commoner. A side loses the instant any one of its piece types is wiped out (Pawn, Knight, Bishop, Rook, Queen, or king reaches zero), so capturing the last enemy queen — or promoting your own last pawn — decides the game. Rides the generic extinction terminal (all army types, threshold 0).",
            oracle: "Fairy-Stockfish (`UCI_Variant extinction`).",
        },
        WideVariantId::FogOfWar => Meta {
            display: "Fog of War (Dark Chess, 8x8)",
            pieces: "Standard chess army with a non-royal king.",
            rules: "Standard chess movement, castling, en passant, and promotion but no check: a side may leave its king attacked, and capturing the enemy king wins; the per-player fog is a view layer only.",
            oracle: "Fairy-Stockfish (`UCI_Variant fogofwar`): the deterministic movegen perft-matches FSF; the fog is a view layer, not part of perft.",
        },
        WideVariantId::Gorogoro => Meta {
            display: "Gorogoro Shogi Plus (5x6)",
            pieces: "Shogi minus Rook and Bishop — King, Gold and Silver Generals, Knight, Lance, Pawn — with a Lance and Knight starting in hand.",
            rules: "5x6 Shogi with a hand and drops and a two-rank promotion zone (optional, forced when otherwise immobile); nifu and dead-piece drop rules, no uchifuzume. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant gorogoroplus`).",
        },
        WideVariantId::Gothic => Meta {
            display: "Gothic Chess (10x8)",
            pieces: "Capablanca army: a Chancellor (Rook + Knight) and an Archbishop (Bishop + Knight).",
            rules: "Capablanca board and castle geometry (king on the f-file to i/c) with a different back-rank order; standard pawns, en passant, and six-target last-rank promotion. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant gothic`).",
        },
        WideVariantId::Grand => Meta {
            display: "Grand chess (10x10)",
            pieces: "Standard army plus a Marshal (Rook + Knight) and a Cardinal (Bishop + Knight).",
            rules: "10x10 board, no castling, pawns double-push from rank 3/8; a three-rank promotion zone, promoting only to an already-captured piece type. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant grand`).",
        },
        WideVariantId::Grandhouse => Meta {
            display: "Grandhouse (10x10)",
            pieces: "Grand army (Marshal, Cardinal) plus a crazyhouse drop hand.",
            rules: "Grand chess with captures banking to hand for later drops (promoted pieces demote to Pawn; a pawn not on its back rank or promotion zone); no castling; three-rank promotion zone. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant grandhouse`).",
        },
        WideVariantId::HoppelPoppel => Meta {
            display: "Hoppel-Poppel (8x8)",
            pieces: "Standard army where the Knight captures like a bishop (moves as a knight) and the Bishop captures like a knight (moves as a bishop).",
            rules: "Standard 8x8 chess with castling, double step, and en passant; a pawn promotes to Queen, Rook, or one of the two swapped-capture pieces (never an ordinary bishop or knight). Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant hoppelpoppel`).",
        },
        WideVariantId::Janggi => Meta {
            display: "Janggi (Korean chess, 9x10)",
            pieces: "General, Guards, Chariots, Cannons (jump one screen), hobbled Horses, long-leaping Elephants, and Soldiers; no river, palace with diagonal lines.",
            rules: "Palace-confined General and Guards moving along palace diagonals; a side may pass (not while in check); bikjang restricts the generals from sliding while facing on an open line. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant janggi`).",
        },
        WideVariantId::Janus => Meta {
            display: "Janus Chess (10x8)",
            pieces: "Capablanca board with two Januses (Bishop + Knight) per side and no Chancellor.",
            rules: "King on the e-file with its own castle files (king e to i/b); standard pawns and en passant; pawns promote to Queen, Rook, Bishop, Knight, or Janus. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant janus`).",
        },
        WideVariantId::Jieqi => Meta {
            display: "Jieqi (hidden Xiangqi, 9x10)",
            pieces: "Xiangqi army, but every piece except the two Generals starts face-down as a Dark piece, revealing its identity on its first move.",
            rules: "Standard Xiangqi geometry, palace, river, and flying-general; a dark piece moves as the Xiangqi piece native to its home square and reveals (from a hidden pool) the instant it moves.",
            oracle: "Rules-validated (not an FSF variant, hidden identities); the deterministic all-dark core perft-matches FSF `UCI_Variant xiangqi`.",
        },
        WideVariantId::Judkins => Meta {
            display: "Judkins Shogi (6x6 Shogi)",
            pieces: "One each of King, Rook, Bishop, Gold and Silver Generals, Knight, and Pawn — no Lance.",
            rules: "6x6 Shogi with a hand and drops and a two-rank promotion zone; nifu and dead-piece drop rules (Pawn last rank, Knight last two ranks), no uchifuzume. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant judkins`).",
        },
        WideVariantId::Karouk => Meta {
            display: "Ka Ouk (Kar Ouk)",
            pieces: "Makruk army: Rook, Knight, Khon (silver), Met (ferz), King, single-step promote-to-Met pawns.",
            rules: "Cambodian chess (Makruk plus the one-time King and Met first-move leaps and counting endgame) except that giving check wins the game outright (a checked side has no reply). Win by delivering check (or by checkmate).",
            oracle: "Fairy-Stockfish (`UCI_Variant karouk`).",
        },
        WideVariantId::Khans => Meta {
            display: "Khan's Chess (8x8)",
            pieces: "Standard White army vs a Black Khan (Orda-family) army: Lancer, Kheshig, Archer, Khan, and Khan soldiers (move like a knight, capture differently).",
            rules: "Asymmetric; only White castles; Khan soldiers force-promote to a Khan on the first rank; flag-win when a king reaches the far rank; stalemate-is-loss. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant khans`).",
        },
        WideVariantId::Kinglet => Meta {
            display: "Kinglet chess (8x8)",
            pieces: "Standard chess army with a non-royal Commoner king.",
            rules: "Standard chess movement, castling, and en passant but no check or checkmate — the king is an ordinary capturable Commoner. Pawns promote only to a (non-royal) Commoner/King, never to Queen, Rook, Bishop, or Knight. A side loses the instant it has no pawns left, so capturing the enemy's last pawn — or promoting your own last pawn — decides the game. Rides the generic extinction terminal (watching only the Pawn type, threshold 0).",
            oracle: "Fairy-Stockfish (`UCI_Variant kinglet`).",
        },
        WideVariantId::Knightmate => Meta {
            display: "Knightmate (8x8)",
            pieces: "A royal Knight on the king's square, with the two knights replaced by non-royal Commoners (Manns).",
            rules: "Standard chess but the royal piece moves and checks as a Knight; standard castling; pawns promote to Commoner, Bishop, Rook, or Queen (never a Knight). Win by checkmating the royal Knight.",
            oracle: "Fairy-Stockfish (`UCI_Variant knightmate`).",
        },
        WideVariantId::Kyotoshogi => Meta {
            display: "Kyoto Shogi (5x5 flipping Shogi)",
            pieces: "Four flipping pairs — Pawn/Rook, Silver/Bishop, Lance/Gold, Knight/Gold — plus a King that never flips.",
            rules: "5x5 Shogi where every piece flips to its alternate form after each move; captures go to hand in base form and drop in either form; no drop restrictions. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant kyotoshogi`).",
        },
        WideVariantId::Makpong => Meta {
            display: "Makpong (Defensive Chess)",
            pieces: "Makruk army: Rook, Knight, Khon (silver), Met (ferz), King, single-step promote-to-Met pawns.",
            rules: "Makruk with one change: while in check the king may not flee, only capture the lone checker (else block or capture the checker with another piece); no castling; counting endgame. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant makpong`).",
        },
        WideVariantId::Makruk => Meta {
            display: "Makruk (Thai chess)",
            pieces: "Rua (rook), Ma (knight), Khon (silver, forward + diagonals), Met (ferz), Khun (king), and single-step Bia pawns.",
            rules: "8x8 with no castling; a pawn promotes to a Met on the sixth rank; a board-honour and pieces-honour counting endgame. Win by checkmate; stalemate draws.",
            oracle: "Fairy-Stockfish (`UCI_Variant makruk`).",
        },
        WideVariantId::Manchu => Meta {
            display: "Manchu (yipaisanxianqi, 9x10)",
            pieces: "A full Xiangqi army for one side; the other replaces its rook/cannon/horse cluster with a single Banner super-piece (Chariot + Cannon + Horse).",
            rules: "Asymmetric Xiangqi sharing the palace, river-bound Elephant, hobbled Horse, over-screen Cannon, river-crossing Soldier, and flying-general. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant manchu`).",
        },
        WideVariantId::Mansindam => Meta {
            display: "Mansindam (Pantheon tale, 9x9)",
            pieces: "Shogi-chess hybrid army: Pawn, Knight, Bishop, Rook, Cardinal, Marshal, Queen, Angel, King, promoting to stronger forms (Guard, Centaur, Archer, Tiger, Rhino, Ship).",
            rules: "9x9 crazyhouse with captures-to-hand and drops (nifu, no pawn on the last rank); mandatory far-three-ranks promotion; campmate when a King reaches the enemy back rank. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant mansindam`).",
        },
        WideVariantId::Micro => Meta {
            display: "Micro Shogi (4x5 Shogi)",
            pieces: "One each of King, Rook, Bishop, Lance, and Pawn, with the Rook and Lance starting pre-promoted; promoted forms +R and +B move as Gold, +L as Silver, and +P as a Knight.",
            rules: "4x5 Shogi with a hand and drops (dual-form, no nifu or dead-piece restrictions); no promotion zone — a piece flips form only on a capture (a base piece promotes, a promoted piece demotes). Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant micro`).",
        },
        WideVariantId::Minishogi => Meta {
            display: "Minishogi (5x5 Shogi)",
            pieces: "One each of King, Rook, Bishop, Gold and Silver Generals, and Pawn — no Knight or Lance.",
            rules: "5x5 Shogi with a hand and drops and a single-rank promotion zone; nifu and dead-piece drop rules, no uchifuzume. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant minishogi`).",
        },
        WideVariantId::Minixiangqi => Meta {
            display: "Minixiangqi (7x7)",
            pieces: "General, Horse (hobbled), Chariot, Cannon (over-screen), and Soldier — no advisors or elephants.",
            rules: "7x7 Xiangqi with a palace but no river (soldiers step sideways from the start) and the flying-general rule. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant minixiangqi`).",
        },
        WideVariantId::Modern => Meta {
            display: "Modern chess (9x9)",
            pieces: "Standard chess army plus an Archbishop (Bishop + Knight compound) on each side's back rank.",
            rules: "Standard 8x8 chess widened to a 9x9 board with an added Archbishop; standard castling (king on the e-file, rooks on the a/i files), double step, en passant, and promotion to Queen/Rook/Bishop/Knight/Archbishop. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant modern`).",
        },
        WideVariantId::Nocastle => Meta {
            display: "No-castle chess (8x8)",
            pieces: "Standard chess army.",
            rules: "Standard 8x8 chess with castling disabled — neither side may ever castle; double step, en passant, and promotion to Queen/Rook/Bishop/Knight are standard. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant nocastle`).",
        },
        WideVariantId::Opulent => Meta {
            display: "Opulent chess (10x10)",
            pieces: "Standard sliders plus an augmented Knight (Knight + Wazir), Chancellor, Archbishop, Wizard (Camel + Ferz), and Lion (Ferz + Dabbaba + Threeleaper).",
            rules: "10x10 board, no castling; pawns double-push from rank 3/8; a three-rank promotion zone, promoting only to an already-captured piece type. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant opulent`).",
        },
        WideVariantId::Orda => Meta {
            display: "Orda (8x8)",
            pieces: "Standard White army vs a Black Orda cavalry: Lancer, Kheshig, Archer (move like a knight, capture along a slider line) and Yurt (silver).",
            rules: "Asymmetric; only White castles; a pawn of either side promotes to a Queen or Kheshig; flag-win (campmate) when a king reaches the far rank. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant orda`).",
        },
        WideVariantId::Ordamirror => Meta {
            display: "Ordamirror (8x8)",
            pieces: "Both armies are Orda horde: Lancer, Kheshig, Archer, and Falcon (moves like a queen, captures like a knight), plus a King.",
            rules: "Symmetric horde mirror; no castling, single-step pawns; a pawn promotes to Lancer, Kheshig, Archer, or Falcon; flag-win (campmate) when a king reaches the far rank. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant ordamirror`).",
        },
        WideVariantId::Pawnback => Meta {
            display: "Pawn back chess",
            pieces: "Standard chess army with a pawn that may also step backward (`p`/`P`).",
            rules: "Standard chess, but the pawn may also make a single quiet step straight backward (same file); it still captures diagonally forward, double-steps forward from the second rank, and promotes on the last rank. A pawn may never retreat onto its own first rank (White ranks 2-8, Black ranks 1-7), so a home-rank pawn cannot step back. En passant is standard (only off the forward double step). Because pawns can move backward, a pawn move does NOT reset the fifty-move clock (only captures and promotions do), so pawn shuffling can reach the fifty-move draw. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant pawnback`).",
        },
        WideVariantId::Pawnsideways => Meta {
            display: "Pawn-sideways chess (8x8)",
            pieces: "Standard chess army with a pawn that may also step sideways (`p`/`P`).",
            rules: "Standard 8x8 chess in which a pawn, in addition to its ordinary moves, may make a single quiet step sideways (one square left or right along its own rank) onto an empty square. The forward push, initial forward double step, diagonal capture, en passant (off the forward double step only), and promotion to Queen/Rook/Bishop/Knight are all standard; a sideways step never captures, never promotes, gives no check, and creates no en-passant target. Castling and the fifty-move rule are standard. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant pawnsideways`).",
        },
        WideVariantId::Placement => Meta {
            display: "Placement (Pre-Chess, 8x8)",
            pieces: "Standard chess army; the eight non-pawn pieces start off the board, in hand.",
            rules: "A deployment phase drops the back-rank pieces onto the first rank (bishops on opposite colours), conferring castling rights, then normal chess (castling, en passant, promotion) follows. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant placement`).",
        },
        WideVariantId::Pocketknight => Meta {
            display: "Pocket Knight chess (8x8)",
            pieces: "Standard chess army plus one extra Knight in hand per side.",
            rules: "Standard 8x8 chess (castling, double step, en passant, promotion to Queen/Rook/Bishop/Knight) with one Knight in each side's pocket, droppable onto any empty square as a move; captures are not banked, so the pocket is a one-shot reserve. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant pocketknight`).",
        },
        WideVariantId::Seirawan => Meta {
            display: "Seirawan chess (S-Chess, 8x8)",
            pieces: "Standard chess army plus a reserve Hawk (Bishop + Knight) and Elephant (Rook + Knight), one of each per side.",
            rules: "Standard 8x8 chess; when a back-rank piece first moves the player may gate a reserve onto the vacated square (castling gates on the king's or the rook's square). Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant seirawan`).",
        },
        WideVariantId::Shako => Meta {
            display: "Shako (10x10)",
            pieces: "Full standard army plus a Cannon (over-screen) and an Elephant (Fers-Alfil leaper), with cannons in the corners.",
            rules: "10x10 chess; castling on rank 2/9 (king f to h/d); pawns double-push, take en passant, and promote on the last rank to Q/R/B/N, Cannon, or Elephant. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shako`).",
        },
        WideVariantId::Shatar => Meta {
            display: "Shatar (Mongolian chess, 8x8)",
            pieces: "Standard Rook, Knight, Bishop, and King, with the Queen replaced by a Bers (Rook slide plus one diagonal step).",
            rules: "No castling; single-step pawns (centre pawns pre-advanced) promote only to a Bers; the Robado bare-king rule draws immediately and truncates perft. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shatar`).",
        },
        WideVariantId::Shatranj => Meta {
            display: "Shatranj (medieval chess, 8x8)",
            pieces: "Rukh (rook), Faras (knight), Pil (Alfil, two-square diagonal leap), Farzin (ferz), Shah (king), and single-step pawns.",
            rules: "No double push, en passant, or castling; a pawn promotes only to a Ferz; bare-king loss and stalemate-is-loss terminal rules. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shatranj`).",
        },
        WideVariantId::Shinobi => Meta {
            display: "Shinobi (8x8)",
            pieces: "Standard Black army vs a White clan: Lances, Shogi Knights, a Commoner, a Bers, an Archbishop, and Fers, with a fixed reserve hand.",
            rules: "Only Black castles; White drops its fixed reserve onto its own half; clan pieces mandatorily promote into standard pieces in the far two ranks; flag-win when a king reaches the opponent's back rank. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shinobi`).",
        },
        WideVariantId::Shogi => Meta {
            display: "Shogi (Japanese chess, 9x9)",
            pieces: "King, Rook (to Dragon), Bishop (to Horse), Gold and Silver Generals, Knight, Lance, and Pawn, with gold-moving promoted minors.",
            rules: "9x9 with a persistent hand: captures flip and bank for later drops (nifu, dead-piece, and uchifuzume rules); optional far-three-ranks promotion, forced when otherwise immobile. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shogi`).",
        },
        WideVariantId::Shogun => Meta {
            display: "Shogun (8x8)",
            pieces: "Standard chess army; each base piece promotes to a single stronger compound (Commoner, Centaur, Archbishop, Chancellor, Queen), capped at one of each.",
            rules: "8x8 crazyhouse (captures-to-hand, drops in a rank 1-5/4-8 region, no nifu) with a shogi-style far-three-ranks promotion zone, optional and capped. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shogun`).",
        },
        WideVariantId::ShoShogi => Meta {
            display: "Sho Shogi (old 9x9 Shogi, no drops)",
            pieces: "Shogi army plus a Drunk Elephant (seven king-steps, no straight back) that promotes to a Crown Prince, a second royal piece.",
            rules: "9x9 Shogi movement and promotions but captures are removed, not pocketed (no hand or drops); two royals — neither is royal while a side holds both. Win by capturing the sole royal.",
            oracle: "Fairy-Stockfish (`UCI_Variant shoshogi`).",
        },
        WideVariantId::Shouse => Meta {
            display: "S-House (Seirawan-house, 8x8)",
            pieces: "Standard army plus a reserve Hawk (Bishop + Knight) and Elephant (Rook + Knight) held in a shared crazyhouse hand.",
            rules: "Seirawan gating composed with crazyhouse drops on one hand: captures bank (promoted pieces demote) and any held piece may be dropped or gated; pawns promote to six targets. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant shouse`).",
        },
        WideVariantId::Sittuyin => Meta {
            display: "Sittuyin (Burmese chess, 8x8)",
            pieces: "Makruk army: Yathay (rook), Myin (knight), Sin (silver), Sit-ke/Met (ferz), Min Gyi (king), single-step Nè pawns.",
            rules: "A setup phase places the eight non-pawn pieces onto own territory (rooks on the back rank), then no castling; a Met-driven special pawn promotion in place or by a ferz step. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant sittuyin`).",
        },
        WideVariantId::Spartan => Meta {
            display: "Spartan chess (8x8)",
            pieces: "Standard White army vs Black Spartans: Lieutenant, General, Captain, Warlord (Archbishop), two Kings, and Berolina Hoplite pawns.",
            rules: "Asymmetric; only White castles; Black has two kings under duple check (in check only when all kings are attacked at once); Hoplites promote to Spartan pieces or a King. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant spartan`).",
        },
        WideVariantId::Synochess => Meta {
            display: "Synochess (8x8)",
            pieces: "Standard White Kingdom vs a Black Dynasty: Chariot, Horse (free knight), Elephant (Fers-Alfil), Advisor (commoner), Cannon (Janggi-style), and Soldiers (two in hand).",
            rules: "Asymmetric; only White castles; Black drops Soldiers onto rank 5; campmate when a king reaches the far rank, broad flying-general, and stalemate-is-loss. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant synochess`).",
        },
        WideVariantId::Tencubed => Meta {
            display: "Ten-Cubed chess (10x10)",
            pieces: "Standard army plus a Marshal (Rook + Knight), Archbishop (Bishop + Knight), Wizard (Camel + Ferz), and Champion (Wazir + Alfil + Dabbaba).",
            rules: "10x10 board, no castling; pawns double-push from rank 3/8, take en passant, and promote on the last rank only to a Queen, Marshal, or Archbishop (unrestricted). Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant tencubed`).",
        },
        WideVariantId::Tenjiku => Meta {
            display: "Tenjiku Shogi (16x16)",
            pieces: "The ~36-type Tenjiku army: the whole Chu army plus the jump-capturing Great/Vice/Rook/Bishop Generals, the area-burning Fire Demon, the Lion Hawk (Lion + Bishop), Free Eagle, Water Buffalo, Chariot Soldier, Heavenly Tetrarch, Vertical/Side Soldier, Multi-General, Dog, and the shared Iron General and Knight. Uses HaChu's exact `variant tenjiku` start layout, including its hand-written White/Black asymmetries.",
            rules: "No hand or drops; five-rank promotion zone with Tenjiku-specific promotions (Soaring Eagle→Rook General, Lion→Lion Hawk, Free King→Free Eagle, …); two royals (King and the promoted Prince). **Honest partial:** ordinary movement of every piece is modelled and validated; the Fire Demon's multi-square **area burn** and the four Generals' **jump-capture** are documented-unmodelled (they capture as ordinary blockable sliders); the Lion / Lion Hawk keep the full igui / double-capture / pass move set.",
            oracle: "HaChu 0.23 **crashes deterministically** on `variant tenjiku` (its 16x16 board leaves no EDGE-sentinel border), so no live oracle is available. mcr's start position is instead reconciled **move-for-move against HaChu's own source tables** (`tenjikuPieces` / `tenArray` / `GenNonCapts`): start-position perft(1)=72 node-for-node; perft(2)=5662 and perft(3)=424195 are mcr regression pins (faithful to the rules at these depths — no special power is reachable — but not HaChu-cross-checked).",
        },
        WideVariantId::Threekings => Meta {
            display: "Three kings chess (8x8)",
            pieces: "Standard chess army minus the rooks, with three non-royal Commoner kings per side (on files a, e, and h).",
            rules: "Standard chess movement, en passant, and standard Q/R/B/N promotion but no check or checkmate — each king is an ordinary capturable Commoner and each side fields three of them. A side loses the instant its king count drops to two, so losing any one of its three kings decides the game. No castling (the start array has no rooks). Rides the generic extinction terminal (the king type, threshold 2).",
            oracle: "Fairy-Stockfish (`UCI_Variant threekings`).",
        },
        WideVariantId::Tori => Meta {
            display: "Tori Shogi (bird shogi, 7x7)",
            pieces: "A seven-bird army: Swallow (to Goose), Falcon (to Eagle), Crane, Left and Right Quail, Pheasant, and King.",
            rules: "7x7 Shogi with a hand and drops (up to two Swallows per file); mandatory two-rank promotion for the Swallow and Falcon only. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant torishogi`).",
        },
        WideVariantId::Torpedo => Meta {
            display: "Torpedo chess (8x8)",
            pieces: "Standard chess army.",
            rules: "Standard 8x8 chess in which a pawn may make its two-square advance from any rank (not only its starting rank), whenever both squares ahead are empty. Single pushes, diagonal captures, en passant (off a double-step from any rank), promotion to Queen/Rook/Bishop/Knight, and castling are all standard. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant torpedo`).",
        },
        WideVariantId::Washogi => Meta {
            display: "Wa Shogi (animal shogi, 11x11)",
            pieces: "An animal-and-bird army of thirty-one kinds: the royal Crane King plus sixteen non-royal base pieces (Sparrow Pawn, Oxcart, Liberated Horse, Strutting Crow, Swooping Owl, Climbing Monkey, Flying Goose, Flying Cock, Blind Dog, Violent Stag, Violent Wolf, Swallow's Wings, Running Rabbit, Flying Falcon, and the never-promoting Treacherous Fox and Cloud Eagle) and fourteen distinct promoted forms (Golden Bird, Plodding Ox, Heavenly Horse, Flying Falcon, Cloud Eagle, Violent Stag, Swallow's Wings, Raiding Falcon, Violent Wolf, Gliding Swallow, Treacherous Fox, Tenacious Falcon, Roaming Boar, Bear's Eyes).",
            rules: "11x11 Shogi-family game with a persistent hand and FSF-style captures-to-hand drops; captures bank unpromoted and flip sides, a dropped piece is always unpromoted, and a Sparrow Pawn or Oxcart may not be dropped on the last rank. Optional promotion for a move starting or ending in the furthest three ranks (forced for a Sparrow Pawn or Oxcart on the last rank); the Treacherous Fox, Cloud Eagle and Crane King never promote. Win by capturing the Crane King.",
            oracle: "Rules-only (no FSF perft oracle): Fairy-Stockfish has no Wa Shogi variant and HaChu's perft is unreliable, so it is rules-validated (as for Alice / Fog-of-War / Bughouse) via hand-derived low-depth perft, property/unit tests, and attacker-consistency playouts.",
        },
        WideVariantId::Xiangfu => Meta {
            display: "Xiang Fu (9x9 Xiangqi-themed drops)",
            pieces: "Champion (royal, ring-confined), Pupil (drop-only commoner), Horse, Chariot, Cannon, Crossbow (diagonal cannon), Bishop, and Mahout (non-jumping two-leaper).",
            rules: "9x9 board with a central 5x5 ring replacing the palaces; captures go to hand and drop onto own first two ranks; two pseudo-royal Champions under duple check (capture one to mate the other).",
            oracle: "Fairy-Stockfish (`UCI_Variant xiangfu`).",
        },
        WideVariantId::Xiangqi => Meta {
            display: "Xiangqi (Chinese chess, 9x10)",
            pieces: "General, Advisors, Elephants (river-bound), Horses (hobbled), Chariots, Cannons (over-screen), and Soldiers.",
            rules: "9x10 board with palace, river, and flying-general (generals may not face on an open file); soldiers step sideways only after crossing the river. Win by checkmate.",
            oracle: "Fairy-Stockfish (`UCI_Variant xiangqi`).",
        },
    }
}

#[test]
fn variants_doc_is_up_to_date() {
    let generated = render();
    let path = doc_path();
    if std::env::var_os("REGEN").is_some() || std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, &generated).expect("write docs/variants.md");
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        committed, generated,
        "docs/variants.md is out of date; regenerate with `REGEN=1 cargo test --test variants_doc`",
    );
}

/// The generated document must cover every registered variant exactly once, and
/// its counts must match the registries (a structural sanity check independent
/// of the golden-file comparison).
#[test]
fn every_variant_is_covered() {
    let doc = render();
    for &id in VariantId::ALL {
        assert!(
            doc.contains(&format!("`{}`", id.as_str())),
            "concrete variant {} missing from the doc",
            id.as_str()
        );
    }
    for &id in WideVariantId::ALL {
        assert!(
            doc.contains(&format!("`{}`", id.as_str())),
            "fairy variant {} missing from the doc",
            id.as_str()
        );
    }
}
