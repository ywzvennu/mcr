<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# manchu

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `manchu`
- Board: 9x10 (90 squares, `Xiangqi9x10` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant manchu`)

## Setup

Starting position (mcr FEN dialect):

```
rjoukuojr/9/1c5c1/z1z1z1z1z/9/9/Z1Z1Z1Z1Z/9/9/*M1OUKUO2 w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Rook | `r` | slider | no |
| King | `k` | leaper / stepper | no |
| Cannon | `c` | leaper / stepper | yes |
| Advisor | `u` | leaper / stepper | no |
| Horse | `j` | leaper / stepper | no |
| XiangqiElephant | `o` | leaper / stepper | no |
| Soldier | `z` | leaper / stepper | no |
| Banner | `*m` | whole-board attacker | no |

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

### Advisor (`u`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Horse (`j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### XiangqiElephant (`o`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Soldier (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward (+0,+1), right (+1,+0)

### Banner (`*m`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

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

- Stalemate is a loss for the stalemated side

## Special mechanics

- Fields cannons (screen-hopping capture)
- Some role's attack set is computed from the whole board
- Flying-general rule (facing generals)

