@tool
class_name Room
extends Node2D
## One room of the world: a tiled floor with walls around its edge, and an
## optional doorway (a gap in the wall) in the middle of any side.
##
## The node's position is the room's top-left corner. The room draws itself
## (so you see it in the Godot editor too, thanks to @tool) and builds its
## wall collisions when the game starts. The camera uses rooms to decide
## what to show: it never looks past the edges of the room the player is in.
##
## To connect two rooms, put them side by side and give both a doorway on
## the shared side. Doorways sit in the middle of a side, so line the rooms
## up so those middles meet.

## Room size in pixels. The window is 1280×720, so a room that size fills the
## screen exactly; a bigger room makes the camera scroll inside it.
@export var size := Vector2(1280, 720)
## Which sides have a doorway in the middle of the wall.
@export var doorway_north := false
@export var doorway_south := false
@export var doorway_east := false
@export var doorway_west := false
## How wide each doorway is, in pixels.
@export var doorway_width := 128.0
## How thick the walls are, in pixels.
@export var wall_thickness := 40.0

@export_group("Colors")
## The floor is a checkerboard of these two colors (tiles of floor_tile_size).
@export var floor_color := Color("7cc47f")
@export var floor_color_alt := Color("74bb78")
@export var floor_tile_size := 64.0
@export var wall_color := Color("4a3f5c")
## A lighter strip along the top of each wall, so walls look raised.
@export var wall_top_color := Color("6b5b84")


func _ready() -> void:
	add_to_group("rooms")
	# In the editor we only draw; the game also needs walls you bump into.
	if not Engine.is_editor_hint():
		_build_wall_collisions()


func _process(_delta: float) -> void:
	# Redraw while editing, so changes in the Inspector show up right away.
	if Engine.is_editor_hint():
		queue_redraw()


## The room's area in world coordinates (used by the camera).
func get_global_rect() -> Rect2:
	return Rect2(global_position, size)


func _draw() -> void:
	# Floor: a checkerboard of two close colors makes movement easy to read.
	var bounds := Rect2(Vector2.ZERO, size)
	var columns := ceili(size.x / floor_tile_size)
	var rows := ceili(size.y / floor_tile_size)
	for x in columns:
		for y in rows:
			var tile := Rect2(Vector2(x, y) * floor_tile_size, Vector2.ONE * floor_tile_size)
			var color := floor_color if (x + y) % 2 == 0 else floor_color_alt
			draw_rect(tile.intersection(bounds), color)

	var walls := _wall_rects()
	# A soft shadow on the floor just below each wall.
	for wall in walls:
		var shadow := Rect2(wall.position.x, wall.end.y, wall.size.x, 12.0)
		draw_rect(shadow.intersection(bounds), Color(0, 0, 0, 0.12))
	for wall in walls:
		draw_rect(wall, wall_color)
		draw_rect(Rect2(wall.position, Vector2(wall.size.x, 8.0)), wall_top_color)


## The wall pieces, in the room's own coordinates, leaving gaps for doorways.
func _wall_rects() -> Array[Rect2]:
	var t := wall_thickness
	var rects: Array[Rect2] = []
	rects.append_array(_split(Rect2(0, 0, size.x, t), doorway_north, true))
	rects.append_array(_split(Rect2(0, size.y - t, size.x, t), doorway_south, true))
	rects.append_array(_split(Rect2(0, 0, t, size.y), doorway_west, false))
	rects.append_array(_split(Rect2(size.x - t, 0, t, size.y), doorway_east, false))
	return rects


## Returns the wall `wall` whole, or as two pieces with a doorway between.
func _split(wall: Rect2, doorway: bool, horizontal: bool) -> Array[Rect2]:
	var pieces: Array[Rect2] = []
	if not doorway:
		pieces.append(wall)
	elif horizontal:
		var w := (wall.size.x - doorway_width) / 2.0
		pieces.append(Rect2(wall.position, Vector2(w, wall.size.y)))
		pieces.append(Rect2(wall.position + Vector2(w + doorway_width, 0), Vector2(w, wall.size.y)))
	else:
		var h := (wall.size.y - doorway_width) / 2.0
		pieces.append(Rect2(wall.position, Vector2(wall.size.x, h)))
		pieces.append(Rect2(wall.position + Vector2(0, h + doorway_width), Vector2(wall.size.x, h)))
	return pieces


## One StaticBody2D holding a box-shaped collision for every wall piece.
## It's on physics layer 1 ("world"), which the player and enemies bump into.
func _build_wall_collisions() -> void:
	var body := StaticBody2D.new()
	body.name = "Walls"
	add_child(body)
	for wall in _wall_rects():
		var shape := RectangleShape2D.new()
		shape.size = wall.size
		var collision := CollisionShape2D.new()
		collision.shape = shape
		collision.position = wall.get_center()
		body.add_child(collision)
