extends Control
## The backdrop behind everything: a soft gradient from a top color to a
## bottom color, plus a few faint circles so it doesn't look flat.
## set_colors() blends from the current look to a new one.

var _top := Color("#141a3d")
var _bottom := Color("#34477a")
var _gradient_texture := GradientTexture2D.new()
var _gradient := Gradient.new()
var _blend: Tween


func _ready() -> void:
	_gradient_texture.gradient = _gradient
	_gradient_texture.fill_from = Vector2(0.5, 0.0)
	_gradient_texture.fill_to = Vector2(0.5, 1.0)
	_gradient_texture.width = 4
	_gradient_texture.height = 256
	_apply()


## Changes the backdrop. `seconds` of 0 switches at once.
func set_colors(top: Color, bottom: Color, seconds: float) -> void:
	if _blend:
		_blend.kill()
	if seconds <= 0.0:
		_top = top
		_bottom = bottom
		_apply()
		return
	var from_top := _top
	var from_bottom := _bottom
	_blend = create_tween()
	_blend.tween_method(func(amount: float) -> void:
		_top = from_top.lerp(top, amount)
		_bottom = from_bottom.lerp(bottom, amount)
		_apply(), 0.0, 1.0, seconds)


func _apply() -> void:
	_gradient.set_color(0, _top)
	_gradient.set_color(1, _bottom)
	queue_redraw()


func _draw() -> void:
	draw_texture_rect(_gradient_texture, Rect2(Vector2.ZERO, size), false)
	# A few big, very faint circles for depth (like out-of-focus lights).
	var light := _bottom.lightened(0.35)
	for spot in [Vector3(0.12, 0.30, 90.0), Vector3(0.33, 0.16, 50.0), Vector3(0.71, 0.24, 120.0), Vector3(0.9, 0.12, 60.0), Vector3(0.55, 0.42, 70.0)]:
		draw_circle(Vector2(size.x * spot.x, size.y * spot.y), spot.z, Color(light, 0.10))
