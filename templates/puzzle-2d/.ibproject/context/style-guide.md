---
type: style-guide
title: Style guide
status: working
links: [concept.md, mechanics/levels.md]
implemented_in: [scripts/board.gd, scripts/block.gd, scripts/player.gd, scripts/background.gd, scenes/ui/hud.tscn, scenes/ui/start_screen.tscn]
---

# Style guide

A cozy, friendly look built only from simple shapes: rounded squares, soft
shadows and a warm, calm palette. Everything is drawn with Godot's drawing
code and `StyleBoxFlat`; there are no image files. The default Godot font is
used everywhere, in large, readable sizes with a dark outline on the HUD.

## Palette (as used in the code)

| Use | Color | Where |
|---|---|---|
| Background (top to bottom) | `#3d3a63` to `#262444` | `scripts/background.gd` |
| Floor (two tones, checkerboard) | `#f2e9d8`, `#e9dec8` | `scripts/board.gd` |
| Walls: face, top edge, front edge | `#5b5f97`, `#7a7fc0`, `#434777` | `scripts/board.gd` |
| Target marker | `#f4a259` (orange ring and dot) | `scripts/board.gd` |
| Block (crate) and its border | `#e07a5f`, `#b5563f` | `scripts/block.gd` |
| Block on a target and its border | `#81b29a`, `#5a8f76` (white tick) | `scripts/block.gd` |
| Player body and border | `#4d9de0`, `#2f6fab` (white eyes) | `scripts/player.gd` |
| HUD text | `#fff3dc`; best-moves line `#f4c98b` | `scenes/ui/hud.tscn` |
| Screen panels (cream) | `#fff8ea`, text `#3d405b`, accents `#d9603f` | `scenes/ui/*.tscn` |
| Buttons | `#e07a5f` (hover `#ea8c73`, pressed `#c4654c`), white text | `scenes/ui/*.tscn` |

## Shapes and motion

- Squares are 64 units; blocks and the player are drawn inside them with a
  little margin, rounded corners and a soft drop shadow underneath.
- Movement slides in about 0.12 seconds with an ease-out; a block landing
  on a target pops to 125% and settles back; screens fade in over 0.25
  seconds.
- Walls show a lighter top edge and a darker front edge so rooms look
  slightly raised.

## Keep it consistent

New colors should come from this family: warm oranges and reds for things
you push or want, cool blues and purples for the world and you, green for
"done". When you change a color in code, update this card.
