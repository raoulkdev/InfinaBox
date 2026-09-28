extends CanvasLayer
## The on-screen display: the coin counter in the top-left corner, and a
## short controls hint at the bottom that fades out after a few seconds.

## Seconds the controls hint stays on screen before fading out.
@export var hint_time: float = 6.0

@onready var _coin_label: Label = $CoinCounter/Label
@onready var _hint: Label = $Hint


func _ready() -> void:
	var tween := create_tween()
	tween.tween_interval(hint_time)
	tween.tween_property(_hint, "modulate:a", 0.0, 1.0)


## Shows "collected / total" and gives the counter a little bounce when a
## coin is picked up.
func set_coins(collected: int, total: int) -> void:
	_coin_label.text = "%d / %d" % [collected, total]
	if collected > 0:
		_coin_label.pivot_offset = _coin_label.size / 2.0
		var tween := create_tween()
		tween.tween_property(_coin_label, "scale", Vector2(1.3, 1.3), 0.08)
		tween.tween_property(_coin_label, "scale", Vector2.ONE, 0.15)
