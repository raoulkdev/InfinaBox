---
type: mechanic
title: Story script
status: working
links: [mechanics/choices-and-flags.md, mechanics/endings.md, story/overview.md]
implemented_in: [scripts/story_parser.gd, scripts/story_runner.gd, story/characters.txt, story/intro.txt, story/cafe.txt]
---

# Story script

The whole story is written in plain text files in `story/`, in a tiny
format anyone can edit: `# scene: name`, `background: night`,
`Name: what they say`, `> narration`, `? choice -> scene`, `set flag`,
`if flag -> scene`, `goto scene`, `end: Ending name`. `//` starts a note to
yourself. The format is written at the top of every story file.

`StoryParser` reads every `.txt` in `story/` (the cast comes from
`characters.txt`) and checks the story fits together. `StoryRunner` walks it.
Mistakes never crash the game: each becomes a readable message with the file
and line ("There is no scene called "pek". Did you mean "peek"?"), shown on an
error screen and printed to the output as `[story] ...`.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `story_folder` | `res://story` | `scripts/main.gd` |
| `start_scene` | `intro` | `scripts/main.gd` |
| max choices per choice point | 4 | `MAX_CHOICES` in `scripts/story_parser.gd` |
| backdrop names | night, day, dawn, sunset, cafe, park, home, black | `BACKGROUNDS` in `scripts/story_parser.gd` |

## How to change it

- Edit or add lines in `story/*.txt`; keep scene names unique across files.
- A new kind of line: add it to `_parse_step` in the parser and handle its
  step type in `StoryRunner.next()` or `main.gd`.
