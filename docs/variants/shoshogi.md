<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# shoshogi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `shoshogi`
- Board: 9x9 (81 squares, `Shogi9x9` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant shoshogi`)

## Setup

Starting position (mcr FEN dialect):

```
lnsgkgsnl/1r2**e2b1/ppppppppp/9/9/9/PPPPPPPPP/1B2**E2R1/LNSGKGSNL w - - 0 1
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
| Lance | `l` | slider | no |
| DrunkElephant | `**e` | leaper / stepper | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward (+0,+1)

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

### Lance (`l`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)

### DrunkElephant (`**e`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: CrownPrince
- Promotion zone rank(s): 7, 8, 9
- Forced on the furthest rank
- Non-pawn pieces promote by ending in the zone (no hand)

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Pseudo-royal — every move must leave all royals safe.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 4-fold repetition
- Stalemate is a loss for the stalemated side
- Perpetual check loses for the checker

## Special mechanics

- None.

