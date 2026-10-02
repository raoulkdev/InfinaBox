---
type: story
title: Story overview
status: working
links: [concept.md, mechanics/story-script.md, mechanics/choices-and-flags.md, mechanics/endings.md]
implemented_in: [story/intro.txt, story/cafe.txt, story/ending_friends.txt, story/ending_passing.txt]
---

# Story overview

"Lantern Café" is a short story you can change freely.

| Scene | File | What happens |
|---|---|---|
| intro | `story/intro.txt` | A rainy evening. You find a lost notebook and choose to peek inside or take it to Mira. |
| peek | `story/intro.txt` | You look at the drawings. This remembers that you peeked. |
| cafe | `story/cafe.txt` | Mira and Theo talk to you. You choose to hand the notebook back, joke, or leave. |
| joke, give_back, compliment, leave | `story/cafe.txt` | The reactions that lead to an ending. |
| ending_friends, ending_passing | `story/ending_*.txt` | The two endings. |

To add a scene, write `# scene: name` in any file in `story/` and point a choice at it.
