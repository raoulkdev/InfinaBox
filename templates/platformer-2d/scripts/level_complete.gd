extends CanvasLayer
## The "Level complete!" screen, shown by main.gd when the player reaches
## the goal flag. Its button goes to the next level, or plays this one
## again if it was the last. R restarts the level at any time.

## Sent when the player presses the button.
signal continue_pressed

@onready var _coins_label: Label = %CoinsLabel
@onready var _button: Button = %ContinueButton


func _ready() -> void:
	_button.pressed.connect(func() -> void: continue_pressed.emit())


## Shows the screen with the number of coins found.
func show_result(coins: int, total: int, has_next_level: bool) -> void:
	_coins_label.text = "You found %d of %d coins" % [coins, total]
	_button.text = "Next level" if has_next_level else "Play again"
	show()
	# Wait a moment before the button takes keyboard focus, so a jump press
	# that was still held when the flag was reached doesn't click it.
	await get_tree().create_timer(0.5).timeout
	if visible:
		_button.grab_focus()
