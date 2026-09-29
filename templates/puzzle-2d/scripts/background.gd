extends Node2D
## The backdrop: a soft top-to-bottom color gradient with a few large, faint
## circles for a bit of depth. It is drawn far bigger than the window so it
## still fills the screen when the window is resized.

@export var top_color: Color = Color("3d3a63")
@export var bottom_color: Color = Color("262444")
## The color of the faint circles (keep the alpha low).
@export var circle_color: Color = Color(1, 1, 1, 0.04)


func _draw() -> void:
	var top := -800.0
	var bottom := 1520.0
	var left := -1200.0
	var right := 2480.0
	draw_polygon(
		PackedVector2Array([Vector2(left, top), Vector2(right, top), Vector2(right, bottom), Vector2(left, bottom)]),
		PackedColorArray([top_color, top_color, bottom_color, bottom_color]))
	draw_circle(Vector2(180, 130), 260.0, circle_color)
	draw_circle(Vector2(1120, 610), 340.0, circle_color)
	draw_circle(Vector2(700, 40), 150.0, circle_color)
