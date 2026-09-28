---
type: mechanic
title: Exit and winning
status: working
implemented_in: [scripts/exit.gd, scenes/exit.tscn, scripts/main.gd, scripts/end_screen.gd, scenes/hud.tscn]
---

# Exit and winning

The glowing swirl marked "EXIT" in the vault (behind the locked door) is the
way out. Stepping on it pauses the game and shows "You escaped!" with a
"Play again" button; R (or gamepad Start) restarts too.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `spin_speed` | 0.35 turns/s | `scripts/exit.gd` |
| exit position | (2240, -380) | `Exit` in `scenes/main.tscn` |
| win text | "You escaped!" / "You found the key and made it out." | `_on_exit_reached` in `scripts/main.gd` |

## How to change it

- Move the exit: change the `Exit` node's position.
- Change the win message: edit the two strings in `main.gd`.
- A next level instead of a win screen: in `main.gd`'s `_on_exit_reached`,
  call `get_tree().change_scene_to_file("res://scenes/level_2.tscn")`.
