---
type: mechanic
title: Movement
status: working
implemented_in: [scripts/player.gd, scenes/player.tscn, project.godot]
---

# Movement

The player runs left and right with the arrow keys, A/D, or a gamepad's
d-pad or left stick. It speeds up and slows down quickly rather than
instantly, and has a little less grip in the air. The camera follows the
player smoothly and stays inside the level.

## Tuning values

In `scripts/player.gd` ("Running" group):

| Value | Default | What it does |
|---|---|---|
| `run_speed` | 320 | Top speed, pixels per second |
| `ground_acceleration` | 2400 | How fast it speeds up on the ground |
| `ground_friction` | 2800 | How fast it stops on the ground |
| `air_acceleration` | 1500 | Steering in the air (lower = floatier) |

Camera (`Camera2D` in `scenes/player.tscn`): `position_smoothing_speed`
6. Its limits come from the level's `size` (set by `scripts/main.gd`).

Juice: the player squashes when it lands and puffs dust
(`Dust` particles in `scenes/player.tscn`).

## How to change it

- Faster or slower: change `run_speed`.
- Slippery (ice): lower `ground_friction` and `ground_acceleration`.
- Tighter air control: raise `air_acceleration` to match the ground.
- Other keys or buttons: the `move_left`/`move_right` actions in
  `project.godot`'s `[input]` section.
