extends Control
## The shared behavior of the three full-screen messages (the start screen,
## "Level complete!" and the ending): they fade in, and they send `confirmed`
## when the player presses Space / Enter / the gamepad's A button, or clicks
## the screen's button. main.gd decides what happens next.
##
## The scripts start_screen.gd, level_complete.gd and ending_screen.gd extend
## this one to fill in their own text.

## Sent when the player wants to carry on.
signal confirmed

## How long the fade-in takes, in seconds.
@export var open_time: float = 0.25

var _tween: Tween


func _ready() -> void:
	hide()
	# Screens with a button (%ActionButton) also work with the mouse.
	var button := get_node_or_null("%ActionButton") as Button
	if button:
		button.pressed.connect(func() -> void: confirmed.emit())


## Shows the screen with a short fade and a gentle grow.
func open() -> void:
	show()
	modulate.a = 0.0
	var panel := get_node_or_null("%Panel") as Control
	if panel:
		panel.pivot_offset = panel.size / 2.0
		panel.scale = Vector2(0.92, 0.92)
	if _tween:
		_tween.kill()
	_tween = create_tween().set_parallel(true)
	_tween.tween_property(self, "modulate:a", 1.0, open_time)
	if panel:
		_tween.tween_property(panel, "scale", Vector2.ONE, open_time) \
			.set_trans(Tween.TRANS_BACK).set_ease(Tween.EASE_OUT)


func close() -> void:
	if _tween:
		_tween.kill()
	hide()


func _unhandled_input(event: InputEvent) -> void:
	if visible and not event.is_echo() and event.is_action_pressed("confirm"):
		get_viewport().set_input_as_handled()
		confirmed.emit()
