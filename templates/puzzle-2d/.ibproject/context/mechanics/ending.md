---
type: mechanic
title: Start, level-complete and ending screens
status: working
links: [mechanics/levels.md, mechanics/undo-and-restart.md, style-guide.md]
implemented_in: [scripts/main.gd, scripts/screen.gd, scripts/start_screen.gd, scripts/level_complete.gd, scripts/ending_screen.gd, scenes/ui/start_screen.tscn, scenes/ui/level_complete.tscn, scenes/ui/ending_screen.tscn]
---

# Start, level-complete and ending screens

The game has a beginning, a middle and an end:

1. **Start screen** - the game's name (the project's name) and a blinking
   "Press Space to start", with the room of level 1 faintly behind it.
2. **Playing** - the levels in order.
3. **"Level complete!"** - after each level: the moves it took and either
   "New best!" or "Best so far: N" for this session. The button (Space,
   Enter, gamepad A, or a click) goes to the next level; on the last level
   it says "Finish". R replays the level instead.
4. **Ending** - "You solved them all!" with the total moves of this run
   through all the levels, and a **Play again** button (or Space) that
   starts over from level 1. Best-move records carry over between runs
   until the game is closed.

## Tuning values

| Value | Default | Where | What it does |
|---|---|---|---|
| `open_time` | 0.25 | `scripts/screen.gd` | Fade-in time of every screen, seconds |
| `blink_time` | 0.9 | `scripts/start_screen.gd` | One blink of "Press Space to start" |
| `solved_delay` | 0.45 | `scripts/board.gd` | Wait after the last block lands before the panel |

## How it works

`main.gd` has a small state machine (`START`, `PLAYING`, `LEVEL_COMPLETE`,
`ENDING`) and turns the board's input on only while playing. The three
screens share `scripts/screen.gd`: they fade in and emit `confirmed`;
`main.gd` decides what comes next. Records live in `main.gd`'s `_best`
dictionary (level index to fewest moves), in memory only.

## How to change it

- Words: edit the labels in the three scenes under `scenes/ui/`.
- Save records between launches: write `_best` to a file in `user://` in
  `main.gd`'s `_on_solved()` and read it back in `_ready()`.
- A credits screen: add a scene like `ending_screen.tscn`, and show it from
  `_on_play_again_confirmed()` in `main.gd`.
