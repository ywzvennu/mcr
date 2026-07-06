<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# placement

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `placement`
- Board: 8x8 (64 squares, `Chess8x8` geometry, 64-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant placement`)

## Setup

Starting position (mcr FEN dialect):

```
8/pppppppp/8/8/8/8/PPPPPPPP/8[NNBBRRQKnnbbrrqk] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward-left (-1,+1), forward-right (+1,+1)

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Knight, Bishop, Rook, Queen
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

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- Setup / placement phase

