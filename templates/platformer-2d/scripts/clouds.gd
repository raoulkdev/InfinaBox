@tool
extends Node2D
## Soft clouds for the background, each made of a few overlapping circles.
## Like the hills, it sits inside a Parallax2D that repeats it sideways and
## slowly drifts it along.

@export var color: Color = Color(1, 1, 1, 1):
	set(value):
		color = value
		queue_redraw()
## Where each cloud is (its middle), in pixels. Keep x between 100 and
## 1180 so clouds aren't cut off where the row repeats.
@export var cloud_positions: PackedVector2Array = PackedVector2Array([
	Vector2(160, 120), Vector2(520, 70), Vector2(860, 150), Vector2(1120, 90),
]):
	set(value):
		cloud_positions = value
		queue_redraw()


func _draw() -> void:
	for center in cloud_positions:
		draw_circle(center + Vector2(-34, 6), 24.0, color)
		draw_circle(center + Vector2(0, -6), 32.0, color)
		draw_circle(center + Vector2(36, 4), 26.0, color)
		draw_rect(Rect2(center + Vector2(-58, 6), Vector2(120, 24)), color)
