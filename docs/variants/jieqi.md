<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# jieqi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `jieqi`
- Board: 9x10 (90 squares, `Xiangqi9x10` geometry, 128-bit backing)
- Validation oracle: Independent — no external engine oracle (in-repo generator / hand-derived counts)

## Setup

Starting position (mcr FEN dialect):

```
=d=d=d=dk=d=d=d=d/9/1=d5=d1/=d1=d1=d1=d1=d/9/9/=D1=D1=D1=D1=D/1=D5=D1/9/=D=D=D=DK=D=D=D=D w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| King | `k` | leaper / stepper | no |
| Dark | `d` | leaper / stepper | no |

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Dark (`d`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

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

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- Fields cannons (screen-hopping capture)
- Flying-general rule (facing generals)

