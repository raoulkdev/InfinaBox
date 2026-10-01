# {{PROJECT_NAME}}

A Godot 4 2D game made with InfinaBox. These are instructions for the AI
agent working on it (`CLAUDE.md` imports this file).

This project is the foundation of a **full-size game**, not a quick prototype.
There is no gameplay yet on purpose: the game is designed with the developer
first and built piece by piece. Work in small, agreed steps, and keep the
systems below clean, because every later feature sits on them.

## How the game is organised

- `project.godot`: the main scene is `scenes/boot.tscn`. The window is
  1280x720. Input action `pause` (Esc, gamepad Start). Physics layers
  1 `world`, 2 `player`, 3 `enemies`, 4 `interactables`. New input actions are
  added here by name (`jump`, `interact`), never by raw key in a script.
- `core/` holds the game's **autoloads**, the few scripts that exist for the
  whole run of the game:
  - `Events` (`events.gd`): the message board. Systems announce what
    happened with a signal; others listen. Parts never call each other
    directly when a signal will do. Add a signal here when two systems need
    to talk, named for what happened.
  - `GameState` (`game_state.gd`): the flow (`BOOT`, `MENU`, `PLAYING`,
    `PAUSED`), starting a new game, pausing. `FIRST_LEVEL` names the level a
    new game opens.
  - `SceneRouter` (`scene_router.gd`): the only place scenes are changed
    (`SceneRouter.go_to(path)`), so a loading screen or fade has one home.
  - `SaveSystem` (`save_system.gd`): JSON saves in `user://saves/`, versioned.
    Anything saved joins group `saveable` and has `save_key()`, `save_data()`
    and `load_data(data)`.
  - `UserSettings` (`user_settings.gd`): volume and fullscreen, kept in
    `user://settings.cfg`.
- `scenes/boot.tscn` starts the game and opens the menu; `ui/main_menu.tscn`
  is the menu; `levels/sandbox.tscn` is the empty level to build in.
- `entities/`, `systems/`, `data/`, `assets/`, `levels/`, `ui/` each have a
  `README.md` saying what goes there. Put new things where they say.
- `addons/infinabox/` is installed and updated by the InfinaBox app and
  registered as the `InfinaBox` autoload. **Don't edit or remove it.**
- `.ibproject/` is InfinaBox's data for this game; `.godot/` is Godot's cache.
  Never edit either by hand (Context cards are the exception, see below).

## Building rules

- **One job per script.** A script that grows past a couple of hundred lines
  or does two unrelated things is split. No "god" script that knows everything.
- **Scenes are the building blocks.** Build things as scenes that can be
  placed many times; configure them with `@export` variables, not constants
  buried in code.
- **Content is data.** Numbers, lists and text that a designer would tune
  (enemy health, item prices, dialogue) live in files under `data/`
  (a `Resource` script or JSON), not in scripts.
- **Talk through `Events`**, not through long chains of `get_node` paths.
  Reach other objects by groups or signals.
- **Type your GDScript** (`var speed: float = 200.0`, `func hit(damage: int) -> void`)
  and comment the *why* in plain language.
- **Keep the game running.** After each change run the game and check for
  errors before saying it works.
- New systems get a Context card in `systems/` (see below) in the same turn
  they are built.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`, or `other`) describing what the
game is and how it works. The Documents page in InfinaBox shows them.

- `concept.md` is the game's definition (pitch, pillars, feel). It starts
  mostly empty. **It is filled in with the developer, not for them.**
- `style-guide.md` is the look and sound direction, also filled in together.
- `tasks/` holds the pre-production tasks and later the game's to-do list.
- `systems/` explains the foundation above in plain words, with a technical
  section for each system. **Keep it in sync with the code**: when a system
  changes or a new one is added, update or add its card.
- Read the relevant cards before changing something, and keep what the
  developer wrote.

## InfinaBox tools

The `infinabox` MCP server gives you tools for this project:

- **Context:** `list_context_cards`, `read_context_card`, `search_context`,
  `write_context_card`.
- **Game:** `run_game`, `stop_game`, `get_game_status`, `get_game_errors`,
  `get_game_output`.
- **History:** `list_snapshots`. InfinaBox saves a snapshot after each of your
  turns that changes files; don't make git commits yourself.
