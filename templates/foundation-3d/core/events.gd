extends Node
## The game's message board (an autoload, so it is reachable as `Events`).
##
## When something happens that other parts of the game might care about, the
## part that made it happen announces it here, and anything interested
## listens. The parts never need to know about each other, so adding a new
## system (an inventory, a quest log) means adding listeners, not rewiring
## the old ones.
##
## Announce:  Events.game_started.emit()
## Listen:    Events.game_started.connect(_on_game_started)
##
## Add a signal here the first time two systems need to talk. Name it for
## what happened (`enemy_defeated`), not for what should happen in response.

signal game_started
signal game_paused(paused: bool)
signal scene_changed(path: String)
signal settings_changed
signal save_completed(slot: String)
signal load_completed(slot: String)
