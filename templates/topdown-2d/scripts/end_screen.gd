extends Control
## The win / game-over screen: a big title, a line of text and a Restart
## button. R (or Start on a gamepad) restarts too. It keeps working while
## the game is paused (process_mode is "Always").

@onready var _title: Label = $Center/Panel/Margin/Rows/Title
@onready var _subtitle: Label = $Center/Panel/Margin/Rows/Subtitle
@onready var _restart_button: Button = $Center/Panel/Margin/Rows/Restart


func _ready() -> void:
	hide()
	_restart_button.pressed.connect(restart)


func show_screen(title: String, subtitle: String) -> void:
	_title.text = title
	_subtitle.text = subtitle
	show()
	modulate.a = 0.0
	create_tween().tween_property(self, "modulate:a", 1.0, 0.3)
	_restart_button.grab_focus()


func _unhandled_input(event: InputEvent) -> void:
	if visible and event.is_action_pressed("restart"):
		get_viewport().set_input_as_handled()
		restart()


## Starts the game over from the beginning.
func restart() -> void:
	# Pausing survives a scene reload, so unpause first.
	get_tree().paused = false
	get_tree().reload_current_scene()
