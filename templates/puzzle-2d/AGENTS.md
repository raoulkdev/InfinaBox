# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox, started from the Puzzle template.
These are instructions for the AI agent working on it (`CLAUDE.md` imports
this file).

## Project layout

- `project.godot` — the Godot project file. The main scene is
  `scenes/main.tscn`; the window is 1280×720 and scales with the window
  (`canvas_items` stretch). Input actions (`[input]`): `move_up`,
  `move_down`, `move_left`, `move_right` (arrows, WASD, d-pad, left stick),
  `undo` (Z, gamepad B), `restart` (R, gamepad Start) and `confirm` (Space,
  Enter, gamepad A — used by the start, "Level complete!" and ending
  screens).
- `scenes/main.tscn` + `scripts/main.gd` — the game. Children: `Background`,
  `Board` (with the `Player` inside), `Interface/HUD` and the three
  full-screen messages under `Screens`. `main.gd` is a small state machine
  (start screen → playing → "Level complete!" → … → ending → play again),
  keeps the best moves per level for the session (in memory only) and
  handles R.
- `scripts/levels.gd` — **every level, as a text grid.** The only place
  levels live (see "Levels" below).
- `scripts/board.gd` — the puzzle itself, attached to `Board`: reads a
  level, draws the floor, walls and targets, moves the player, pushes
  blocks, keeps the undo history and says when the level is solved. It
  draws in "board units" (a square is 64 wide) and scales/centers itself so
  any room up to about 16×12 fits the window.
- `scenes/player.tscn` + `scripts/player.gd` — the character (a blue rounded
  square with eyes). Draws itself, slides between squares, bumps into walls.
- `scenes/block.tscn` + `scripts/block.gd` — a pushable block (a crate).
  Draws itself, slides, turns green with a tick and a pop on a target. The
  board creates one per `$` in the level.
- `scenes/ui/hud.tscn` + `scripts/hud.gd` — "Level 2 of 5 - name", the move
  counter, the best for the level, and the key reminder.
- `scenes/ui/start_screen.tscn` (`scripts/start_screen.gd`),
  `scenes/ui/level_complete.tscn` (`scripts/level_complete.gd`),
  `scenes/ui/ending_screen.tscn` (`scripts/ending_screen.gd`) — the
  full-screen messages. All three extend `scripts/screen.gd` (fade-in, and
  they send `confirmed` on Space/Enter/A or a button click).
- `scripts/background.gd` — the gradient backdrop (drawn on `Background`).
- All art is drawn with Godot shapes (`draw_*` calls, `StyleBoxFlat`); there
  are no image files yet. Put new images/sounds in `assets/`.
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

Scripts use `preload()` constants for types (see the top of `main.gd`)
rather than `class_name`. Keep new scripts short, typed, commented in plain
language, with tuning values as `@export` vars at the top.

### The grid model

The game is a grid of squares. `board.gd` keeps: `_floor` (squares you can
stand on: everything reachable from the start that isn't a wall),
`_targets`, `_block_pos` (one `Vector2i` per block, matching the `Block`
nodes in `_blocks`) and `_player_pos`. Nothing is physics-based: a move is
checked against the grid, then the nodes are tweened to their new squares.
Undo works from `_history`, one entry per move (`{player, blocks}` from
before the move), so undo is unlimited and always exact.

## How to change common things

- **Add or change a level** — edit `scripts/levels.gd`. Each level is
  `{"name": "…", "rows": ["#####", "#@$.#", "#####"]}`; the levels play in
  the order of the `LEVELS` list. Characters: `#` wall, `.` target, `$`
  block, `*` block on a target, `@` player, `+` player on a target, space
  floor. A level needs one player, as many blocks as targets, closed walls,
  and must be solvable — play it (or reason it through) before keeping it.
  A broken level prints a clear error when loaded. Nothing else needs to
  change: the HUD ("Level 3 of 6"), best-move records and the ending all
  follow the list.
- **Room size** — automatic. `fit_area` and `max_tile_size` in
  `scripts/board.gd` set how much of the window a room may use.
- **Feel** — `slide_time` (seconds per step), `repeat_delay` (seconds
  between steps while a key is held), `solved_delay` in `scripts/board.gd`;
  `pop_size` / `pop_time` in `scripts/block.gd`; `bump_distance` in
  `scripts/player.gd`.
- **Colors** — floor, wall and target colors: the `Colors` exports in
  `scripts/board.gd`; block colors: exports in `scripts/block.gd`; player
  colors: `scripts/player.gd`; background gradient: `scripts/background.gd`;
  HUD and screen text/panel colors live in the scenes under `scenes/ui/`.
  The palette is described in `.ibproject/context/style-guide.md`; keep the
  two in sync.
- **Add a new kind of square** (ice, a one-way arrow, a key door, a second
  color of block…) — pick a free character in `levels.gd`'s legend, read it
  in `board.gd`'s `load_level()` (where the text is turned into `_walls`,
  `_targets`, `_block_pos`), give it a rule in `try_move()`, draw it in
  `_draw()`, and save whatever it changes in the `_history` entries so
  undo keeps working. Add a line for it to the legend at the top of
  `levels.gd` and to `mechanics/levels.md`.
- **Change the text on a screen** — edit the labels in `scenes/ui/*.tscn`;
  the ending's summary line is set in `scripts/ending_screen.gd`, the
  "Level complete!" numbers in `scripts/level_complete.gd`.
- **Sounds and better art** — put files in `assets/` and play/draw them
  from `block.gd`, `player.gd` and `board.gd` (a good spot for a "thud" is
  `try_move()`, and for a "ding" `Block.set_on_target()`).
- When editing `.tscn` files by hand, keep every `ExtResource`/`SubResource`
  id you use declared at the top of the file.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`) describing what the game is and
how it should work. `concept.md` is the starting card; the template's
mechanics each have a card in `mechanics/` (grid movement, pushing blocks,
undo and restart, levels, ending) with their tuning values, and
`style-guide.md` describes the palette.

- Read the relevant cards before making a change, so it fits the game.
- When you build or change something (a mechanic, a character, a level),
  create or update its card: what it is, its tuning values, and which
  scenes and scripts implement it.
- Keep cards short, factual, and in sync with the code.

Other folders in `.ibproject/`:

- `.ibx` — the InfinaBox project marker. Don't edit it.
- `chat/` — saved conversations, managed by InfinaBox. Don't edit them.

## InfinaBox tools

The `infinabox` MCP server gives you tools for this project. Use them rather
than guessing:

- **Context:** `list_context_cards`, `read_context_card`, `search_context`,
  `write_context_card`.
- **Game:** `run_game`, `stop_game`, `get_game_status`, `get_game_errors`,
  `get_game_output`. After changing scenes or scripts, run the game and check
  its errors before saying the change works.
- **History:** `list_snapshots` shows the saved versions of the project.
  InfinaBox saves a snapshot after each change on its own; don't make git
  commits yourself.
