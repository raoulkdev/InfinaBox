extends "res://scripts/screen.gd"
## The title screen: the game's name (taken from the project's name) and a
## blinking "Press Space to start". To change the words, edit
## scenes/ui/start_screen.tscn; to rename the game, rename the project.

## How long one blink of "Press Space to start" takes, in seconds.
@export var blink_time: float = 0.9

@onready var _title: Label = %TitleLabel
@onready var _prompt: Label = %PromptLabel


func _ready() -> void:
	super()
	_title.text = str(ProjectSettings.get_setting("application/config/name", "Push Puzzle"))
	var blink := create_tween().set_loops()
	blink.tween_property(_prompt, "modulate:a", 0.35, blink_time / 2.0)
	blink.tween_property(_prompt, "modulate:a", 1.0, blink_time / 2.0)
