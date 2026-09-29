extends Control
## The screen shown before the game starts, and again when you pause with
## Esc. Click (or press E, Space or Enter) to start or carry on. It keeps
## working while the game is paused (process_mode is "Always").

## Sent when the player clicks to play.
signal clicked

@onready var _title: Label = %Title
@onready var _subtitle: Label = %Subtitle
@onready var _action: Label = %Action
@onready var _hint: Label = %Hint


func _ready() -> void:
	gui_input.connect(_on_gui_input)


func set_game_name(game_name: String) -> void:
	_title.text = game_name


## Shows the screen. `paused` swaps the welcome text for a "Paused" one.
func show_screen(paused: bool) -> void:
	_subtitle.text = "Paused" if paused else "Find 3 keys. Open 3 doors. Get out."
	_action.text = "Click to continue" if paused else "Click to start"
	_hint.text = "R  play again from the start" if paused else \
		"WASD  walk      Mouse  look      Shift  sprint\nSpace  jump      E  pick up / open      Esc  pause"
	show()


func _on_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		clicked.emit()


func _input(event: InputEvent) -> void:
	# The keyboard and gamepad work too. (Esc is left alone: it pauses.)
	if visible and (event.is_action_pressed("interact") or event.is_action_pressed("jump") \
			or event.is_action_pressed("ui_accept")):
		get_viewport().set_input_as_handled()
		clicked.emit()
