---
type: other
title: "Saving"
status: draft
---

# Saving

**In plain words.** The game can save and load. Saves are small text files
in the player's own folder, kept in numbered slots. Anything in the game that
should be remembered (health, inventory, which doors are open) says so, and
the saving helper collects it without knowing what it is.

## Technical details

`core/save_system.gd`, autoload `SaveSystem`: `save_game(slot)`,
`load_game(slot)`, `has_save`, `delete_save`. Files are
`user://saves/<slot>.json` with `version`, `saved_at` and `data`. A node is
saved by joining group `saveable` and implementing `save_key()` (a stable
name), `save_data() -> Dictionary` and `load_data(data)`. Change
`FORMAT_VERSION` and extend `_upgrade()` when the saved shape changes, so
older saves keep loading.
