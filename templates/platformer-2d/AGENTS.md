# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox, started from the Platformer template.
These are instructions for the AI agent working on it (`CLAUDE.md` imports
this file).

## Project layout

- `project.godot` — the Godot project file. The main scene is
  `scenes/main.tscn`; the window is 1280×720 and scales with the window
  (`canvas_items` stretch). Input actions (`[input]`): `move_left`,
  `move_right`, `jump`, `restart` — keyboard (arrows, A/D, Space/W/Up, R)
  and gamepad (d-pad, left stick, A, Start). Physics layers: 1 `world`
  (platforms), 2 `player`.
- `scenes/main.tscn` + `scripts/main.gd` — the game: background, player,
  HUD and "Level complete!" screen. `main.gd` loads the current level from
  its `levels` list, counts coins, handles R (restart) and the goal.
- `scenes/player.tscn` + `scripts/player.gd` — the player
  (`CharacterBody2D`, in group `player`, origin at its feet). Running,
  jumping (coyote time, jump buffer, variable height), squash/stretch,
  dust and death particles, respawning, and the following `Camera2D`. All
  movement tuning is `@export` vars at the top of `player.gd`.
- `scenes/levels/level_1.tscn` + `scripts/level.gd` — the level. Its
  `size` (4200×720) sets the camera limits; falling 64 px below its
  bottom kills the player. Children: `PlayerStart` (Marker2D),
  `LeftEdge`/`RightEdge` (invisible walls), and the folders `Platforms`,
  `Hazards`, `Coins`, `Checkpoints`, plus `GoalFlag`.
- `scenes/objects/` — the level's building blocks, each with its script
  in `scripts/`:
  - `platform.tscn` (`platform.gd`, `@tool`) — ground block or, with
    `one_way`, a plank you can jump up through. Position = **top-left
    corner**; `size` sets width/height. Draws itself; colors are exports.
  - `spikes.tscn` (`spikes.gd`, `@tool`) — a row of `count` spikes.
    Position = **bottom-left corner** (put it on a platform's top edge).
  - `coin.tscn` (`coin.gd`) — in group `coins`, emits `collected`.
  - `checkpoint.tscn` (`checkpoint.gd`) — sets the player's respawn point.
    Position = bottom of the pole.
  - `goal_flag.tscn` (`goal_flag.gd`) — in group `goal`, emits `reached`.
    Position = bottom of the pole.
- `scenes/background.tscn` — sky gradient (`CanvasLayer`), clouds and two
  rows of hills in `Parallax2D` nodes (`scripts/clouds.gd`,
  `scripts/hills.gd`, both `@tool`, colors/heights are exports).
- `scenes/ui/hud.tscn` (`scripts/hud.gd`) — coin counter and the controls
  hint. `scenes/ui/level_complete.tscn` (`scripts/level_complete.gd`) —
  the end screen; its button goes to the next level or plays again.
- All art is drawn with Godot shapes (`Polygon2D`, `_draw()`); there are no
  image files yet. Put new images/sounds in `assets/`.
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

Scripts use `preload()` constants for types (see the top of `main.gd`)
rather than `class_name`, and duck typing through groups
(`body.is_in_group("player")`). Keep new scripts short, typed, commented
in plain language, with tuning values as `@export` vars at the top.

### Why the level is built from scenes, not a TileMap

Every platform, hazard and coin is its own node with a readable position
and size in `level_1.tscn`, so a level can be changed by editing a few
lines of text. (A `TileMapLayer` stores its tiles as packed binary data
that can't be edited reliably by hand.) Keep building levels this way.

## How to change common things

- **Movement feel** — the `@export` vars in `scripts/player.gd`
  (`run_speed`, `jump_height`, `time_to_jump_peak`, …). Gravity is
  computed from jump height and time to the peak.
- **Add a platform** — in `scenes/levels/level_1.tscn`, add under
  `Platforms`:
  `[node name="MyPlatform" parent="Platforms" instance=ExtResource("2_platform")]`
  with `position = Vector2(x, y)` (top-left), `size = Vector2(w, h)` and
  optionally `one_way = true` (planks look best 20 px tall). The ground's
  top is at y = 600; a full jump rises about 145 px and crosses about
  210 px of gap at full speed (keep gaps at 200 px or less).
- **Add a coin** — under `Coins`, an instance of `coin.tscn` (id
  `3_coin`) with a unique name and a position (its center). The counter
  counts coins automatically.
- **Add a hazard** — under `Hazards`, an instance of `spikes.tscn` (id
  `4_spikes`) with `position` on a platform's top edge and `count`. For
  a new kind of hazard, make an `Area2D` (`collision_layer = 0`,
  `collision_mask = 2`) that calls `body.die()` when a body in group
  `player` enters.
- **Add a checkpoint** — under `Checkpoints`, an instance of
  `checkpoint.tscn` (id `5_checkpoint`) on a platform's top edge.
- **Add a level** — duplicate `scenes/levels/level_1.tscn` (e.g.
  `level_2.tscn`, root renamed `Level2`), change it, and add it to the
  `levels` list of `Main` in `scenes/main.tscn` (an `ext_resource` plus
  `levels = Array[PackedScene]([ExtResource("2_level1"), ExtResource("…")])`).
  Every level needs `PlayerStart`, a `GoalFlag`, and its `size`.
- **Colors** — each object's colors are `@export` vars or `Polygon2D`
  `color`s in its scene; the sky is the `Gradient_sky` resource in
  `scenes/background.tscn`.
- When editing `.tscn` files by hand, keep every `ExtResource`/`SubResource`
  id you use declared at the top of the file.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`) describing what the game is and
how it should work. `concept.md` is the starting card; the template's
mechanics each have a card in `mechanics/` (movement, jumping, coins,
hazards and checkpoints, goal) with their tuning values.

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
