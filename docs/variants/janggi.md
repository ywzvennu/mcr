<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# janggi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `janggi`
- Board: 9x10 (90 squares, `Xiangqi9x10` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant janggi`)

## Setup

Starting position (mcr FEN dialect):

```
rjxu1uxjr/4k4/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/1C5C1/4K4/RJXU1UXJR w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Rook | `r` | slider | no |
| King | `k` | leaper / stepper | no |
| Cannon | `c` | whole-board attacker | no |
| Advisor | `u` | leaper / stepper | no |
| Horse | `j` | leaper / stepper | no |
| Soldier | `z` | leaper / stepper | no |
| JanggiElephant | `x` | leaper / stepper | no |

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Cannon (`c`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### Advisor (`u`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Horse (`j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Soldier (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)

### JanggiElephant (`x`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-3,-2), forward-left (-3,+2), back-left (-2,-3), forward-left (-2,+3), back-right (+2,-3), forward-right (+2,+3), back-right (+3,-2), forward-right (+3,+2)

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Knight, Bishop, Rook, Queen
- Promotion zone rank(s): 10
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 3-fold repetition
- Bikjang: the two generals facing on an open line draws
- Perpetual check loses for the checker

## Special mechanics

- Fields cannons (screen-hopping capture)
- Some role's attack set is computed from the whole board
- A side may pass the turn

