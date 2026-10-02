@tool
extends StaticBody2D
## A platform the player can stand on: a grassy block of ground, or (with
## `one_way` on) a thin wooden plank the player can jump up through from
## below. It draws itself, so it's the right size in the Godot editor too.
##
## The platform's position is its TOP-LEFT corner, and `size` is how far it
## reaches right and down. To add one to a level, add another instance of
## scenes/objects/platform.tscn and set its position and size.

## Width and height in pixels. Planks look best about 20 pixels tall.
@export var size: Vector2 = Vector2(256, 64):
	set(value):
		size = value
		_update_shape()
## A plank the player can jump up through from below and stand on.
@export var one_way: bool = false:
	set(value):
		one_way = value
		_update_shape()

@export_group("Colors")
@export var grass_color: Color = Color("5dbb63")
@export var grass_shadow_color: Color = Color("3f9a4d")
@export var dirt_color: Color = Color("9c6b45")
@export var dirt_spot_color: Color = Color("875a39")
@export var plank_color: Color = Color("c8894f")
@export var plank_edge_color: Color = Color("8a5a31")


func _ready() -> void:
	_update_shape()


## Makes the collision box match `size` and redraws.
func _update_shape() -> void:
	if not is_node_ready():
		return  # _ready() calls this again once the child nodes exist
	var collision: CollisionShape2D = $CollisionShape2D
	(collision.shape as RectangleShape2D).size = size
	collision.position = size / 2.0
	collision.one_way_collision = one_way
	queue_redraw()


func _draw() -> void:
	if one_way:
		# A wooden plank with a darker edge and a light top.
		draw_rect(Rect2(Vector2.ZERO, size), plank_edge_color)
		draw_rect(Rect2(2, 2, size.x - 4, size.y - 6), plank_color)
		draw_rect(Rect2(2, 2, size.x - 4, 4), plank_color.lightened(0.25))
		# Nails at each end.
		for x in [8.0, size.x - 12.0]:
			draw_rect(Rect2(x, size.y / 2.0 - 2.0, 4, 4), plank_edge_color)
		return

	# Dirt, with a few darker spots so big blocks don't look flat. The spots
	# repeat in a fixed pattern, so every platform looks the same each run.
	draw_rect(Rect2(Vector2.ZERO, size), dirt_color)
	for row in range(1, int(size.y / 40.0) + 1):
		var y := row * 40.0 - 8.0
		if y > size.y - 12.0:
			break
		var x := 24.0 if row % 2 == 0 else 56.0
		while x < size.x - 16.0:
			draw_rect(Rect2(x, y, 10, 6), dirt_spot_color)
			x += 72.0
	# Grass on top, with a shadow line and little tufts along its edge.
	var grass := minf(14.0, size.y)
	draw_rect(Rect2(0, 0, size.x, grass + 4.0), grass_shadow_color)
	draw_rect(Rect2(0, 0, size.x, grass), grass_color)
	var tuft := 6.0
	while tuft < size.x - 10.0:
		draw_rect(Rect2(tuft, grass, 6, 4), grass_color)
		tuft += 22.0
