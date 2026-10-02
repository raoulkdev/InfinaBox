---
type: mechanic
title: Movement
status: working
implemented_in: [scripts/player.gd, scenes/player.tscn, scripts/camera.gd, project.godot]
---

# Movement

The hero walks in eight directions with the arrow keys, WASD, the d-pad or
the left stick. Diagonals are as fast as straight lines. The hero speeds up
and slows down quickly rather than instantly, which feels smooth but still
responsive. Walls, doors, rocks, bushes and characters block the way.

The camera follows smoothly and stays inside the current room, so walking
through a doorway slides the view to the next room.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `max_speed` | 260 px/s | `scripts/player.gd` |
| `acceleration` | 2000 px/s² | `scripts/player.gd` |
| `friction` (stopping) | 2400 px/s² | `scripts/player.gd` |
| `follow_speed` (camera) | 7 | `scripts/camera.gd` |
| camera `zoom` | 1.25 | `Camera` node in `scenes/main.tscn` |

## How to change it

- Faster or slower hero: change `max_speed`.
- Slippery, ice-like movement: lower `acceleration` and `friction`.
- Show more of the world: lower the camera `zoom` (1.0 shows a whole
  1280×720 room at once).
- New keys or buttons: edit the `move_*` actions in `project.godot`'s
  `[input]` section (or Project Settings → Input Map in Godot).
