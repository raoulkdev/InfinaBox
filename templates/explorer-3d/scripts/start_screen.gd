extends CanvasLayer
## The title screen: the game's name and "Press Space to start". The game
## (main.gd) shows and hides it; the text is in start_screen.tscn.

## How fast the "Press Space" line pulses, in seconds per pulse.
@export var pulse_seconds: float = 0.9

@onready var _prompt: Label = %Prompt


func _ready() -> void:
	# Make the prompt breathe so it's clear the game is waiting for you.
	var tween := create_tween().set_loops()
	tween.tween_property(_prompt, "modulate:a", 0.35, pulse_seconds)
	tween.tween_property(_prompt, "modulate:a", 1.0, pulse_seconds)
