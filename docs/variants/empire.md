<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# empire

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `empire`
- Board: 8x8 (64 squares, `Chess8x8` geometry, 64-bit backing)
- Validation oracle: Fairy-Stockfish (`UCI_Variant empire`)

## Setup

Starting position (mcr FEN dialect):

```
rnbqkbnr/pppppppp/8/8/8/PPPZZPPP/8/*T*E*C*DK*C*E*T w kq - 0 1
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
| Soldier | `z` | leaper / stepper | no |
| Eagle | `*e` | whole-board attacker | no |
| Cardinal | `*c` | whole-board attacker | no |
| Tower | `*t` | whole-board attacker | no |
| Duke | `*d` | whole-board attacker | no |

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

### Soldier (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward (+0,+1), right (+1,+0)

### Eagle (`*e`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### Cardinal (`*c`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### Tower (`*t`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

### Duke (`*d`)

- Type: whole-board attacker
- Attack set is computed from the whole board; not sampled on an empty board.

## Pawns

- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Queen
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

- Stalemate is a loss for the stalemated side

## Special mechanics

- Some role's attack set is computed from the whole board
- Flying-general rule (facing generals)

