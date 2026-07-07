# Validation of the oracle-less variants

Most mcr variants are pinned node-for-node against **Fairy-Stockfish** (FSF) by the
`compare-fairy/` differential perft sweep. A handful cannot be: FSF does not
implement them, and either the only reference engine (**HaChu**, for large shogi)
covers them shallowly, buggily, or not at all — or there is no external engine
oracle whatsoever. These are the thinnest-validated variants in the tree. This
page states, per variant, **exactly** what each is validated against, **to what
depth**, and **what remains unverified**. Honesty about the residual gap is the
point: nothing here should be read as more strongly validated than it is.

The authoritative machine-readable provenance lives in two places, kept in sync
with the code by drift-check tests:

- `tests/coverage_gate.rs` — the `PerftOracle` per variant (`HaChu`,
  `HandDerived`, `HandDerivedX2`, `Fsf`), the pinned depth, and the difffuzz
  status.
- `src/geometry/rules.rs` — `ValidationOracle` (`FairyStockfish`, `HaChu`,
  `Independent`), surfaced at runtime by `WideVariantId::validation_oracle()`.

## The oracle-less set

Selected by `ValidationOracle` (runtime) and the non-`Fsf` `PerftOracle` rows:

| Variant | Board | Oracle | Perft cross-checked | mcr-only pin | Second source |
|---|---|---|---|---|---|
| Chu Shogi | 12x12 | HaChu | depth 1–2 node-for-node; depth 3 all-but-one node | depth 4 | HaChu 0.23 (one documented HaChu bug at depth 3) |
| Dai Shogi | 15x15 | HaChu | depth 1–3 node-for-node (full depth-3 divide) | depth 4 | HaChu 0.23 |
| Tenjiku Shogi | 16x16 | Independent | depth 1 vs HaChu **source tables**; depth 2–3 vs in-repo brute force | depth 4+ | independent in-repo 16x16 generator |
| Alice Chess | 8x8 ×2 | Independent | depth 1–4 vs in-repo brute force | depth 5 | independent in-repo two-board generator |
| Jieqi | 9x10 | Independent (Hand-derived) | depth 1–4 live vs **FSF Xiangqi** (identity reveal) | — | FSF `xiangqi`, via the identity-reveal equivalence |
| Wa Shogi | 11x11 | Independent | depth 1–3 vs in-repo brute force | depth 4 | independent in-repo 11x11 generator |
| Okisaki Shogi | 10x10 | Independent | depth 1 hand-derived; depths 1–3 vs in-repo generator | depth 4 | independent in-repo 10x10 generator |

"Second source" is what stands in for the missing FSF oracle. For the HaChu-oracle
variants it is the HaChu engine (driven as a subprocess by `compare-fairy`, never
vendored or linked — see `compare-fairy/src/locate_hachu.rs`). For the Independent
variants it is a **second, fully from-scratch implementation in the same repo**
(its own board model and move generator), so two independent programs must agree
on every node count — issue #500.

## Invariants every oracle-less variant is held to

Beyond perft, all six are swept by the crate's variant-generic property tests,
which iterate `WideVariantId::ALL` and therefore cover these variants with no
per-variant opt-in:

- **make / unmake byte-identity** (board, state, promoted mask, and the Zobrist
  key restored exactly) — `make_unmake_round_trips_for_every_variant` and the deep
  seeded `make_unmake_line_for_every_variant` in `src/geometry/any.rs`.
- **FEN fixed point + path-independent hash** — `properties::wide_fen_round_trip`,
  `invariants::any_fen_and_hash`.
- **UCI / SAN round-trip injectivity** — `properties::wide_uci_round_trip`,
  `notation_roundtrip.rs`, `invariants::any_move_list_integrity`.
- **perft children-sum** (`perft(n) == Σ_child perft(n−1)`, which also
  cross-checks the make/unmake tree walk against copy-make expansion) —
  `properties::perft_children_sum_wide`.
- **attackers-consistency** (`attackers_to` reverse projection ≡ the forward
  attack relation, and king-safety agreement) — `tests/attackers_consistency.rs`.
  Chu, Dai, Tenjiku, and Alice were added to this sweep under issue #558; Jieqi and
  Wa Shogi were already covered.
- **colour symmetry of the start** — `tests/symmetry_oracle_less.rs` (issue #558):
  for the five colour-symmetric starts, handing the first move to Black gives an
  identical perft. Tenjiku is deliberately excluded (see below).

None of these needs an external oracle; they are self-consistency checks that would
catch a large class of move-generation defects independently of the perft pins.

## Per-variant residual trust gap

### Chu Shogi — HaChu oracle
- **perft(1) = 36** is a **byte-identical** move-set match with HaChu.
- **perft(2) = 1296** matches HaChu **exactly**, node-for-node.
- **perft(3) = 48319** matches HaChu at every node **except one**: HaChu 0.23 misses
  two legal anti-diagonal Lion captures (a HaChu bug — mcr is correct and
  symmetric), so HaChu's tree totals 48317. This single adjudicated divergence is
  documented in `tests/perft_chu.rs`.
- **perft(4) = 1802285** is an **mcr-only regression pin**: a node-by-node HaChu
  cross-check at ~1.8M nodes (one subprocess per node) is intractable.
- **Residual gap:** depth ≥ 4 is not oracle-validated; depth 3 depends on a
  human adjudication of one known HaChu bug. Mitigated by the invariants above and
  the colour-symmetry check.

### Dai Shogi — HaChu oracle
- **perft(1) = 71** is a node-for-node identical move-set match with HaChu.
- **perft(2) = 5041** matches HaChu exactly at every root.
- **perft(3) = 357836** is validated **node-for-node** by a full depth-3 divide
  walk against HaChu (issue #500).
- **perft(4) = 25400968** is an **mcr-only regression pin** (`#[ignore]`d, ~25M
  nodes); a depth-4 HaChu cross-check is intractable.
- **Residual gap:** depth ≥ 4 is not oracle-validated.

### Tenjiku Shogi — no usable engine oracle
- HaChu **crashes** on `variant tenjiku`, so there is no live oracle at all.
- **perft(1) = 72** is validated node-for-node against HaChu's **source tables**
  (hand-transcribed, not a live run).
- **perft(2) = 5663** and **perft(3) = 424582** are cross-checked node-for-node
  against an **independent in-repo brute-force generator** (issue #500), so they are
  no longer self-referential.
- The start array **faithfully reproduces HaChu's documented asymmetry** (White's
  second rank is one file short of Black's); it is **not** colour-symmetric by
  design, which is why `symmetry_oracle_less.rs` excludes it — colour symmetry
  would contradict the HaChu-matching goal (White perft(1) = 72, Black = 79).
- **Residual gap:** no live external engine oracle whatsoever; depths 2–3 rest on
  two independent in-repo implementations agreeing; depth ≥ 4 is single-source.

### Alice Chess — no external oracle
- FSF implements no two-board teleport variant.
- **perft(1) = 20, perft(2) = 400** are hand-derived (standard chess with the
  transfer overlaid; see `tests/perft_alice.rs`).
- **perft(3) = 9384, perft(4) = 219236** are cross-checked against an **independent
  from-scratch two-board generator**, non-`#[ignore]`d to depth 4 (issue #500).
- **perft(5) = 5910465** is an engine-only pin (`#[ignore]`d).
- **Residual gap:** no external oracle; depth ≥ 5 is single-source (mcr's own
  generator only).

### Jieqi — hidden Xiangqi, validated via the identity-reveal equivalence
- Under mcr's model a face-down piece moves as the piece native to its **home
  square** (the identity baseline), which makes an identity-revealed Jieqi position
  **bit-identical to the equivalent Xiangqi position**.
- The all-dark start matches the **FSF-confirmed Xiangqi** perft at depths 1–4 (live
  in `all_dark_startpos_matches_xiangqi_live`), a fully-revealed middlegame matches
  Xiangqi, and a seeded lockstep playout keeps Jieqi and Xiangqi move-set-identical
  through reveals. Xiangqi itself is FSF-validated, so under the identity baseline
  Jieqi's movement inherits a real external oracle.
- **Residual gap:** what is validated is the identity-reveal *movement model*. The
  genuinely **hidden** reveal mechanic — a face-down piece resolving to a *random*
  member of the remaining pool rather than its home role — is a game-play layer that
  a static perft cannot express and that is therefore **out of scope** of this
  validation. Movement, drops-free legality, and the reveal transition are covered;
  the stochastic identity assignment is not.

### Wa Shogi — no usable engine oracle
- FSF does not implement Wa Shogi and it is inexpressible via `variants.ini`.
- HaChu ships a **different** Wa Shogi ruleset (51 vs mcr's 57 start moves, a
  different start array and piece set), recorded by `compare-fairy`'s
  `probe_washogi`, so it is **not** a usable node-for-node oracle.
- **perft(1) = 57** is hand-derived from the documented ruleset; **perft(2) = 3204**
  and **perft(3) = 174579** are produced **identically** by the engine and an
  **independent from-scratch 11x11 generator** (issue #500).
- **perft(4) = 9531440** is an engine-only pin (`#[ignore]`d).
- **Residual gap:** no external oracle; two in-repo implementations agree to
  depth 3; depth ≥ 4 is single-source.

### Okisaki Shogi — FSF built-in, but no large-board binary
- Okisaki Shogi is a Fairy-Stockfish built-in (`okisakishogi`), but the FSF binary
  available here is a **non-large-board build**: asked for `UCI_Variant okisakishogi`
  it silently stays in standard chess (a `go perft` returns the 20-move chess root),
  so there is **no live FSF oracle**.
- **perft(1) = 37** is **hand-derived** (the per-piece enumeration is tabulated in
  `tests/perft_okisakishogi.rs`); **perft(2) = 1369 = 37²** follows from the armies
  starting four ranks apart (no White first move alters Black's reply count).
- **perft(2)/(3)** for the start position and **perft(1)–(3)** for two midgame
  positions (one where nifu fully suppresses a held pawn's drops, one with a Queen in
  hand exercising 58 drops) are produced **identically** by the engine and an
  **independent from-scratch 10x10 generator** in the same test file (its own board
  model, move tables, hand/drops, nifu, per-piece promotion and king safety).
- **perft(4) = 1697913** is an engine-only regression pin (`#[ignore]`d).
- **Residual gap:** no external engine oracle; two in-repo implementations agree to
  depth 3; depth ≥ 4 is single-source.

## Summary of what is and is not claimed

- **Claimed:** low-depth move generation for each variant is either matched
  node-for-node against a reference engine (Chu, Dai; Tenjiku's depth 1 against
  HaChu source) or agreed on node-for-node by two fully independent in-repo
  implementations (Tenjiku 2–3, Alice 1–4, Wa 1–3), Jieqi movement is equal to
  FSF-validated Xiangqi under the identity baseline, and every variant satisfies the
  make/unmake, hash, notation, attackers-consistency, and (where applicable)
  colour-symmetry invariants.
- **Not claimed:** oracle agreement at the deeper `#[ignore]`d regression pins
  (Chu ≥4, Dai ≥4, Alice ≥5, Wa ≥4, all Tenjiku depths beyond the independent
  cross-check), and — for Jieqi — the stochastic hidden-reveal identity mechanic.
