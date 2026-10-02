---
type: style-guide
title: Style guide
status: working
links: [concept.md, mechanics/dialogue-box.md]
implemented_in: [scenes/theme.tres, scripts/background.gd, scripts/character_portrait.gd, scripts/story_parser.gd]
---

# Style guide

The look the template really uses. Change these values in the files listed,
then update this card.

## Feel

Cozy and gentle, like a warm room on a rainy night. Soft gradients, round
shapes, big readable text. Friendly, simple language.

## Colors

- UI panels: deep indigo `#2a2540` (94% opaque) with a cream border `#fff8ec`.
- Accent (hover/focus buttons, the "▼", the ending name): gold `#ffd54f`.
- Text: cream `#fff8ec`; narration is softer lavender `#d6d0f0`.
- Backdrops (`BACKGROUNDS` in `scripts/story_parser.gd`, top to bottom):
  night `#141a3d` to `#34477a`, day, dawn, sunset `#3b2a5a` to `#f08a5d`,
  cafe `#4a2f2a` to `#c98a4b`, park, home, black.
- Characters: Mira `#ffa45c`, Theo `#6fb7e9`, You `#9db8ff` (`story/characters.txt`).

## Shapes and type

- Portraits are one big shape (circle, square, triangle or diamond) with a
  darker outline and a simple face; the speaker is bright, others dimmed.
- Panels have 18 px rounded corners and a 4 px cream border; buttons 14 px.
- Godot's default font: 28 px in general, 32 px for dialogue, 30 px for name
  plates, 88 px for the title.

## Motion

- Text is typed at 45 letters per second; the "▼" bobs when the line is done.
- Scenes fade through near-black in 0.5 s; backdrops blend in 0.8 s.
