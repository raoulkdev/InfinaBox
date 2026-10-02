---
type: mechanic
title: Jumping
status: working
implemented_in: [scripts/player.gd, scenes/player.tscn, scenes/objects/platform.tscn]
---

# Jumping

Space, W, Up, or a gamepad's A button jumps. Holding jump gives a full
jump; letting go early gives a short hop. Falling is faster than rising,
which feels snappy. Two forgiving tricks: **coyote time** (you can still
jump for a moment after running off a ledge) and a **jump buffer** (a jump
pressed just before landing still happens). Wooden planks (platforms with
`one_way` on) can be jumped up through from below.

## Tuning values

In `scripts/player.gd` ("Jumping" group):

| Value | Default | What it does |
|---|---|---|
| `jump_height` | 150 | Height of a full jump, pixels (about 145 in play) |
| `time_to_jump_peak` | 0.38 | Seconds to reach the top of a full jump |
| `fall_gravity_multiplier` | 1.7 | How much faster falling is than rising |
| `jump_release_multiplier` | 0.45 | Upward speed kept when jump is let go early |
| `max_fall_speed` | 900 | Fastest fall, pixels per second |
| `coyote_time` | 0.1 | Seconds after leaving a ledge that jump still works |
| `jump_buffer_time` | 0.12 | Seconds a jump press is remembered before landing |

Gravity and jump speed are computed from `jump_height` and
`time_to_jump_peak`, so they can be changed independently. A full jump at
full speed crosses a gap of about 210 pixels. The player stretches on
take-off and puffs dust.

## How to change it

- Higher jump: raise `jump_height` (then check that gaps and platforms in
  the levels still fit).
- Floatier: raise `time_to_jump_peak` and lower `fall_gravity_multiplier`.
- Double jump: count jumps in `player.gd`, reset the count on landing, and
  allow a jump while in the air if the count is below 2.
