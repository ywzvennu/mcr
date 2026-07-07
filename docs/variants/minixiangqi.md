<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# minixiangqi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `minixiangqi`
- Board: 7x7 (49 squares, `Minixiangqi7x7` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant minixiangqi`)

## Setup

Starting position (mcr FEN dialect):

```
rcjkjcr/z1zzz1z/7/7/7/Z1ZZZ1Z/RCJKJCR w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Rook | `r` | slider | no |
| King | `k` | leaper / stepper | no |
| Cannon | `c` | leaper / stepper | yes |
| Horse | `j` | leaper / stepper | no |
| Soldier | `z` | leaper / stepper | no |

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Cannon (`c`)

- Type: leaper / stepper
- **Move ≠ capture** — the two geometries differ.
- Moves (non-capturing):
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
- Captures / gives check: none sampled

### Horse (`j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Soldier (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward (+0,+1), right (+1,+0)

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Knight, Bishop, Rook, Queen
- Promotion zone rank(s): 7
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 3-fold repetition
- Stalemate is a loss for the stalemated side
- Perpetual check loses for the checker

## Special mechanics

- Fields cannons (screen-hopping capture)
- Flying-general rule (facing generals)

