<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# washogi

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `washogi`
- Board: 11x11 (121 squares, `Washogi11x11` geometry, 128-bit backing)
- Validation oracle: Independent — no external engine oracle (in-repo generator / hand-derived counts)

## Setup

Starting position (mcr FEN dialect):

```
**f**j**h**l**nk**o**k**g**m**d/1**v3**q3**t1/**b**b**b**r**b**b**b**u**b**b**b/11/11/11/11/11/**B**B**B**U**B**B**B**R**B**B**B/1**T3**Q3**V1/**D**M**G**K**OK**N**L**H**J**F[] w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| King | `k` | leaper / stepper | no |
| SparrowPawn | `**b` | leaper / stepper | no |
| Oxcart | `**d` | slider | no |
| LiberatedHorse | `**f` | slider | no |
| StruttingCrow | `**g` | leaper / stepper | no |
| SwoopingOwl | `**h` | leaper / stepper | no |
| ClimbingMonkey | `**j` | leaper / stepper | no |
| FlyingGoose | `**k` | leaper / stepper | no |
| FlyingCock | `**l` | leaper / stepper | no |
| BlindDog | `**m` | leaper / stepper | no |
| ViolentStag | `**n` | leaper / stepper | no |
| ViolentWolf | `**o` | leaper / stepper | no |
| SwallowsWings | `**q` | slider | no |
| RunningRabbit | `**r` | slider | no |
| FlyingFalcon | `**t` | slider | no |
| TreacherousFox | `**u` | leaper / stepper | no |
| CloudEagle | `**v` | slider | no |

### King (`k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### SparrowPawn (`**b`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward (+0,+1)

### Oxcart (`**d`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)

### LiberatedHorse (`**f`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): backward (+0,-1), forward (+0,+1)

### StruttingCrow (`**g`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward (+0,+1), back-right (+1,-1)

### SwoopingOwl (`**h`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward (+0,+1), back-right (+1,-1)

### ClimbingMonkey (`**j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), backward (+0,-1), forward (+0,+1), forward-right (+1,+1)

### FlyingGoose (`**k`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), backward (+0,-1), forward (+0,+1), forward-right (+1,+1)

### FlyingCock (`**l`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), right (+1,+0), forward-right (+1,+1)

### BlindDog (`**m`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), backward (+0,-1), right (+1,+0), forward-right (+1,+1)

### ViolentStag (`**n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### ViolentWolf (`**o`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)

### SwallowsWings (`**q`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), right (+1,+0)
  - single step / leap: backward (+0,-1), forward (+0,+1)

### RunningRabbit (`**r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), back-right (+1,-1), forward-right (+1,+1)

### FlyingFalcon (`**t`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: forward (+0,+1)

### TreacherousFox (`**u`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### CloudEagle (`**v`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward-left (-1,+1), backward (+0,-1), forward (+0,+1), forward-right (+1,+1)
  - single step / leap: back-left (-1,-1), left (-1,+0), back-right (+1,-1), right (+1,+0)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: GoldenBird
- Promotion zone rank(s): 9, 10, 11
- Forced on the furthest rank

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Single royal king — a side loses by checkmate.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 4-fold repetition
- Perpetual check loses for the checker

## Special mechanics

- Persistent hand with drops
- Pinned leapers are confined to the king–pinner segment

