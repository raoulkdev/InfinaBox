extends StaticBody3D
## A locked door. Look at it and press E: with a key in hand the door uses
## the key and slides up out of the way; without one it says it is locked.

## Sent when the door has been unlocked.
signal opened

## What to say when the player has no key.
@export var locked_text := "Locked. You need a key."
## How long the door takes to slide open, in seconds.
@export var open_time := 1.1
## How far the door slides up, in metres (the door is 3.2 m tall).
@export var slide_distance := 3.3

var is_open := false


func _ready() -> void:
	# main.gd counts the doors in this group for the HUD.
	add_to_group("doors")


## The text shown when the player looks at this (see player.gd).
func get_prompt(player: Player) -> String:
	if is_open:
		return ""
	return "[E] Unlock the door" if player.keys > 0 else "Locked. Find a key."


func interact(player: Player) -> void:
	if is_open:
		return
	if not player.use_key():
		player.show_message(locked_text)
		return
	is_open = true
	opened.emit()
	var tween := create_tween()
	tween.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN_OUT)
	tween.tween_property(self, "position:y", position.y + slide_distance, open_time)
	# Only once the door is out of the way does it stop blocking.
	tween.tween_callback(func() -> void: collision_layer = 0)
