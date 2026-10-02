---
type: mechanic
title: Endings
status: working
links: [concept.md, story/overview.md, mechanics/choices-and-flags.md]
implemented_in: [scripts/ending_screen.gd, story/ending_friends.txt, story/ending_passing.txt]
---

# Endings

A scene that says `end: Some Name` finishes the story and shows an ending
screen with that name. The template has two: "A New Friend" (you and Mira
become friends) and "Just Passing Through" (you leave). The player can start
over from the ending screen; flags reset.

## How to change it

Add a scene ending in `end: Your Ending Name`, then point a choice at it.
