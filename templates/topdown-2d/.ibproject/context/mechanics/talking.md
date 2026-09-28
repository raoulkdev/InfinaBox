---
type: mechanic
title: Talking
status: working
implemented_in: [scripts/talkable.gd, scenes/npc.tscn, scenes/sign.tscn, scripts/dialogue_box.gd, scenes/hud.tscn, scripts/player.gd, scripts/main.gd]
---

# Talking

Walk up to a character or a sign and an "E" bubble appears above it. Press
E (or Space, Enter, gamepad A) to open the dialogue box at the bottom of the
screen. Each line is typed out letter by letter; E shows the whole line,
then the next one, and closes the box after the last. The world pauses while
the box is open.

The template has two: the sign in the garden (controls and goal) and Moss,
who explains the locked door, the key and the slime.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `speaker_name` | "Moss" (empty for the sign) | each instance in `scenes/main.tscn` |
| `lines` | 5 lines (Moss), 3 lines (sign) | each instance in `scenes/main.tscn` |
| `letters_per_second` | 60 (0 = instant) | `scripts/dialogue_box.gd` |
| talk range | `TalkZone` radius 56 px | `scenes/npc.tscn`, `scenes/sign.tscn` |

## How to change it

- Change what someone says: edit their `lines` in `scenes/main.tscn` (one
  entry per page).
- Add a character: instance `scenes/npc.tscn` in `World` and set
  `speaker_name` and `lines`.
- Make talking do something (give an item, open a door): add a signal to
  `talkable.gd` emitted when the dialogue closes, and connect it in `main.gd`.
