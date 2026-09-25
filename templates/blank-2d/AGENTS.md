# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox. These are instructions for the AI agent
working on it (`CLAUDE.md` imports this file).

## Project layout

- `project.godot` — the Godot project file. The main scene is `main.tscn`;
  the window is 1280×720.
- `main.tscn` — the main scene: a background and the player.
- `player.gd` — the player, a `CharacterBody2D` moved with the arrow keys.
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

Add new scenes, scripts, and assets wherever they fit best (for example
`scenes/`, `scripts/`, `assets/`), and keep `res://` paths correct when
moving files.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`) describing what the game is and
how it should work. `concept.md` is the starting card.

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
