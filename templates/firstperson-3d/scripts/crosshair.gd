extends Control
## The small dot in the middle of the screen. It grows and turns gold when
## you are looking at something you can use with E.

## The dot's colours and sizes (radius, in pixels).
@export var idle_color := Color(1.0, 1.0, 1.0, 0.8)
@export var active_color := Color(1.0, 0.82, 0.25, 1.0)
@export var idle_radius := 2.5
@export var active_radius := 4.5

var _active := false


func set_active(active: bool) -> void:
	if active != _active:
		_active = active
		queue_redraw()


func _draw() -> void:
	var middle := size / 2.0
	var radius := active_radius if _active else idle_radius
	# A soft dark ring first, so the dot shows on light walls too.
	draw_circle(middle, radius + 1.5, Color(0.0, 0.0, 0.0, 0.45))
	draw_circle(middle, radius, active_color if _active else idle_color)
