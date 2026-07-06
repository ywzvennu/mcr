<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# cannonshogi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `cannonshogi`
- Board: 9x9 (81 squares, `Shogi9x9` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant cannonshogi`)

## Setup

Starting position (mcr FEN dialect):

```
lnsgkgsnl/1r=c=i1c=ab1/p1p1p1p1p/9/9/9/P1P1P1P1P/1B=AC1=I=CR1/LNSGKGSNL[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| Knight | `n` | leaper / stepper | no |
| Bishop | `b` | slider | no |
| Rook | `r` | slider | no |
| King | `k` | leaper / stepper | no |
| Silver | `s` | leaper / stepper | no |
| Gold | `g` | leaper / stepper | no |
| Cannon | `c` | leaper / stepper | no |
| Lance | `l` | slider | no |
| RookCannon | `a` | screen hopper | no |
| BishopCannon | `c` | leaper / stepper | no |
| BishopHopper | `i` | screen hopper | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: left (-1,+0), forward (+0,+1), right (+1,+0)

### Knight (`n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+2), forward-right (+1,+2)

### Bishop (`b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Silver (`s`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### Gold (`g`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)

### Cannon (`c`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Lance (`l`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)

### RookCannon (`a`)

- Type: screen hopper
- Move/capture is occupancy-dependent (needs a screen); not sampled on an empty board.

### BishopCannon (`c`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### BishopHopper (`i`)

- Type: screen hopper
- Move/capture is occupancy-dependent (needs a screen); not sampled on an empty board.

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Gold
- Promotion zone rank(s): 7, 8, 9
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 4-fold repetition
- Perpetual check loses for the checker

## Special mechanics

- Fields cannons (screen-hopping capture)
- Persistent hand with drops

