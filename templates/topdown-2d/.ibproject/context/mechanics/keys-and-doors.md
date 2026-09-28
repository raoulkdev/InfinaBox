---
type: mechanic
title: Keys and doors
status: working
implemented_in: [scripts/key_pickup.gd, scenes/key.tscn, scripts/locked_door.gd, scenes/locked_door.tscn, scripts/player.gd, scripts/status_bar.gd]
---

# Keys and doors

A key lies in the hall's lower-right corner, near the slime. Walking over it
picks it up with a sparkle, and a key counter appears next to the hearts.
The locked door in the hall's north wall blocks the way to the vault;
walking into it with a key uses the key and the door slides open for good.
Without a key it shows "Locked! Find the key." for a moment.

Each key opens any one locked door.

## Tuning values

| Value | Default | Where |
|---|---|---|
| `locked_text` | "Locked! Find the key." | `scripts/locked_door.gd` (or per instance) |
| `bob_height` (key float) | 6 px | `scripts/key_pickup.gd` |
| key position | (3040, 580) | `Key` in `scenes/main.tscn` |
| door position | (2240, 20) | `LockedDoor` in `scenes/main.tscn` |

## How to change it

- Move the key: change the `Key` node's position (hide it somewhere fun).
- More doors: instance `scenes/locked_door.tscn` in a doorway (it is 128 px
  wide and 40 px thick, matching the walls) and another `scenes/key.tscn`.
- Keys that only fit one door: give the key and the door a matching
  `@export var key_id` and store key ids on the player instead of a count.
