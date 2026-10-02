extends Control
## The hearts and key counter in the top-left corner, drawn with shapes
## (no images needed). Full hearts are red, lost ones are dark.

@export var heart_color := Color("ff5a6e")
@export var empty_heart_color := Color("4a3f5c")
@export var outline_color := Color("1e1b2e")
@export var key_color := Color("ffd54f")
## Size of one heart, in pixels.
@export var heart_size := 36.0

var _hearts := 3
var _max_hearts := 3
var _keys := 0


func set_hearts(hearts: int, max_hearts: int) -> void:
	var lost := hearts < _hearts
	_hearts = hearts
	_max_hearts = max_hearts
	queue_redraw()
	if lost:
		# A quick "ouch" pop so the change is noticed.
		pivot_offset = Vector2(heart_size, heart_size / 2.0)
		scale = Vector2(1.25, 1.25)
		create_tween().tween_property(self, "scale", Vector2.ONE, 0.25).set_trans(Tween.TRANS_BACK)


func set_keys(keys: int) -> void:
	_keys = keys
	queue_redraw()


func _draw() -> void:
	var spacing := heart_size * 1.2
	for i in _max_hearts:
		var center := Vector2(heart_size / 2.0 + i * spacing, heart_size / 2.0)
		var color := heart_color if i < _hearts else empty_heart_color
		_draw_heart(center, heart_size, color)

	# The key counter, only once the player has a key.
	if _keys > 0:
		var x := _max_hearts * spacing + 16.0
		_draw_key(Vector2(x + 14.0, heart_size / 2.0))
		var font := get_theme_default_font()
		draw_string_outline(font, Vector2(x + 38.0, heart_size / 2.0 + 10.0), "x %d" % _keys,
				HORIZONTAL_ALIGNMENT_LEFT, -1, 28, 8, outline_color)
		draw_string(font, Vector2(x + 38.0, heart_size / 2.0 + 10.0), "x %d" % _keys,
				HORIZONTAL_ALIGNMENT_LEFT, -1, 28, Color.WHITE)


## A heart from the classic heart curve, with a dark outline.
func _draw_heart(center: Vector2, size: float, color: Color) -> void:
	var points := PackedVector2Array()
	for step in 32:
		var t := step * TAU / 32.0
		var x := 16.0 * pow(sin(t), 3)
		var y := -(13.0 * cos(t) - 5.0 * cos(2 * t) - 2.0 * cos(3 * t) - cos(4 * t))
		points.append(Vector2(x, y + 1.0) / 34.0)
	var outline := PackedVector2Array()
	var fill := PackedVector2Array()
	for p in points:
		outline.append(center + p * (size + 8.0))
		fill.append(center + p * size)
	draw_colored_polygon(outline, outline_color)
	draw_colored_polygon(fill, color)
	# A small shine in the upper left.
	draw_circle(center + Vector2(-size * 0.2, -size * 0.12), size * 0.08, Color(1, 1, 1, 0.6))


## A little key icon: a ring and a stem with one tooth.
func _draw_key(center: Vector2) -> void:
	draw_circle(center + Vector2(-6, 0), 11.0, outline_color)
	draw_rect(Rect2(center + Vector2(-2, -5), Vector2(22, 10)), outline_color)
	draw_circle(center + Vector2(-6, 0), 8.0, key_color)
	draw_circle(center + Vector2(-6, 0), 3.0, outline_color)
	draw_rect(Rect2(center + Vector2(0, -3), Vector2(18, 6)), key_color)
	draw_rect(Rect2(center + Vector2(12, 1), Vector2(5, 7)), key_color)
