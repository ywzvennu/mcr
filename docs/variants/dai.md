<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# dai

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `dai`
- Board: 15x15 (225 squares, `Dai15x15` geometry, 256-bit backing)
- Validation oracle: HaChu large-shogi reference engine

## Setup

Starting position (mcr FEN dialect):

```
l*n***z***u***csgkgs***c***u***z*nl/***r1m1***l1***t**e***t1***l1m1***r/1***x1*j1***f***p***n***k***f1*j1***x1/r***d***i***vb+b+rq+r+bb***v***i***dr/ppppppppppppppp/4***g5***g4/15/15/15/4***G5***G4/PPPPPPPPPPPPPPP/R***D***I***VB+B+RQ+R+BB***V***I***DR/1***X1*J1***F***K***N***P***F1*J1***X1/***R1M1***L1***T**E***T1***L1M1***R/L*N***Z***U***CSGKGS***C***U***Z*NL w - - 0 1
```

## Pieces & movement

Move and capture geometry are **sampled from the engine's own move hooks** on an empty board (White's orientation: positive rank = forward, positive file = toward the h-file). Each step is `direction (Δfile,Δrank)`; "rides" marks a repeating slider / rider.

| Piece | FEN | Type | Move ≠ capture |
|---|---|---|---|
| Pawn | `p` | leaper / stepper | yes |
| Bishop | `b` | slider | no |
| Rook | `r` | slider | no |
| Queen | `q` | slider | no |
| King | `k` | leaper / stepper | no |
| Met | `m` | leaper / stepper | no |
| Silver | `s` | leaper / stepper | no |
| Gold | `g` | leaper / stepper | no |
| Wazir | `j` | leaper / stepper | no |
| Lance | `l` | slider | no |
| Dragon | `r` | slider | no |
| DragonHorse | `b` | slider | no |
| ShogiKnight | `n` | leaper / stepper | no |
| DrunkElephant | `e` | leaper / stepper | no |
| CopperGeneral | `c` | leaper / stepper | no |
| FerociousLeopard | `l` | leaper / stepper | no |
| BlindTiger | `t` | leaper / stepper | no |
| GoBetween | `g` | leaper / stepper | no |
| ReverseChariot | `r` | slider | no |
| SideMover | `i` | slider | no |
| VerticalMover | `v` | slider | no |
| Kirin | `k` | leaper / stepper | no |
| Phoenix | `p` | leaper / stepper | no |
| ChuLion | `n` | leaper / stepper | no |
| ViolentOx | `x` | leaper / stepper | no |
| FlyingDragon | `d` | leaper / stepper | no |
| EvilWolf | `f` | leaper / stepper | no |
| IronGeneral | `u` | leaper / stepper | no |
| StoneGeneral | `z` | leaper / stepper | no |

### Pawn (`p`)

- Type: leaper / stepper
- Forward move is defined in the **Pawns** section; the geometry below is this piece's capture / threat set.
- Captures / threats:
  - single step / leap: forward (+0,+1)

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

### Met (`m`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Silver (`s`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### Gold (`g`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)

### Wazir (`j`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### Lance (`l`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): forward (+0,+1)

### Dragon (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### DragonHorse (`b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### ShogiKnight (`n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+2), forward-right (+1,+2)

### DrunkElephant (`e`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### CopperGeneral (`c`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), backward (+0,-1), forward (+0,+1), forward-right (+1,+1)

### FerociousLeopard (`l`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### BlindTiger (`t`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### GoBetween (`g`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: backward (+0,-1), forward (+0,+1)

### ReverseChariot (`r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): backward (+0,-1), forward (+0,+1)

### SideMover (`i`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), right (+1,+0)
  - single step / leap: backward (+0,-1), forward (+0,+1)

### VerticalMover (`v`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): backward (+0,-1), forward (+0,+1)
  - single step / leap: left (-1,+0), right (+1,+0)

### Kirin (`k`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Phoenix (`p`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### ChuLion (`n`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### ViolentOx (`x`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### FlyingDragon (`d`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### EvilWolf (`f`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: left (-1,+0), forward-left (-1,+1), forward (+0,+1), right (+1,+0), forward-right (+1,+1)

### IronGeneral (`u`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), forward (+0,+1), forward-right (+1,+1)

### StoneGeneral (`z`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), forward-right (+1,+1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Gold
- Promotion zone rank(s): 11, 12, 13, 14, 15
- Forced on the furthest rank
- Chu-Shogi lion-style promotion
- Non-pawn pieces promote by ending in the zone (no hand)

## Castling

- Not available.

## Draws & terminal conditions

**Royalty & win condition**

- Pseudo-royal — every move must leave all royals safe.

**Draw / adjudication rules**

- Repetition tracked; adjudicates on 4-fold repetition
- Perpetual check loses for the checker
- One-sided attack repetition loses

## Special mechanics

- Full Chu-Shogi Lion moves (igui, double capture, area move, pass)

