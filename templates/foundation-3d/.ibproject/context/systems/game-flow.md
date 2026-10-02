---
type: other
title: "Game flow"
status: draft
---

# Game flow

**In plain words.** The game is always in one of four states: starting up,
in the main menu, playing, or paused. One helper keeps track, so nothing else
has to guess. Pressing Esc while playing pauses and unpauses.

## Technical details

`core/game_state.gd`, autoload `GameState`. `Flow` enum: `BOOT`, `MENU`,
`PLAYING`, `PAUSED`. `show_menu()`, `start_new_game()` and `set_paused()` are
the only ways the flow changes. `FIRST_LEVEL` is the scene a new game opens;
the script runs with `PROCESS_MODE_ALWAYS` so it can unpause. The `pause`
input action is defined in `project.godot`.
