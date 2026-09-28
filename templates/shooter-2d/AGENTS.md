# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox, started from the **Arena shooter**
template: a twin-stick shooter where the player survives growing waves of
enemies in a walled arena. These are instructions for the AI agent working
on it (`CLAUDE.md` imports this file).

## Project layout

- `project.godot` — the Godot project file. The main scene is
  `scenes/main.tscn`; the window is 1280×720 and scales with the window
  (`canvas_items` stretch). It defines the input actions (`move_left`,
  `move_right`, `move_up`, `move_down`, `aim_left`, `aim_right`, `aim_up`,
  `aim_down`, `shoot`, `restart`: keyboard, mouse and gamepad) and names the
  physics layers: 1 `walls`, 2 `player`, 3 `enemies`, 4 `bullets`.
- `scenes/main.tscn` + `scripts/game.gd` — the round: the arena (backdrop,
  tiled `Floor`, neon `Border`, and the `Walls` collision), the `Enemies`
  container, the `Player`, a fixed `Camera2D` and the `HUD`. `game.gd`
  starts waves, spawns enemies just inside the edge of `Arena/Floor` (away
  from the player), counts the score, shakes the camera, and pauses the
  game behind the game-over screen. Wave and screen-shake tuning lives here.
- `scenes/player.tscn` + `scripts/player.gd` (`class_name Player`) — the
  ship: movement, aiming (mouse, or right stick), shooting with a fire rate
  and muzzle flash, health, and the blink after a hit. The `Hurtbox` area
  detects touching enemies. Movement, weapon and health tuning lives here.
- `scenes/bullet.tscn` + `scripts/bullet.gd` (`class_name Bullet`) — one
  bullet: flies straight, damages the first enemy it touches, disappears on
  walls or after `lifetime`. The player sets its speed and damage.
- `scenes/enemy.tscn` + `scripts/enemy.gd` (`class_name Enemy`) — the
  **Chaser**: chases the player, is knocked back and flashes white when hit,
  bursts into particles when destroyed and emits `died` (the game adds its
  `score_value`). Enemy tuning is in `@export` vars at the top of the script.
- `scenes/brute.tscn` — the **Brute**: an inherited scene of `enemy.tscn`
  that only overrides numbers and shapes (slower, 4 health, 2 contact
  damage, 30 points, bigger orange hexagon). It appears from wave 3.
- `scenes/explosion.tscn` + `scripts/explosion.gd` (`class_name Explosion`)
  — the one-shot `CPUParticles2D` burst left by a destroyed enemy; it frees
  itself when done. Its amount, speed and size are set on the node.
- `scenes/hud.tscn` + `scripts/hud.gd` (`class_name Hud`) — score, wave,
  health bar, the "WAVE N" banner, the controls hint, and the game-over
  screen with the final score and a **Play again** button (R or Start also
  work). Its process mode is "Always" so it works while the game is paused.
- `assets/floor_grid.png` — the 64×64 floor tile (generated, no third-party
  art). Everything else is drawn with `Polygon2D`, `Line2D` and `ColorRect`.
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

Put new scenes in `scenes/`, scripts in `scripts/` and images or sounds in
`assets/`, and keep `res://` paths correct when moving files. Don't reuse
the class names above (`Player`, `Enemy`, `Bullet`, `Explosion`, `Hud`) for
new scripts.

### Design choices

- **Fixed camera.** The whole arena fits the screen, so the player always
  sees enemies coming from every edge; that's easiest for new players. The
  `Camera2D` only moves for screen shake (its `offset`). For a bigger arena,
  make `Camera2D` a child of `Player` (gentle follow: turn on its position
  smoothing) and move the walls and `Floor` out.
- **Colours**: neon on dark. Player cyan, bullets warm yellow, Chasers pink,
  Brutes orange, arena border magenta. Keep new things in this palette
  unless the game's style guide says otherwise.

### Where the tuning values live

All are `@export` vars at the top of their script (editable in the Godot
inspector too); a scene can override them (`brute.tscn` does).

- `scripts/player.gd` — `speed`, `acceleration`, `fire_rate`,
  `bullet_speed`, `bullet_damage`, `spread_degrees`, `max_health`,
  `invincibility_time`, `contact_push`.
- `scripts/enemy.gd` — `speed`, `max_health`, `contact_damage`,
  `score_value`, `knockback_per_hit`, `spin_speed`, `death_effect`.
- `scripts/game.gd` — `first_wave_size`, `wave_size_growth`,
  `spawn_interval`, `time_between_waves`, `brute_first_wave`,
  `brute_chance`, `wave_speed_growth`, `min_spawn_distance`, and the shake
  strengths `shake_on_player_hit`, `shake_on_kill`, `shake_fade`.
- `scripts/bullet.gd` — `lifetime`. `scripts/hud.gd` — `hint_time`.

### How to extend it

- **A new enemy type**: make an inherited scene of `scenes/enemy.tscn` (like
  `brute.tscn`) and change its numbers, shape and colours. For new
  behaviour (shooting, zig-zagging), give it its own script that
  `extends Enemy` and overrides `_physics_process`. Then add a
  `PackedScene` export for it in `game.gd` and pick it in `_spawn_enemy()`.
- **A new weapon**: the weapon is the "Weapon" group in `player.gd` plus
  `_shoot()`. A shotgun is `_shoot()` firing several bullets with different
  angles; a different projectile is a new bullet scene set as
  `bullet_scene`. Keep the fire rate in `fire_rate`.
- **A power-up**: an `Area2D` scene (mask: layer 2, `player`) dropped by
  `Enemy.die()` or spawned by `game.gd`; on `body_entered` with a `Player`
  it changes a value (e.g. `fire_rate`, or heals with `health` +
  `health_changed.emit(...)`) and frees itself. For a timed boost, start a
  `get_tree().create_timer(...)` that puts the value back.
- **New input**: add actions in Project Settings → Input Map (keyboard and
  gamepad), then read them with `Input.is_action_pressed(...)`.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`) describing what the game is and
how it should work. `concept.md` is the starting card, and
`mechanics/` has one card per core mechanic of this template (movement and
aiming, shooting, enemies and waves, health and score), each listing its
tuning values and the files that implement it.

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
