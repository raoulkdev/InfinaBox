extends Control
## The ending: a big title, how long you took and a Play again button.
## R (or Start on a gamepad) plays again too. It keeps working while the
## game is paused (process_mode is "Always").

@onready var _title: Label = %Title
@onready var _time: Label = %Time
@onready var _button: Button = %PlayAgain


func _ready() -> void:
	hide()
	_button.pressed.connect(_on_play_again)


func show_screen(title: String, time_text: String) -> void:
	_title.text = title
	_time.text = "Your time  " + time_text
	show()
	modulate.a = 0.0
	create_tween().tween_property(self, "modulate:a", 1.0, 0.4)
	_button.grab_focus()


func _on_play_again() -> void:
	# main.gd owns the restart, so R and this button do the same thing.
	get_tree().current_scene.restart()
