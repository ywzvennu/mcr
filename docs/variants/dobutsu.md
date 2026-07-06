<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# dobutsu

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `dobutsu`
- Board: 3x4 (12 squares, `Dobutsu3x4` geometry, 64-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant dobutsu`)

## Setup

Starting position (mcr FEN dialect):

```
*jkm/1p1/1P1/MK*J[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| King | `k` | leaper / stepper | no |
| Met | `m` | leaper / stepper | no |
| Wazir | `*j` | leaper / stepper | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward (+0,+1)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Met (`m`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Wazir (`*j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Tokin
- Promotion zone rank(s): 4
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- King is non-royal (no check) — a side loses by king capture / extinction.
- Flag / campmate: a king reaching rank 4 wins (the king must be safe there)

**Draw / adjudication rules**

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- Persistent hand with drops

