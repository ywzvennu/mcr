<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# tori

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `tori`
- Board: 7x7 (49 squares, `Tori7x7` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant torishogi`)

## Setup

Starting position (mcr FEN dialect):

```
*r*z*kk*k*z*v/3*a3/*y*y*y*y*y*y*y/2*y1*Y2/*Y*Y*Y*Y*Y*Y*Y/3*A3/*V*Z*KK*K*Z*R[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| King | `k` | leaper / stepper | no |
| Swallow | `*y` | leaper / stepper | no |
| ToriFalcon | `*a` | leaper / stepper | no |
| Crane | `*k` | leaper / stepper | no |
| LeftQuail | `*v` | slider | no |
| RightQuail | `*r` | slider | no |
| Pheasant | `*z` | leaper / stepper | no |

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Swallow (`*y`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward (+0,+1)

### ToriFalcon (`*a`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Crane (`*k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### LeftQuail (`*v`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1), back-right (+1,-1)
  - single step / leap: back-left (-1,-1)

### RightQuail (`*r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward (+0,+1)
  - single step / leap: back-right (+1,-1)

### Pheasant (`*z`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)
  - single step / leap: back-left (-1,-1), back-right (+1,-1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Goose
- Promotion zone rank(s): 6, 7
- Forced on the furthest rank
- Mandatory anywhere in the zone (Shogi far-zone rule)

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 4-fold repetition
- Perpetual check loses for the checker

## Special mechanics

- Persistent hand with drops
- Pinned leapers are confined to the king–pinner segment

