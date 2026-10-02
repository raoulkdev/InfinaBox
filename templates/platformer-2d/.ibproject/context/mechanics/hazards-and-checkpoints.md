---
type: mechanic
title: Hazards and checkpoints
status: working
implemented_in: [scripts/spikes.gd, scenes/objects/spikes.tscn, scripts/checkpoint.gd, scenes/objects/checkpoint.tscn, scripts/player.gd, scripts/level.gd]
---

# Hazards and checkpoints

Spikes and bottomless pits are dangerous: touching spikes, or falling
below the bottom of the level, makes the player burst and reappear at the
last checkpoint reached (or the level's start). Checkpoints are small flags
that turn from grey to green when the player walks past them. Coins
already collected stay collected.

Level 1 has three rows of spikes (3, 12 and 2 spikes), two pits, and two
checkpoints (x = 1400 and x = 2950).

## Tuning values

| Value | Where | Default | What it does |
|---|---|---|---|
| `respawn_delay` | `scripts/player.gd` | 0.6 | Seconds before reappearing |
| `count` | `scripts/spikes.gd` | 3 | Spikes in a row |
| `spike_size` | `scripts/spikes.gd` | 32 × 28 | Size of one spike, pixels |
| `inactive_color` / `active_color` | `scripts/checkpoint.gd` | grey / green | Checkpoint flag colors |
| fall limit | `scripts/player.gd` `set_camera_limits()` | 64 | Pixels below the level before a fall counts |

The spikes' hit box is a little smaller than their drawing, so only a real
touch counts.

## How it works

Hazards call `die()` on a body in the `player` group. `die()` hides the
player, plays the `Burst` particles, waits `respawn_delay`, then calls
`respawn()`, which moves the player to `respawn_point`. A checkpoint sets
`respawn_point` with `set_checkpoint()` the first time the player touches it.

## How to change it

- New hazard (lava, saw, enemy): an `Area2D` with `collision_layer = 0`,
  `collision_mask = 2` whose `body_entered` calls `body.die()` for bodies
  in the `player` group.
- Lives or a health bar: count deaths from the player's `died` signal in
  `main.gd`.
