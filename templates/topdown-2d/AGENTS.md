# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox, started from the **Top-down adventure**
template. These are instructions for the AI agent working on it
(`CLAUDE.md` imports this file).

## The game right now

Seen from above: the hero starts in a garden, reads a sign, talks to Moss
(a villager), walks east into a hall, grabs a key next to a wandering slime,
opens the locked door in the hall's north wall, and steps on the exit in the
vault behind it: "You escaped!". The slime costs a heart on touch; losing
all three hearts shows "Oh no!". Both screens restart with R or a button.

Controls (input actions in `project.godot`): `move_left/right/up/down`
(arrows, WASD, d-pad, left stick), `interact` (E, Space, Enter, gamepad A),
`restart` (R, gamepad Start).

## Project layout

- `project.godot` — the Godot project file. The main scene is
  `scenes/main.tscn`; the window is 1280×720 and scales with the window.
  Physics layers: 1 `world` (walls, doors, props, characters), 2 `player`,
  3 `enemies`, 4 `interactables` (talk zones).
- `scenes/main.tscn` + `scripts/main.gd` — the main scene and the game's
  rules: connects the player to the HUD, pauses the world while the dialogue
  box is open, shows the win / game-over screens. Its `World` node holds
  `Rooms`, `Props`, and every character and item; `Camera` and `HUD` sit
  next to it.
- `scenes/room.tscn` + `scripts/room.gd` — one room: floor, walls, and
  optional doorways (`doorway_north/south/east/west`) in the middle of a
  side. Tuning: `size`, `doorway_width`, `wall_thickness`, colors. Draws
  itself in the editor too (`@tool`) and builds its wall collisions at start.
- `scripts/camera.gd` (the `Camera` node) — follows the player smoothly but
  never shows past the edges of the current room (see "Camera" below).
  Tuning: `follow_speed`, `shake_decay`; `zoom` is set on the node (1.25).
- `scenes/player.tscn` + `scripts/player.gd` — the hero (`class_name
  Player`): 8-way movement with acceleration, hearts, keys, knockback and
  blinking after a hit, talking. Tuning: `max_speed`, `acceleration`,
  `friction`, `max_hearts`, `knockback_strength`, `knockback_time`,
  `invincible_time`. Its `InteractArea` finds things to talk to.
- `scenes/npc.tscn`, `scenes/sign.tscn` + `scripts/talkable.gd` — things
  you talk to with E. Each has `speaker_name` and `lines` (one page each),
  set per instance in `main.tscn`, a `TalkZone` area and an "E" `Prompt`.
- `scenes/key.tscn` + `scripts/key_pickup.gd` — a key; touching it gives
  the player one key. Tuning: `bob_height`.
- `scenes/locked_door.tscn` + `scripts/locked_door.gd` — blocks a doorway
  until the player touches it holding a key (uses the key, slides open).
  Tuning: `locked_text`.
- `scenes/enemy.tscn` + `scripts/enemy.gd` — the slime: wanders near its
  start and hurts the player on touch (its `Hitbox`). Tuning: `speed`,
  `wander_radius`, `min_wander_time`, `max_wander_time`, `rest_chance`.
  It can't be defeated yet.
- `scenes/exit.tscn` + `scripts/exit.gd` — the way out; touching it wins.
  Adds itself to the `exits` group, which `main.gd` listens to.
- `scenes/bush.tscn`, `scenes/rock.tscn` — solid decorations (no script).
- `scenes/hud.tscn` + `scripts/hud.gd` — everything on screen:
  `status_bar.gd` (hearts and key counter, drawn with shapes),
  `dialogue_box.gd` (typewriter text, E for the next page; tuning:
  `letters_per_second`), `end_screen.gd` (win / game over, restart).
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

All art is Godot shapes (`Polygon2D`, `Line2D`, drawing code): no image
files yet. Put new assets in `assets/`, new scenes in `scenes/` and scripts
in `scripts/`, and keep `res://` paths correct when moving files.

### Camera

The camera follows the player but stays inside the room the player is in,
so walking through a doorway slides the view to the next room, like classic
adventure games. With the 1.25 zoom the view is 1024×576 world pixels, so
the 1280×720 rooms scroll a little and the 1920-wide hall scrolls more. A
room smaller than the view is shown centered.

### How to add things

- **A room:** instance `scenes/room.tscn` under `World/Rooms`, set its
  `position` (top-left corner) and `size`, and turn on the doorways it
  needs. Doorways sit in the middle of a side, so line rooms up so that the
  middles of the shared sides meet (both rooms need the doorway). Give it
  its own floor colors so each room feels different.
- **A character or sign:** instance `scenes/npc.tscn` (or `sign.tscn`)
  under `World` and set `speaker_name` and `lines`. For a new look, make a
  new scene with the same structure (`CollisionShape2D`, `TalkZone` on layer
  4, `Visual`, `Prompt`) and `talkable.gd`.
- **An enemy:** instance `scenes/enemy.tscn` under `World` where it should
  roam; tune `speed` and `wander_radius` per instance.
- **An item:** copy the key's pattern: an `Area2D` with `collision_mask =
  2` whose `body_entered` checks `body is Player` and calls a function on
  the player (add that function and a signal the HUD can listen to).
- **Another locked door / key:** instance both; each key opens any one door.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`) describing what the game is and
how it should work. `concept.md` is the starting card; `mechanics/` has one
card per mechanic of this template and `characters/` the hero, Moss and the
slime.

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
