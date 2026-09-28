@tool
extends Area2D
## A row of spikes: touching them sends the player back to the last
## checkpoint. It draws itself, so it looks right in the Godot editor too.
##
## The spikes' position is their BOTTOM-LEFT corner: put it on top of a
## platform (same y as the platform's top edge). `count` sets how many
## spikes are in the row; each is `spike_size` wide.

## How many spikes are in the row.
@export_range(1, 50) var count: int = 3:
	set(value):
		count = value
		_update_shape()
## The width and height of one spike, in pixels.
@export var spike_size: Vector2 = Vector2(32, 28):
	set(value):
		spike_size = value
		_update_shape()
@export var color: Color = Color("e2e8f0")
@export var outline_color: Color = Color("475569")


func _ready() -> void:
	_update_shape()
	if not Engine.is_editor_hint():
		body_entered.connect(_on_body_entered)


## Makes the hit box match the row. It's a little smaller than the drawing
## (lower and narrower), so only a real touch counts: that feels fair.
func _update_shape() -> void:
	if not is_node_ready():
		return  # _ready() calls this again once the child nodes exist
	var width := count * spike_size.x
	var collision: CollisionShape2D = $CollisionShape2D
	(collision.shape as RectangleShape2D).size = Vector2(width - 12.0, spike_size.y * 0.6)
	collision.position = Vector2(width / 2.0, -spike_size.y * 0.3)
	queue_redraw()


func _draw() -> void:
	for i in count:
		var left := i * spike_size.x
		var points := PackedVector2Array([
			Vector2(left + 2.0, 0.0),
			Vector2(left + spike_size.x / 2.0, -spike_size.y),
			Vector2(left + spike_size.x - 2.0, 0.0),
		])
		draw_colored_polygon(points, color)
		points.append(points[0])
		draw_polyline(points, outline_color, 2.0)


func _on_body_entered(body: Node2D) -> void:
	if body.is_in_group("player"):
		body.die()
