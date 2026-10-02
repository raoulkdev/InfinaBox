---
type: other
title: "Settings"
status: draft
---

# Settings

**In plain words.** The player's own choices, such as volume and fullscreen,
are remembered between sessions and applied when the game starts.

## Technical details

`core/user_settings.gd`, autoload `UserSettings`, stored in
`user://settings.cfg`. `get_value(key)` and `set_value(key, value)`; defaults
are in `DEFAULTS`. Changing a value saves, applies it and emits
`Events.settings_changed`. Add a setting by adding a default and handling it
in `_apply()`.
