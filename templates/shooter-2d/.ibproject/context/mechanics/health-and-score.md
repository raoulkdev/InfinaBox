---
type: mechanic
title: Health, score and game over
status: working
implemented_in: [scripts/player.gd, scripts/game.gd, scripts/hud.gd, scenes/hud.tscn]
---

# Health, score and game over

The player has a health bar. An enemy touching the ship takes away health,
shoves that enemy away, shakes the screen, and the ship blinks and can't
be hurt again for a moment. Each destroyed enemy adds its points to the
score. At zero health the game freezes behind a "GAME OVER" screen showing
the final score and the wave reached; **Play again**, R or the gamepad's
Start button starts a new round.

## Tuning values

`scripts/player.gd`:

- `max_health` = 5.
- `invincibility_time` = 1.0 s of blinking safety after a hit.
- `contact_push` = 520 — how hard a touching enemy is shoved away.

`scripts/game.gd`: `shake_on_player_hit` = 14 px, `shake_fade` = 45 px/s.
`scripts/enemy.gd`: `contact_damage` (1 Chaser, 2 Brute) and `score_value`
(10 Chaser, 30 Brute).
`scripts/hud.gd`: `hint_time` = 7 s for the controls hint.

## How to change it

- Tougher player: raise `max_health` or `invincibility_time`.
- A best score: keep it in `game.gd` and save it to a file in `user://`
  (for example with `ConfigFile`), then show it on the game-over screen.
- Healing: a power-up that raises `health` and calls
  `health_changed.emit(health, max_health)` so the bar updates.
