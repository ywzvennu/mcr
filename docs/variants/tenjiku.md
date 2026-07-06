<!-- GENERATED FILE — do not edit by hand. -->
<!-- Regenerate with: REGEN=1 cargo test --test variant_pages_doc (see tests/variant_pages_doc.rs). -->

# tenjiku

Engine-derived ruleset — every statement below is rendered from mcr's own `VariantRules` model, so it can never drift from the move generator. See the [index](README.md) for all variants.

## Overview

- Id: `tenjiku`
- Board: 16x16 (256 squares, `Tenjiku16x16` geometry, 256-bit backing)
- Validation oracle: Independent — no external engine oracle (in-repo generator / hand-derived counts)

## Setup

Starting position (mcr FEN dialect):

```
l*n***l***u***csg**ekgs***c***u***l*nl/***r1****c****c1***t***pq***n***k***t1****c****c1***r/****s****lb+b+r****w****i****g****h****i****w+r+bb****l****s/***i***vr***h***e****b****r****v****g****r****b***e***hr***v***i/pppppppppppppppp/4+r6+r4/16/16/16/16/4+R6+R4/PPPPPPPPPPPPPPPP/***I***VR***H***E****B****R****G****V****R****B***E***HR***V***I/****S****LB+B+R****W****I****H****E****I****W+R+BB****L****S/***R1****C****C***T***K***NQ***P***T1****C****C1***R1/L*N***L***U***CSGK**EGS***C***U***L*NL w - - 0 1
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
| Silver | `s` | leaper / stepper | no |
| Gold | `g` | leaper / stepper | no |
| Lance | `l` | slider | no |
| Dragon | `+r` | slider | no |
| DragonHorse | `+b` | slider | no |
| ShogiKnight | `*n` | leaper / stepper | no |
| DrunkElephant | `**e` | leaper / stepper | no |
| CopperGeneral | `***c` | leaper / stepper | no |
| FerociousLeopard | `***l` | leaper / stepper | no |
| BlindTiger | `***t` | leaper / stepper | no |
| ReverseChariot | `***r` | slider | no |
| SideMover | `***i` | slider | no |
| VerticalMover | `***v` | slider | no |
| Kirin | `***k` | leaper / stepper | no |
| Phoenix | `***p` | leaper / stepper | no |
| ChuLion | `***n` | leaper / stepper | no |
| HornedFalcon | `***h` | slider | no |
| SoaringEagle | `***e` | slider | no |
| IronGeneral | `***u` | leaper / stepper | no |
| FireDemon | `****i` | slider | no |
| GreatGeneral | `****g` | slider | no |
| ViceGeneral | `****v` | slider | no |
| RookGeneral | `****r` | slider | no |
| BishopGeneral | `****b` | slider | no |
| LionHawk | `****h` | slider | no |
| FreeEagle | `****e` | slider | no |
| ChariotSoldier | `****c` | slider | no |
| WaterBuffalo | `****w` | slider | no |
| VerticalSoldier | `****l` | slider | no |
| SideSoldier | `****s` | slider | no |

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

### Dragon (`+r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### DragonHorse (`+b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### ShogiKnight (`*n`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+2), forward-right (+1,+2)

### DrunkElephant (`**e`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### CopperGeneral (`***c`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), backward (+0,-1), forward (+0,+1), forward-right (+1,+1)

### FerociousLeopard (`***l`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### BlindTiger (`***t`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### ReverseChariot (`***r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): backward (+0,-1), forward (+0,+1)

### SideMover (`***i`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), right (+1,+0)
  - single step / leap: backward (+0,-1), forward (+0,+1)

### VerticalMover (`***v`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): backward (+0,-1), forward (+0,+1)
  - single step / leap: left (-1,+0), right (+1,+0)

### Kirin (`***k`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)
  - single step / leap: back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### Phoenix (`***p`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)
  - single step / leap: left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### ChuLion (`***n`)

- Type: leaper / stepper
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### HornedFalcon (`***h`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### SoaringEagle (`***e`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### IronGeneral (`***u`)

- Type: leaper / stepper
- Moves & captures:
  - single step / leap: forward-left (-1,+1), forward (+0,+1), forward-right (+1,+1)

### FireDemon (`****i`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), forward-right (+1,+1)

### GreatGeneral (`****g`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### ViceGeneral (`****v`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### RookGeneral (`****r`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), backward (+0,-1), forward (+0,+1), right (+1,+0)

### BishopGeneral (`****b`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), forward-left (-1,+1), back-right (+1,-1), forward-right (+1,+1)

### LionHawk (`****h`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)
  - single step / leap: back-left (-2,-1), forward-left (-2,+1), back-left (-1,-2), forward-left (-1,+2), back-right (+1,-2), forward-right (+1,+2), back-right (+2,-1), forward-right (+2,+1)

### FreeEagle (`****e`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### ChariotSoldier (`****c`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### WaterBuffalo (`****w`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): back-left (-1,-1), left (-1,+0), forward-left (-1,+1), backward (+0,-1), forward (+0,+1), back-right (+1,-1), right (+1,+0), forward-right (+1,+1)

### VerticalSoldier (`****l`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), forward (+0,+1), right (+1,+0)
  - single step / leap: backward (+0,-1)

### SideSoldier (`****s`)

- Type: slider
- Moves & captures:
  - rides (repeats until blocked): left (-1,+0), forward (+0,+1), right (+1,+0)
  - single step / leap: backward (+0,-1)

## Pawns

- Forward stepper (Shogi-style single forward step)
- Double-step allowed from rank(s): 2
- En passant available

## Promotion

- Promotes to: Gold
- Promotion zone rank(s): 12, 13, 14, 15, 16
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

## Special mechanics

- Full Chu-Shogi Lion moves (igui, double capture, area move, pass)
- Tenjiku Fire Demon area burn
- Tenjiku range-jumping generals

