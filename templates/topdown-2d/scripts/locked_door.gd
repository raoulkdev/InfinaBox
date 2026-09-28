extends StaticBody2D
## A locked door. It blocks the way until the player walks into it carrying
## a key; then it uses up that key and slides open for good. Without a key
## it shows a short hint instead.

signal opened

## What the door says when the player bumps into it without a key.
@export var locked_text := "Locked! Find the key."

var is_open := false

@onready var _left: Node2D = $Visual/Left
@onready var _right: Node2D = $Visual/Right
@onready var _collision: CollisionShape2D = $CollisionShape2D
@onready var _touch_area: Area2D = $TouchArea
@onready var _hint: Label = $Hint
@onready var _dust: CPUParticles2D = $Dust


func _ready() -> void:
	# TouchArea is a little bigger than the door, so walking into the door
	# counts as touching it.
	_touch_area.body_entered.connect(_on_body_touched)
	_hint.text = locked_text
	_hint.modulate.a = 0.0


func _on_body_touched(body: Node2D) -> void:
	if is_open or not body is Player:
		return
	if body.use_key():
		open()
	else:
		_show_hint()


## Opens the door: the two halves slide apart and fade, and it stops
## blocking the way.
func open() -> void:
	is_open = true
	_collision.set_deferred("disabled", true)
	_dust.restart()
	var tween := create_tween().set_parallel()
	tween.tween_property(_left, "position:x", _left.position.x - 56.0, 0.4).set_trans(Tween.TRANS_QUAD)
	tween.tween_property(_right, "position:x", _right.position.x + 56.0, 0.4).set_trans(Tween.TRANS_QUAD)
	tween.tween_property($Visual, "modulate:a", 0.0, 0.4).set_delay(0.15)
	opened.emit()


func _show_hint() -> void:
	var tween := create_tween()
	tween.tween_property(_hint, "modulate:a", 1.0, 0.15)
	tween.tween_interval(1.5)
	tween.tween_property(_hint, "modulate:a", 0.0, 0.4)
