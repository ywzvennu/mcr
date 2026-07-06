<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# grasshopper

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `grasshopper`
- Board: 8x8 (64 squares, `Chess8x8` geometry, 64-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant grasshopper`)

## Setup

Starting position (mcr FEN dialect):

```
rnbqkbnr/***j***j***j***j***j***j***j***j/pppppppp/8/8/PPPPPPPP/***J***J***J***J***J***J***J***J/RNBQKBNR w KQkq - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| Knight | `n` | leaper / stepper | no |
| Bishop | `b` | slider | no |
| Rook | `r` | slider | no |
| Queen | `q` | slider | no |
| King | `k` | leaper / stepper | no |
| Grasshopper | `***j` | screen hopper | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward-left (-1,+1), forward-right (+1,+1)

### Knight (`n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Bishop (`b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Queen (`q`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Grasshopper (`***j`)

- Type: screen hopper
- Move/capture is occupancy-dependent (needs a screen); not sampled on an empty board.

## Pawns

- En passant available

## Promotion

- Promotes to: Knight, Bishop, Rook, Queen, Grasshopper
- Promotion zone rank(s): 8
- Forced on the furthest rank

## Castling

- Castling rank (White): 1
- Kingside: king lands on the g-file, castling with the Rook
- Queenside: king lands on the c-file, castling with the Rook

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Move-count draw after 100 plies
- Repetition tracked; adjudicates on 3-fold repetition

## Special mechanics

- Fields cannons (screen-hopping capture)

