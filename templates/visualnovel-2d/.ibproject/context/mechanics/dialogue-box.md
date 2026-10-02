---
type: mechanic
title: Dialogue box
status: working
links: [mechanics/story-script.md, style-guide.md]
implemented_in: [scenes/dialogue_box.tscn, scripts/dialogue_box.gd, scripts/stage.gd, scripts/character_portrait.gd, scripts/history_log.gd, scripts/main.gd]
---

# Dialogue box

Each line appears in a box at the bottom of the screen, with a name plate in
the speaker's color (none for narration) and the words typed out letter by
letter. Click, Space, Enter or gamepad A shows the whole line at once; the
next press moves on. A small yellow "▼" bobs when the line is complete.

The speaker's placeholder portrait (a colored shape with a face) stands on
their side of the screen, bright and bobbing, while others are dimmed.
Narration dims everyone. H (or the History button) opens a log of everything
said and chosen so far; the story pauses while it is open.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `text_speed` | 45 letters per second (0 = instant) | `scripts/dialogue_box.gd` |
| `fade_time` | 0.5 s | `scripts/main.gd` |
| `background_time` | 0.8 s | `scripts/main.gd` |
| `highlight_time` | 0.2 s | `scripts/character_portrait.gd` |
| portrait positions | left 20%, center 50%, right 80% of the width | `SLOT_X` in `scripts/stage.gd` |

## How to change it

- Character look: colors, shapes and sides are in `story/characters.txt`;
  faces are drawn in `character_portrait.gd`.
- Box look: `scenes/dialogue_box.tscn` and `scenes/theme.tres`.
