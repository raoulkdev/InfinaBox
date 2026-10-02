extends CanvasLayer
## Everything drawn on top of the game: the crosshair, the "Press E"
## prompt, short messages, the key and door counters, and the start and
## ending screens. main.gd and the player call the functions below; this
## script passes them on to the right part.

## The gold and grey of the key markers in the top-left corner.
const KEY_FOUND := Color(1.0, 0.82, 0.25)
const KEY_MISSING := Color(1.0, 1.0, 1.0, 0.18)

@onready var start_screen: Control = $StartScreen
@onready var end_screen: Control = $EndScreen
@onready var _crosshair: Control = $Crosshair
@onready var _prompt: Label = $Prompt
@onready var _message: Label = $Message
@onready var _key_row: HBoxContainer = %KeyRow
@onready var _doors_label: Label = %DoorsLabel

var _message_tween: Tween


func _ready() -> void:
	_prompt.text = ""
	_message.modulate.a = 0.0


## Fills in the counters and the start screen. Called once by main.gd.
func setup(game_name: String, total_keys: int, total_doors: int) -> void:
	start_screen.set_game_name(game_name)
	# One marker per key in the level, all grey until the key is found.
	for child in _key_row.get_children():
		child.queue_free()
	for i in total_keys:
		var marker := ColorRect.new()
		marker.custom_minimum_size = Vector2(22, 22)
		marker.color = KEY_MISSING
		_key_row.add_child(marker)
	set_doors(0, total_doors)


func set_keys(found: int, _total: int) -> void:
	for i in _key_row.get_child_count():
		(_key_row.get_child(i) as ColorRect).color = KEY_FOUND if i < found else KEY_MISSING


func set_doors(opened: int, total: int) -> void:
	_doors_label.text = "Doors opened  %d / %d" % [opened, total]


## Shows what E would do ("" hides it) and lights up the crosshair.
func set_prompt(text: String) -> void:
	_prompt.text = text
	_crosshair.set_active(text != "")


## Shows a line of text in the middle of the screen for a moment.
func show_message(text: String) -> void:
	_message.text = text
	if _message_tween:
		_message_tween.kill()
	_message.modulate.a = 1.0
	_message_tween = create_tween()
	_message_tween.tween_interval(1.6)
	_message_tween.tween_property(_message, "modulate:a", 0.0, 0.5)


func show_start_screen(paused: bool) -> void:
	_crosshair.hide()
	start_screen.show_screen(paused)


func hide_start_screen() -> void:
	start_screen.hide()
	_crosshair.show()


func show_end_screen(title: String, time_text: String) -> void:
	_crosshair.hide()
	_prompt.text = ""
	end_screen.show_screen(title, time_text)
