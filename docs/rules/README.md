<!-- GENERATED DIRECTORY — the *.json files are generated; do not edit them by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --features serde --test rules_json (see tests/rules_json.rs). -->

# mcr machine-readable rules (`docs/rules/`)

One JSON file per variant plus an `index.json` manifest, serialized straight
from mcr's engine-derived `VariantRules` model (`src/geometry/rules.rs`) via
`serde`. Every value is read out of the variant's own move-generation hooks, so
the JSON can never disagree with the engine. A drift-check test
(`tests/rules_json.rs`) regenerates these files and asserts they match the
committed copies, so they never fall behind the code.

- `<name>.json` — the full ruleset of one variant, keyed by
  `VariantRef::name()`. There is one per `VariantRef::ALL` (the 9 concrete 8x8
  variants and the 90 fairy variants).
- `index.json` — a flat array of every variant with the key fields for
  discovery (see below), so a consumer can triage without opening each file.

## Per-variant file (`<name>.json`)

Top-level keys, each mirroring a field of `VariantRules`:

- `board` — geometry and start position: `width`, `height`, `square_count`,
  `backing_bits`, `geometry` (marker-type short name), `start_fen`.
- `army` — the roles on the starting board. Each entry: `role` / `name` (role
  identifier), `fen_char`, `is_slider`, `hopper` (occupancy-dependent screen
  hop), `board_dependent` (whole-board attacker), `move_neq_capture`, and
  `movement` / `capture` geometries. A geometry is `{ "steps": [...] }` where
  each step is `{ "file", "rank", "rides" }`: a primitive `(file, rank)`
  direction (White's orientation) with `rides` set for a repeating
  slider/rider. Steps are empty for hoppers, whole-board attackers, and the
  pawn's forward move (carried authoritatively by `pawns`).
- `pawns` — pawn movement: `double_step_ranks`, `double_step_any_rank`,
  `en_passant`, `moves_sideways`, `moves_backward`, `berolina`, `legan`,
  `stepper`, `move_resets_clock`.
- `promotion` — `roles`, `zone_ranks`, `forced_on_last_rank`,
  `mandatory_in_zone`, `lion_style`, `piece_promotion_no_hand`.
- `castling` — `enabled`, `rook_role_kingside`, `rook_role_queenside`,
  `castle_rank_white`, `king_dest_kingside`, `king_dest_queenside`.
- `draw` — draw / repetition / counting adjudication: `move_rule_plies`,
  `tracks_repetition`, `repetition_fold`, `counting_rule`, `impasse`,
  `has_bikjang`, `stalemate_is_loss`, `has_bare_king_draw`,
  `has_bare_king_loss`, `perpetual_check_loses`, `perpetual_chase_loses`,
  `attack_repetition_loses`.
- `terminal` — win / terminal conditions: `royal`
  (`checkmate` / `non_royal` / `multi_royal_any_survives` /
  `pseudo_royal_all_survive`), `extinction`, `flag_win`, `stalemate_is_loss`,
  `wins_on_check`, `temple_win`, `bare_king_draw`, `bare_king_loss`,
  `explosion_win`, `lose_all_wins`, `all_pieces_lost_loses`,
  `check_count_to_win`, `region_win`.
- `mechanics` — special board mechanics (`needs_full_verify`, `has_petrify`,
  `petrifying_roles`, `has_cannons`, `has_hand`, `has_placement`,
  `supports_gating`, `has_duck`, `is_alice`, `has_lion_moves`, `allows_pass`,
  `atomic_blast`, and the rest).
- `oracle` — the external validation oracle: `{ "fairy_stockfish": "<name>" }`,
  `"ha_chu"`, or `"independent"`.

Ranks and files are 0-based in White's orientation. `null` fields are optional
rules that do not apply to the variant.

## Manifest (`index.json`)

A flat array; each entry has `name`, `family` (`concrete` or `wide`), `width`,
`height`, `start_fen`, `roles` (the army roster by name), and `oracle`.
