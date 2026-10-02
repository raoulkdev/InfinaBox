---
type: mechanic
title: Grid movement
status: working
links: [mechanics/pushing-blocks.md, mechanics/undo-and-restart.md, mechanics/levels.md]
implemented_in: [scripts/board.gd, scripts/player.gd, project.godot]
---

# Grid movement

The room is a grid of squares. The player moves exactly one square per key
press, up, down, left or right, with a quick smooth slide (no diagonals, no
physics). Holding a key keeps stepping. If the next square is a wall, or
outside the room, the player does not move and gives a small nudge instead
(a blocked press is not counted as a move).

Controls: arrow keys or WASD, the gamepad d-pad or left stick. They are the
`move_up`, `move_down`, `move_left` and `move_right` actions in
`project.godot`.

## Tuning values

In `scripts/board.gd`:

| Value | Default | What it does |
|---|---|---|
| `slide_time` | 0.12 | Seconds a step takes to slide |
| `repeat_delay` | 0.16 | Seconds between steps while a key is held |
| `fit_area` | Rect2(40, 96, 1200, 520) | The part of the window the room may use |
| `max_tile_size` | 96 | Biggest a square is drawn, in pixels |

In `scripts/player.gd`: `bump_distance` = 7 (how far the player leans into
a wall).

## How it works

`board.gd` keeps the player's square in `_player_pos` and the walkable
squares in `_floor` (everything reachable from the start that isn't a
wall). `try_move()` checks the target square against `_floor`, then tweens
the `Player` node. The board is drawn in units where a square is 64 wide and
then scaled and centered so any room up to about 16x12 fits the window.

## How to change it

- Faster or slower steps: `slide_time` and `repeat_delay`.
- Bigger or smaller rooms on screen: `max_tile_size` and `fit_area`.
- Different keys: the `[input]` section of `project.godot` (or Godot's
  Project Settings, Input Map).
