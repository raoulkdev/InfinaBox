extends Node2D
## One pushable block. It draws itself (a rounded crate), slides smoothly
## from square to square, and turns green with a little pop when it sits on
## a target. The board (board.gd) decides where blocks are; this script only
## makes them look and move nicely. The origin is the block's center.

## Crate colors while it is not on a target.
@export var body_color: Color = Color("e07a5f")
@export var border_color: Color = Color("b5563f")
## Crate colors once it sits on a target.
@export var done_color: Color = Color("81b29a")
@export var done_border_color: Color = Color("5a8f76")
## How big the "pop" gets when a block lands on a target (1.0 = no pop).
@export var pop_size: float = 1.25
## How long the pop takes, in seconds.
@export var pop_time: float = 0.22

# Half the side of the crate, in board units (a tile is 64 wide).
const HALF: float = 27.0

var _on_target: bool = false
var _slide_tween: Tween
var _pop_tween: Tween
var _style := StyleBoxFlat.new()
var _shadow := StyleBoxFlat.new()


func _ready() -> void:
	_style.set_corner_radius_all(10)
	_style.set_border_width_all(4)
	_style.anti_aliasing = true
	_shadow.set_corner_radius_all(10)
	_shadow.bg_color = Color(0, 0, 0, 0.18)
	_shadow.anti_aliasing = true
	_update_colors()


## Slides the block to `target` (a position in board units).
func slide_to(target: Vector2, time: float) -> void:
	if _slide_tween:
		_slide_tween.kill()
	_slide_tween = create_tween()
	_slide_tween.tween_property(self, "position", target, time) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)


## Moves the block at once, with no sliding (used when a level loads).
func place_at(target: Vector2) -> void:
	if _slide_tween:
		_slide_tween.kill()
	position = target


## Tells the block whether it is sitting on a target. With `pop` set, a block
## that has just landed there does a little bounce.
func set_on_target(value: bool, pop: bool = false) -> void:
	if value == _on_target:
		return
	_on_target = value
	_update_colors()
	if value and pop:
		if _pop_tween:
			_pop_tween.kill()
		scale = Vector2.ONE
		_pop_tween = create_tween()
		_pop_tween.tween_property(self, "scale", Vector2.ONE * pop_size, pop_time * 0.4) \
			.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
		_pop_tween.tween_property(self, "scale", Vector2.ONE, pop_time * 0.6) \
			.set_trans(Tween.TRANS_BACK).set_ease(Tween.EASE_OUT)


func _update_colors() -> void:
	_style.bg_color = done_color if _on_target else body_color
	_style.border_color = done_border_color if _on_target else border_color
	queue_redraw()


func _draw() -> void:
	# A soft shadow slightly below the crate, then the crate itself.
	draw_style_box(_shadow, Rect2(-HALF, -HALF + 5.0, HALF * 2.0, HALF * 2.0))
	draw_style_box(_style, Rect2(-HALF, -HALF, HALF * 2.0, HALF * 2.0))
	var line_color := _style.border_color
	if _on_target:
		# A white tick when the block is home.
		var tick := PackedVector2Array([Vector2(-11, 0), Vector2(-3, 9), Vector2(12, -9)])
		draw_polyline(tick, Color.WHITE, 6.0, true)
	else:
		# Crate planks: two crossing lines.
		var inner := HALF - 9.0
		draw_line(Vector2(-inner, -inner), Vector2(inner, inner), line_color, 4.0, true)
		draw_line(Vector2(-inner, inner), Vector2(inner, -inner), line_color, 4.0, true)
