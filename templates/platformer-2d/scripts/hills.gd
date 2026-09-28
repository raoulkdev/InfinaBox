@tool
extends Node2D
## A row of rolling hills for the background, drawn from a couple of sine
## waves. It sits inside a Parallax2D (see scenes/background.tscn), which
## repeats it sideways forever and scrolls it slower than the level, so
## it looks far away.

## The hills' color. Farther hills look best paler.
@export var color: Color = Color("7cc49a"):
	set(value):
		color = value
		queue_redraw()
## The height (y) the hills roll around, in pixels from the top of the screen.
@export var base_y: float = 520.0:
	set(value):
		base_y = value
		queue_redraw()
## How tall the hills are, in pixels.
@export var hill_height: float = 60.0:
	set(value):
		hill_height = value
		queue_redraw()
## Shifts the wave, so two rows of hills don't line up.
@export var phase: float = 0.0:
	set(value):
		phase = value
		queue_redraw()
## Must match the Parallax2D's repeat width, so the ends join up seamlessly.
@export var width: float = 1280.0:
	set(value):
		width = value
		queue_redraw()


func _draw() -> void:
	var points := PackedVector2Array()
	var steps := 64
	for i in steps + 1:
		var x := width * i / steps
		# Whole numbers of waves across `width`, so the ends match.
		var t := x / width * TAU
		var wave := 0.6 * sin(2.0 * t + phase) + 0.4 * sin(5.0 * t + phase * 2.0)
		points.append(Vector2(x, base_y - hill_height * (0.5 + 0.5 * wave)))
	# Close the shape well below the bottom of the screen.
	points.append(Vector2(width, 1000.0))
	points.append(Vector2(0.0, 1000.0))
	draw_colored_polygon(points, color)
