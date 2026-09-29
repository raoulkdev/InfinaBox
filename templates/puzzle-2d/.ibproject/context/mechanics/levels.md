---
type: mechanic
title: Levels
status: working
links: [mechanics/grid-movement.md, mechanics/pushing-blocks.md, mechanics/ending.md, style-guide.md]
implemented_in: [scripts/levels.gd, scripts/board.gd, scripts/main.gd, scripts/hud.gd]
---

# Levels

Five short rooms, played in order, each drawn as a text picture in
`scripts/levels.gd`. They get harder: a one-push tutorial, then two and
three blocks, then rooms where the order of pushes matters and a block can
get stuck in a corner (use undo!). The fewest possible moves for the five
rooms are 1, 10, 16, 29 and 39.

| # | Name | Blocks |
|---|---|---|
| 1 | First push | 1 |
| 2 | Side by side | 2 |
| 3 | Three in a row | 3 |
| 4 | Mind the corner | 3 |
| 5 | The long way round | 3 |

The HUD shows "Level 3 of 5 - Three in a row", the moves so far and the
best moves for the level this session. The best is kept in memory only, so
it starts fresh each time the game is launched.

## Level text

One line of text per row; each character is a square:

| Character | Meaning |
|---|---|
| `#` | wall |
| `.` | target |
| `$` | block |
| `*` | block already on a target |
| `@` | player start |
| `+` | player start on a target |
| space | floor |

A level needs exactly one player, as many blocks as targets, and walls all
the way round. Squares outside the walls are ignored. Rows can be different
lengths.

## Tuning values

- The level list is `LEVELS` in `scripts/levels.gd`.
- `fit_area` and `max_tile_size` in `scripts/board.gd` decide how large a
  room is drawn; rooms up to about 16x12 fit the window.

## How it works

`board.gd`'s `load_level(index)` reads the text, checks it (one player,
matching blocks and targets, printing a clear error if not), works out the
floor by walking outward from the player, and creates the `Block` nodes.
`main.gd` moves to the next index when the "Level complete!" panel is
confirmed.

## How to change it

- Add a level: add a `{"name": "...", "rows": [...]}` entry to `LEVELS`.
  Make sure it can be solved. The HUD, records and ending update themselves.
- Reorder or remove levels: reorder or delete entries in `LEVELS`.
- New kinds of squares: see "Add a new kind of square" in `AGENTS.md`.
