class_name CharacterPortrait
extends Control
## A character's placeholder portrait: a big colored shape with a friendly
## face. Swap this for real art later (a TextureRect with a PNG per
## character works the same way from stage.gd's point of view).
##
## The speaker is drawn bright and gently bobbing; everyone else on stage is
## dimmed. Call setup() once, then set_speaking().

## How long the bright / dim change takes, in seconds.
@export var highlight_time := 0.2

var _color := Color.WHITE
var _shape := "circle"
var _style := StyleBoxFlat.new()
var _speaking := false
var _time := 0.0
var _base_y := 0.0
var _tween: Tween


## Makes this portrait a `shape` in `color`. Size depends on the shape so a
## cast of different shapes feels varied.
func setup(color: Color, shape: String) -> void:
	_color = color
	_shape = shape
	match shape:
		"square":
			custom_minimum_size = Vector2(250, 330)
		"triangle":
			custom_minimum_size = Vector2(270, 340)
		"diamond":
			custom_minimum_size = Vector2(260, 350)
		_:
			custom_minimum_size = Vector2(250, 260)
	size = custom_minimum_size
	pivot_offset = Vector2(size.x / 2.0, size.y)
	_style.bg_color = color
	_style.border_color = color.darkened(0.45)
	_style.set_border_width_all(6)
	_style.set_corner_radius_all(48)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	queue_redraw()


## Puts the portrait at `bottom_center` (its feet) and fades it in.
func enter(bottom_center: Vector2) -> void:
	_base_y = bottom_center.y - size.y
	position = Vector2(bottom_center.x - size.x / 2.0, _base_y + 30.0)
	modulate.a = 0.0
	var tween := create_tween().set_parallel()
	tween.tween_property(self, "modulate:a", 1.0, 0.3)
	tween.tween_property(self, "position:y", _base_y, 0.3).set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)


## Bright and bobbing while speaking, dimmed otherwise.
func set_speaking(speaking: bool) -> void:
	_speaking = speaking
	if _tween:
		_tween.kill()
	_tween = create_tween().set_parallel()
	# self_modulate tints the drawing without touching the fade-in (modulate).
	var target_color := Color.WHITE if speaking else Color(0.62, 0.62, 0.7, 1.0)
	_tween.tween_property(self, "self_modulate", target_color, highlight_time)
	_tween.tween_property(self, "scale", Vector2.ONE * (1.03 if speaking else 0.97), highlight_time)


func _process(delta: float) -> void:
	if not _speaking:
		return
	_time += delta
	# Only nudge once the fade-in slide has finished.
	if modulate.a >= 0.99:
		position.y = _base_y + sin(_time * 6.0) * 3.0


func _draw() -> void:
	var w := size.x
	var h := size.y
	var outline := _color.darkened(0.45)
	var ink := Color("#2a2540")
	var face := Vector2(w / 2.0, h * 0.45)
	match _shape:
		"square":
			draw_style_box(_style, Rect2(10, 10, w - 20, h - 20))
			# A lighter cheek stripe so it isn't one flat block.
			draw_rect(Rect2(28, h * 0.72, w - 56, 14), _color.lightened(0.18))
		"triangle":
			var points := PackedVector2Array([Vector2(w / 2.0, 12), Vector2(w - 12, h - 12), Vector2(12, h - 12)])
			draw_colored_polygon(points, _color)
			draw_polyline(PackedVector2Array([points[0], points[1], points[2], points[0]]), outline, 6.0, true)
			face = Vector2(w / 2.0, h * 0.62)
		"diamond":
			var points := PackedVector2Array([Vector2(w / 2.0, 10), Vector2(w - 10, h * 0.5), Vector2(w / 2.0, h - 10), Vector2(10, h * 0.5)])
			draw_colored_polygon(points, _color)
			draw_polyline(PackedVector2Array([points[0], points[1], points[2], points[3], points[0]]), outline, 6.0, true)
			face = Vector2(w / 2.0, h * 0.5)
		_:
			var center := Vector2(w / 2.0, h / 2.0)
			var radius := minf(w, h) / 2.0 - 8.0
			draw_circle(center, radius, outline)
			draw_circle(center, radius - 6.0, _color)
			# A soft highlight.
			draw_circle(center + Vector2(-radius * 0.35, -radius * 0.4), radius * 0.28, Color(1, 1, 1, 0.14))
			face = center
	# The face: two eyes and a smile.
	draw_circle(face + Vector2(-44, -12), 11.0, ink)
	draw_circle(face + Vector2(44, -12), 11.0, ink)
	draw_circle(face + Vector2(-40, -16), 4.0, Color.WHITE)
	draw_circle(face + Vector2(48, -16), 4.0, Color.WHITE)
	draw_arc(face + Vector2(0, 14), 32.0, 0.35, PI - 0.35, 20, ink, 6.0, true)
