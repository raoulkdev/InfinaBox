---
type: mechanic
title: Shooting
status: working
implemented_in: [scripts/player.gd, scripts/bullet.gd, scenes/bullet.tscn, scenes/player.tscn]
---

# Shooting

Holding the left mouse button (or the right trigger / right shoulder
button) fires bullets where the ship aims, at a fixed fire rate; pushing
the right stick far also fires, twin-stick style. Each shot has a small
random wobble and a muzzle flash. A bullet flies straight, damages and
knocks back the first enemy it touches, and disappears on walls or after
a while.

## Tuning values

`scripts/player.gd` ("Weapon" group):

- `fire_rate` = 8 — shots per second.
- `bullet_speed` = 950 — pixels per second.
- `bullet_damage` = 1 — a Chaser has 1 health, a Brute 4.
- `spread_degrees` = 3 — random wobble per shot.

`scripts/bullet.gd`: `lifetime` = 1.2 seconds.
`scripts/enemy.gd`: `knockback_per_hit` = 260 (Chaser), 110 (Brute).

## How to change it

- Faster shooting: `fire_rate`. Stronger bullets: `bullet_damage`.
- A shotgun: in `_shoot()`, fire several bullets with different angles.
- A new kind of bullet: a new bullet scene, set as the player's
  `bullet_scene`.
