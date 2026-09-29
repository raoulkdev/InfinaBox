---
type: mechanic
title: Undo and restart
status: working
links: [mechanics/grid-movement.md, mechanics/pushing-blocks.md, mechanics/levels.md]
implemented_in: [scripts/board.gd, scripts/main.gd, project.godot]
---

# Undo and restart

**Undo** (Z, or B on a gamepad) takes back the last move, as many times as
you like, all the way to the start of the level. Holding the key keeps
undoing. The move counter goes down with each undo, so it always shows the
number of moves in the position you are looking at. Undo does nothing when
there is nothing to take back, and can't be used once the level is solved.

**Restart** (R, or Start on a gamepad) puts the level back the way it began
with the counter at 0. It works while playing and also from the "Level
complete!" panel, to have another go at a better score.

## Tuning values

- `repeat_delay` in `scripts/board.gd` (0.16 seconds) sets how fast held
  Undo repeats, the same as held movement.
- There is no limit on undo. The history is a list, one entry per move.

## How it works

Before every move `board.gd` saves `{player, blocks}` (positions only) to
`_history`. `undo()` pops the last entry, puts the player and blocks back
and slides their nodes to the old squares. The moves shown are just
`_history.size()`. Restart is handled in `main.gd`: it reloads the current
level on the board (`load_level()`), which clears the history.

## How to change it

- Limit undos (for example 5 per level): count them in `undo()` in
  `scripts/board.gd` and refuse when the limit is reached.
- Make undo cost a move: keep a separate counter instead of using
  `_history.size()`.
- Different keys: the `undo` and `restart` actions in `project.godot`.
