---
type: mechanic
title: Enemies and hearts
status: working
implemented_in: [scripts/enemy.gd, scenes/enemy.tscn, scripts/player.gd, scripts/status_bar.gd, scripts/main.gd, scripts/end_screen.gd]
---

# Enemies and hearts

The hero has three hearts, shown in the top-left corner. The slime in the
hall wanders around its starting spot; touching it costs a heart, pushes the
hero away, flashes them red, shakes the screen a little, and makes them blink
(unhurtable) for a moment. With no hearts left the hero spins away and the
"Oh no!" screen appears; R or "Play again" restarts.

The slime can't be defeated yet.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `max_hearts` | 3 | `scripts/player.gd` |
| `knockback_strength` | 620 px/s | `scripts/player.gd` |
| `knockback_time` | 0.2 s | `scripts/player.gd` |
| `invincible_time` | 1.2 s | `scripts/player.gd` |
| slime `speed` | 90 px/s | `scripts/enemy.gd` |
| slime `wander_radius` | 220 px | `scripts/enemy.gd` |
| slime `min_wander_time` / `max_wander_time` | 0.8 s / 2.0 s | `scripts/enemy.gd` |
| slime `rest_chance` | 0.25 | `scripts/enemy.gd` |

## How to change it

- Easier game: raise `max_hearts` or `invincible_time`, lower slime `speed`.
- More enemies: instance `scenes/enemy.tscn` under `World`.
- A chasing enemy: in `enemy.gd`, head toward the player when they are
  within some distance instead of picking a random direction.
- Fighting back: give the player an attack (a short-lived `Area2D` in front
  of them) and add a `take_hit` function with health to `enemy.gd`.
- Healing: add a heart pickup like the key that raises `hearts` (up to
  `max_hearts`) and emits `hearts_changed`.
