---
type: mechanic
title: Goal and level complete
status: working
implemented_in: [scripts/goal_flag.gd, scenes/objects/goal_flag.tscn, scripts/main.gd, scripts/level_complete.gd, scenes/ui/level_complete.tscn]
---

# Goal and level complete

A tall waving flag at the end of each level (x = 4000 in level 1).
Reaching it throws confetti, stops the player, and shows the "Level
complete!" screen with the coins found. Its button goes to the next level
(or says "Play again" on the last one). R, or Start on a gamepad, restarts
the current level at any time.

## Tuning values

In `scripts/goal_flag.gd`:

| Value | Default | What it does |
|---|---|---|
| `wave_amount` | 0.12 | How much the flag waves |
| `wave_speed` | 1.2 | Waves per second |

The confetti is the `Confetti` particles in `goal_flag.tscn`; the screen's
text, colors and button are in `scenes/ui/level_complete.tscn`.

## How it works

The flag is in the `goal` group and emits `reached`. `main.gd` turns off
the player's controls and calls `level_complete.gd`'s `show_result()`.
The level order is the `levels` list on `Main` in `scenes/main.tscn`;
`load_level()` swaps levels and resets coins and checkpoints.

## How to change it

- More levels: see "Add a level" in `AGENTS.md`.
- A timer or star rating: track time in `main.gd` and pass it to
  `show_result()`.
