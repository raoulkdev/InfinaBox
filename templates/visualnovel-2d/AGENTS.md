# {{PROJECT_NAME}}

A Godot 4 game made with InfinaBox, started from the **Visual novel**
template. These are instructions for the AI agent working on it
(`CLAUDE.md` imports this file).

## The game right now

A start screen ("Lantern Café": Start, Quit), then a short story told in
scenes: a rainy evening, a lost notebook, Mira the barista and Theo the
artist. The player reads dialogue (typed out letter by letter), makes two
choices, and reaches one of **two endings** ("A New Friend" or "Just
Passing Through"). The ending screen names the ending and offers Play again.
Whether you peeked in the notebook changes what you can say later.

Controls (input actions in `project.godot`): `advance` (Space, Enter, click,
gamepad A) finishes the line being typed, then goes to the next; `history`
(H, gamepad Y) opens the history log. Choices are picked with the mouse, the
number keys 1 to 4, or arrows + Enter.

## The story lives in plain text

Everything the characters say is in `story/*.txt`, not in code. Edit those
files to change the story. The format (also written at the top of every
story file):

```
// a note to yourself; ignored
# scene: cafe            starts a scene (letters, numbers, _ only)
background: night        a name (night, day, dawn, sunset, cafe, park,
                         home, black), a color (#336699) or two (#336699 #99ccff)
Mira: Hello!             a character speaks (Name must be in characters.txt)
> Rain taps the window.  narration (no speaker)
? Say hi -> hello        a choice; 2 to 4 in a row; jumps to scene "hello"
set kind                 remember "kind"
if kind -> nice          jump to scene "nice" if "kind" was set
if not kind -> rude      ...or if it was not set
goto cafe                jump straight to another scene
end: Good Ending         finish the story with this ending name
```

Rules: every scene must finish with a choice, a `goto` or an `end:`; every
scene named after an arrow must exist; the story starts at the scene called
`intro` (`start_scene` in `scripts/main.gd`). Mistakes never crash the game:
`scripts/story_parser.gd` collects readable messages ("story/cafe.txt, line
12: There is no scene called "pek". Did you mean "peek"?"), the game shows
them on an error screen and prints them to the output. After editing the
story, run the game and check `get_game_output` for `[story]` lines.

`story/characters.txt` lists the cast: `Name: #color shape side` (shape:
circle, square, triangle, diamond or none; side: left, center, right). A
speaker not listed there still works, with a grey name plate and no portrait.

Files are split by topic only for the person's convenience: any `.txt` in
`story/` (except `characters.txt`) is read, scene names just have to be
unique across all of them. If the game is exported, add `*.txt` to the
export's "include resources" filter so the story files are packed.

## Project layout

- `project.godot` — the Godot project file. The main scene is
  `scenes/main.tscn`; the window is 1280×720 and scales with the window.
- `story/` — the story (see above): `intro.txt` (the café door, the
  notebook), `cafe.txt` (Theo, the choices, the compliment), `ending_friends.txt`,
  `ending_passing.txt`, and `characters.txt`.
- `scenes/main.tscn` + `scripts/main.gd` — the conductor: loads the story,
  shows the start screen, asks the runner for the next event and shows it
  (backdrop, line, choices, ending), fades between scenes, handles input.
  Tuning (`@export` at the top): `story_folder`, `start_scene`, `game_title`,
  `game_subtitle`, `fade_time`, `background_time`.
- `scripts/story_parser.gd` (`StoryParser`) — reads the text files into
  scenes and steps, reports mistakes, holds the named backdrops
  (`BACKGROUNDS`).
- `scripts/story_runner.gd` (`StoryRunner`) — walks the steps, keeps the
  flags, follows `if`/`goto`/choices. Knows nothing about the screen.
- `scenes/dialogue_box.tscn` + `scripts/dialogue_box.gd` — the box: name
  plate in the speaker's color, typewriter text, bouncing "▼" (drawn as a
  triangle). Tuning: `text_speed` (letters per second, 0 = instant).
- `scripts/stage.gd` + `scripts/character_portrait.gd` (`CharacterPortrait`)
  — portraits made from a colored shape and a face; the speaker is bright
  and bobs, others are dimmed. Tuning: `highlight_time`.
- `scripts/background.gd` — the gradient backdrop and its color blend.
- `scenes/title_screen.tscn`, `scenes/ending_screen.tscn`,
  `scenes/history_log.tscn`, `scenes/error_screen.tscn` (+ scripts of the
  same names) — the start screen, the ending screen, the history log (H) and
  the story-problem screen.
- `scenes/theme.tres` — the look of buttons, panels and text.
- `addons/infinabox/` — the InfinaBox addon, installed and updated by the
  InfinaBox app and registered as the `InfinaBox` autoload. **Don't edit or
  remove it**; changes are overwritten. It prints `[infinabox] ready
  <version>` when the game starts in a debug build.
- `.ibproject/` — InfinaBox's data for this game (see below).
- `.godot/` — Godot's cache. Ignored by git; never edit it.

All art is Godot shapes and colors: no image files yet. Put new assets in
`assets/`, and keep `res://` paths correct when moving files.

### How to add things

- **A scene:** add `# scene: name` and its lines to a story file, then make
  something jump to it (a choice, a `goto` or an `if`). End it with a choice,
  a `goto` or an `end:`.
- **A character:** add a line to `story/characters.txt` (give it a color, a
  shape and a side different from who it shares scenes with), then write
  `Name: line` in the story. Add a card in `.ibproject/context/characters/`.
- **An ending:** write a scene that finishes with `end: Ending Name`, and
  make a choice or `if` lead to it. The ending screen counts endings by
  name automatically.
- **Remember a choice:** `set flag_name` in the scene, then `if flag_name ->
  scene` (or `if not flag_name -> scene`) later.
- **Real portraits or backgrounds:** put PNGs in `assets/`; portraits are
  drawn by `character_portrait.gd` (swap `_draw` for a texture), backdrops by
  `background.gd`.

## Context: the model of the game

`.ibproject/context/` holds the game's **Context**: Markdown cards with YAML
front-matter (`type: concept`, `mechanic`, `character`, `level`, `story`,
`asset`, `style-guide`, `task`, `playtest`; plus `title`, `status`, `links`
and `implemented_in`) describing what the game is and how it should work.
`concept.md` is the starting card; `style-guide.md` is the look; `mechanics/`
has one card per mechanic of this template; `characters/` one per character;
`story/overview.md` lists the scenes and endings.

- Read the relevant cards before making a change, so it fits the game.
- When you build or change something (a mechanic, a character, a scene),
  create or update its card: what it is, its tuning values, and which
  scenes and scripts implement it. When the story changes, update
  `story/overview.md` and the character cards.
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
