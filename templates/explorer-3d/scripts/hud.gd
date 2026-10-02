extends CanvasLayer
## The on-screen display while playing: the gem counter, the timer, a
## short message line, and a reminder of the controls.

## How long a message stays on screen, in seconds.
@export var message_seconds: float = 3.0

var _message_tween: Tween

@onready var _gems_label: Label = %GemsLabel
@onready var _time_label: Label = %TimeLabel
@onready var _message_label: Label = %MessageLabel


func _ready() -> void:
	_message_label.modulate.a = 0.0


## "12 seconds" -> "0:12", "83 seconds" -> "1:23".
static func format_time(seconds: float) -> String:
	return "%d:%02d" % [floori(seconds / 60.0), int(seconds) % 60]


## Shows "Gems 3 / 8".
func set_gems(collected: int, total: int) -> void:
	_gems_label.text = "Gems  %d / %d" % [collected, total]


## Shows the running time.
func set_time(seconds: float) -> void:
	_time_label.text = format_time(seconds)


## Shows a line of text near the top for a few seconds.
func show_message(text: String) -> void:
	_message_label.text = text
	if _message_tween != null:
		_message_tween.kill()
	_message_label.modulate.a = 1.0
	_message_tween = create_tween()
	_message_tween.tween_interval(message_seconds)
	_message_tween.tween_property(_message_label, "modulate:a", 0.0, 0.5)
