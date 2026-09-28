---
type: mechanic
title: Enemies and waves
status: working
implemented_in: [scripts/game.gd, scripts/enemy.gd, scenes/enemy.tscn, scenes/brute.tscn, scenes/explosion.tscn, scripts/explosion.gd]
---

# Enemies and waves

Enemies arrive in waves. A "WAVE N" banner shows, then enemies pop in one
by one just inside the arena edges (never right next to the player) and
chase the player. When every enemy of a wave is destroyed, the next, bigger
wave starts after a short pause. Destroyed enemies burst into particles
and shake the screen a little.

Two enemy types:

- **Chaser** (`scenes/enemy.tscn`, pink diamond): fast, 1 health.
- **Brute** (`scenes/brute.tscn`, orange hexagon): slow, 4 health, hurts
  more; appears from wave 3.

## Tuning values

`scripts/game.gd`:

- `first_wave_size` = 5, `wave_size_growth` = 3 — wave N has
  5 + (N − 1) × 3 enemies.
- `spawn_interval` = 0.55 s between enemies; `time_between_waves` = 2 s.
- `brute_first_wave` = 3; `brute_chance` = 0.25.
- `wave_speed_growth` = 0.06 — enemies 6% faster each wave.
- `min_spawn_distance` = 300 px from the player.
- `shake_on_kill` = 3 px.

`scripts/enemy.gd` (Chaser / Brute): `speed` 150 / 95, `max_health` 1 / 4,
`contact_damage` 1 / 2, `score_value` 10 / 30, `knockback_per_hit`
260 / 110, `spin_speed` 0.6 / 0.25.

## How to change it

- Easier or harder waves: `first_wave_size`, `wave_size_growth`,
  `wave_speed_growth`, `spawn_interval`.
- A new enemy type: an inherited scene of `scenes/enemy.tscn` (like the
  Brute), plus a script that `extends Enemy` for new behaviour; add it to
  `_spawn_enemy()` in `game.gd`.
