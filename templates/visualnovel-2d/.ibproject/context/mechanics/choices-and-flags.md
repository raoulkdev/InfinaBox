---
type: mechanic
title: Choices and flags
status: working
links: [mechanics/story-script.md, mechanics/endings.md, story/overview.md]
implemented_in: [scripts/main.gd, scripts/story_runner.gd, scripts/story_parser.gd, story/intro.txt, story/cafe.txt]
---

# Choices and flags

A choice point is 2 to 4 lines starting with `?`, each ending in `-> scene`.
The game shows them as buttons over the scene; the player picks with the
mouse, the number keys 1 to 4, or arrows + Enter (gamepad A). The choice is
written into the history log.

A **flag** is a one-word memory: `set peeked` remembers it, `if peeked ->
compliment` jumps to a scene when it is set, `if not peeked -> ...` when it
is not. Flags reset when the story starts over.

In the template, the first choice (peek in the notebook or not) sets
`peeked`; later, `give_back` checks it and, if set, jumps to `compliment`.
The second choice (hand it back, joke, or leave) decides the ending.

## Tuning values

| Value | Default | Where |
|---|---|---|
| max choices per point | 4 | `MAX_CHOICES` in `scripts/story_parser.gd` |
| button size | 760 x 64 | `_show_choices` in `scripts/main.gd` |

## How to change it

Add `?` lines to a scene and write the scenes they point to. To remember
something, `set` a flag and test it with `if` in a later scene.
