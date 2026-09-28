---
type: mechanic
title: Movement and aiming
status: working
implemented_in: [scripts/player.gd, scenes/player.tscn, project.godot]
---

# Movement and aiming

The ship moves in any direction with WASD, the arrow keys or the left
stick, easing up to speed and easing to a stop. It always points where it
aims: at the mouse pointer, or where the right stick points (moving the
mouse hands aiming back to the mouse). The arena walls stop it.

## Tuning values (`scripts/player.gd`)

- `speed` = 340 — top speed, pixels per second.
- `acceleration` = 2800 — how fast it speeds up and stops. Lower feels
  floaty (like ice), higher feels snappier.
- Stick dead zone: 0.2 (the `move_*`/`aim_*` actions in `project.godot`);
  the right stick takes over aiming past 0.3.

## How to change it

- Faster or slower ship: `speed`. Drifty ship: lower `acceleration`.
- A dash: a new `dash` input action, and in `_move()` briefly set
  `velocity` to the move direction × a dash speed.
- New keys or buttons: Project Settings → Input Map, actions `move_*` and
  `aim_*`.
