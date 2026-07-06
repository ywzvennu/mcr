<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# chennis

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `chennis`
- Board: 7x7 (49 squares, `Chennis7x7` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant chennis`)

## Setup

Starting position (mcr FEN dialect):

```
1mk*u3/1**p1z3/7/7/7/3Z1**P1/3*UKM1[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| King | `k` | leaper / stepper | no |
| Met | `m` | leaper / stepper | no |
| Soldier | `z` | leaper / stepper | no |
| Commoner | `*u` | leaper / stepper | no |
| ChennisPawn | `**p` | whole-board attacker | no |

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Met (`m`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Soldier (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward (+0,+1), right (+1,+0)

### Commoner (`*u`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### ChennisPawn (`**p`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Rook
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Stalemate is a loss for the stalemated side

## Special mechanics

- Fields cannons (screen-hopping capture)
- Some role's attack set is computed from the whole board
- Persistent hand with drops

