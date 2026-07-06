<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# ordamirror

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `ordamirror`
- Board: 8x8 (64 squares, `Chess8x8` geometry, 64-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant ordamirror`)

## Setup

Starting position (mcr FEN dialect):

```
fwy*fkywf/8/pppppppp/8/8/PPPPPPPP/8/FWY*FKYWF w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| King | `k` | leaper / stepper | no |
| Lancer | `f` | slider | yes |
| Kheshig | `w` | leaper / stepper | no |
| Archer | `y` | slider | yes |
| Falcon | `*f` | leaper / stepper | yes |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward-left (-1,+1), forward-right (+1,+1)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Lancer (`f`)

- Type: slider
- **Move ≠ capture** — the two geometries differ.
- Moves (non-capturing):
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)
- Captures / gives check:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Kheshig (`w`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward-left (-1,+2), backward (+0,-1), forward (+0,+1), back-right (+1,-2), back-right (+1,-1), right (+1,+0), forward-right (+1,+1), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Archer (`y`)

- Type: slider
- **Move ≠ capture** — the two geometries differ.
- Moves (non-capturing):
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)
- Captures / gives check:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Falcon (`*f`)

- Type: leaper / stepper
- **Move ≠ capture** — the two geometries differ.
- Moves (non-capturing):
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)
- Captures / gives check:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Lancer, Kheshig, Archer, Falcon
- Promotion zone rank(s): 8
- Forced on the furthest rank

## Castling

- Castling rank (White): 1
- Kingside: king lands on the g-file, castling with the Rook
- Queenside: king lands on the c-file, castling with the Rook

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.
- Flag / campmate: a king reaching rank 8 wins

**Draw / adjudication rules**

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- None.

