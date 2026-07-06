<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# xiangfu

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `xiangfu`
- Board: 9x9 (81 squares, `Shogi9x9` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant xiangfu`)

## Setup

Starting position (mcr FEN dialect):

```
2rb=m4/2c=cj4/2=k1=k4/9/9/9/4=K1=K2/4J=CC2/4=MBR2[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Bishop | `b` | slider | no |
| Rook | `r` | slider | no |
| Cannon | `c` | leaper / stepper | no |
| Horse | `j` | leaper / stepper | no |
| BishopCannon | `=c` | leaper / stepper | no |
| Champion | `=k` | leaper / stepper | no |
| Mahout | `=m` | leaper / stepper | no |

### Bishop (`b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Cannon (`c`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Horse (`j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### BishopCannon (`=c`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Champion (`=k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Mahout (`=m`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Knight, Bishop, Rook, Queen
- Promotion zone rank(s): 9
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Multiple royals — in check only when every royal is attacked; a side may sacrifice one royal and play on.

**Draw / adjudication rules**

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- Persistent hand with drops

