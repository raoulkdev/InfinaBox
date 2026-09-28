extends CanvasLayer
## Everything drawn on top of the game: hearts and keys in the top-left
## corner, the dialogue box, and the win / game-over screen. main.gd calls
## the functions below; this script just passes them on to the right part.

@onready var dialogue_box: Control = $DialogueBox
@onready var _status_bar: Control = $StatusBar
@onready var _end_screen: Control = $EndScreen


func set_hearts(hearts: int, max_hearts: int) -> void:
	_status_bar.set_hearts(hearts, max_hearts)


func set_keys(keys: int) -> void:
	_status_bar.set_keys(keys)


func show_end_screen(title: String, subtitle: String) -> void:
	_end_screen.show_screen(title, subtitle)
