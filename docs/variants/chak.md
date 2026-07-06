<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# chak

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `chak`
- Board: 9x9 (81 squares, `Shogi9x9` geometry, 128-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant chak`)

## Setup

Starting position (mcr FEN dialect):

```
rn*s*qkw*snr/4*o4/*p1*p1*p1*p1*p/9/9/9/*P1*P1*P1*P1*P/4*O4/RN*SWK*Q*SNR w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Knight | `n` | leaper / stepper | no |
| Rook | `r` | slider | no |
| King | `k` | leaper / stepper | no |
| Kheshig | `w` | leaper / stepper | no |
| Serpent | `*s` | leaper / stepper | no |
| Quetzal | `*q` | whole-board attacker | no |
| ChakSoldier | `*p` | whole-board attacker | no |
| Temple | `*o` | leaper / stepper | no |

### Knight (`n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Rook (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### Kheshig (`w`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward-left (-1,+2), backward (+0,-1), forward (+0,+1), back-right (+1,-2), back-right (+1,-1), right (+1,+0), forward-right (+1,+1), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### Serpent (`*s`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### Quetzal (`*q`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### ChakSoldier (`*p`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### Temple (`*o`)

- Type: leaper / stepper
- Immobile on an empty board (no step sampled).

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: DivineLord
- Promotion zone rank(s): 5, 6, 7, 8, 9
- Forced on the furthest rank
- Mandatory anywhere in the zone (Shogi far-zone rule)
- Non-pawn pieces promote by ending in the zone (no hand)

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Pseudo-royal — every move must leave all royals safe.
- Move a Divine Lord onto the enemy temple to win

**Draw / adjudication rules**

- Stalemate is a loss for the stalemated side

## Special mechanics

- Some role's attack set is computed from the whole board

