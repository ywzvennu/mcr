<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# mansindam

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `mansindam`
- Board: 9x9 (81 squares, `Shogi9x9` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant mansindam`)

## Setup

Starting position (mcr FEN dialect):

```
rnb**akqane/9/ppppppppp/9/9/9/PPPPPPPPP/9/ENAQK**ABNR[] w - - 0 1
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
| Hawk | `a` | slider | no |
| Elephant | `e` | slider | no |
| Angel | `a` | slider | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward (+0,+1)

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

### Hawk (`a`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Elephant (`e`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Angel (`a`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Commoner
- Promotion zone rank(s): 7, 8, 9
- Forced on the furthest rank
- Mandatory anywhere in the zone (Shogi far-zone rule)

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.
- Flag / campmate: a king reaching rank 9 wins

**Draw / adjudication rules**

- No special draw rules beyond the standard checkmate / stalemate.

## Special mechanics

- Persistent hand with drops

