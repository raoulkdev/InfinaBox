extends Node2D
## The little character you move around the grid. It draws itself (a round
## blue square with eyes that look where you are heading), slides smoothly
## from square to square, and gives a small nudge when you bump into
## something. The board (board.gd) decides where the player is; this script
## only makes it look and move nicely. The origin is the player's center.

## Body colors.
@export var body_color: Color = Color("4d9de0")
@export var border_color: Color = Color("2f6fab")
## How far (in board units) the player leans into a wall when it can't move.
@export var bump_distance: float = 7.0

# Half the side of the body, in board units (a tile is 64 wide).
const HALF: float = 24.0

var _facing: Vector2 = Vector2.DOWN
var _slide_tween: Tween
var _style := StyleBoxFlat.new()
var _shadow := StyleBoxFlat.new()


func _ready() -> void:
	_style.set_corner_radius_all(18)
	_style.set_border_width_all(4)
	_style.anti_aliasing = true
	_style.bg_color = body_color
	_style.border_color = border_color
	_shadow.set_corner_radius_all(18)
	_shadow.bg_color = Color(0, 0, 0, 0.18)
	_shadow.anti_aliasing = true


## Slides to `target` (board units) and looks in direction `dir`.
func slide_to(target: Vector2, dir: Vector2, time: float) -> void:
	_face(dir)
	if _slide_tween:
		_slide_tween.kill()
	_slide_tween = create_tween()
	_slide_tween.tween_property(self, "position", target, time) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)


## Moves at once, with no sliding (used when a level loads).
func place_at(target: Vector2) -> void:
	if _slide_tween:
		_slide_tween.kill()
	position = target


## A little nudge toward `dir` and back to `home` (the square the player is
## standing on): "something is in the way".
func bump(home: Vector2, dir: Vector2, time: float) -> void:
	_face(dir)
	if _slide_tween:
		_slide_tween.kill()
	_slide_tween = create_tween()
	_slide_tween.tween_property(self, "position", home + dir * bump_distance, time * 0.4) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
	_slide_tween.tween_property(self, "position", home, time * 0.6) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)


func _face(dir: Vector2) -> void:
	if dir != Vector2.ZERO:
		_facing = dir.normalized()
		queue_redraw()


func _draw() -> void:
	draw_style_box(_shadow, Rect2(-HALF, -HALF + 5.0, HALF * 2.0, HALF * 2.0))
	draw_style_box(_style, Rect2(-HALF, -HALF, HALF * 2.0, HALF * 2.0))
	# Two eyes. The pupils shift toward the direction the player faces.
	for side: float in [-1.0, 1.0]:
		var eye := Vector2(side * 10.0, -4.0) + _facing * 3.0
		draw_circle(eye, 8.0, Color.WHITE)
		draw_circle(eye + _facing * 3.0, 4.0, Color("2b2d42"))
