<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# yarishogi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `yarishogi`
- Board: 7x9 (63 squares, `YariShogi7x9` geometry, 64-bit backing)
- Validation oracle: Independent — no external engine oracle (in-repo generator / hand-derived counts)

## Setup

Starting position (mcr FEN dialect):

```
****o****j****jk****a****a****o/7/ppppppp/7/7/7/PPPPPPP/7/****O****A****AK****J****J****O[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| King | `k` | leaper / stepper | no |
| YariRook | `****o` | slider | no |
| YariKnight | `****j` | slider | no |
| YariBishop | `****a` | slider | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward (+0,+1)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### YariRook (`****o`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), forward (+0,+1), right (+1,+0)

### YariKnight (`****j`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)
  - single step / leap: forward-left (-1,+2), forward-right (+1,+2)

### YariBishop (`****a`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)
  - single step / leap: forward-left (-1,+1), forward-right (+1,+1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: YariGold
- Promotion zone rank(s): 7, 8, 9
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

- Persistent hand with drops

