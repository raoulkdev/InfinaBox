@tool
extends StaticBody3D
## A box of wall, floor, ceiling or decoration: the building block of the
## level. Give it a `size` (in metres) and a `color` and it draws itself and
## (if `solid`) stops the player. Its position is the CENTRE of the box.
##
## It rebuilds itself when you change a value, so you can shape it in the
## Godot editor too. Blocks with the same color and glow share one material.

## Width (x), height (y) and depth (z), in metres.
@export var size := Vector3(1.0, 1.0, 1.0):
	set(value):
		size = value
		_rebuild()
## The colour of the block.
@export var color := Color(0.6, 0.6, 0.6):
	set(value):
		color = value
		_rebuild()
## Solid blocks stop the player and the "E" ray. Turn off for a flat
## decoration such as a rug.
@export var solid := true:
	set(value):
		solid = value
		_rebuild()
## Above 0 the block glows in its own colour (0 = normal).
@export_range(0.0, 8.0, 0.1) var glow := 0.0:
	set(value):
		glow = value
		_rebuild()

## Materials already made, by colour and glow, so blocks can share them.
static var _materials := {}


func _ready() -> void:
	_rebuild()


func _rebuild() -> void:
	if not is_inside_tree():
		return
	var mesh_node := get_node_or_null("Mesh") as MeshInstance3D
	if mesh_node == null:
		mesh_node = MeshInstance3D.new()
		mesh_node.name = "Mesh"
		add_child(mesh_node)
	var box_mesh := BoxMesh.new()
	box_mesh.size = size
	mesh_node.mesh = box_mesh
	mesh_node.material_override = _material_for(color, glow)

	var shape_node := get_node_or_null("Shape") as CollisionShape3D
	if shape_node == null:
		shape_node = CollisionShape3D.new()
		shape_node.name = "Shape"
		add_child(shape_node)
	var box_shape := BoxShape3D.new()
	box_shape.size = size
	shape_node.shape = box_shape
	shape_node.disabled = not solid
	# Layer 1 is "world". A block that isn't solid isn't on any layer.
	collision_layer = 1 if solid else 0


static func _material_for(c: Color, g: float) -> StandardMaterial3D:
	var key := "%s_%s" % [c.to_html(), g]
	if _materials.has(key):
		return _materials[key]
	var material := StandardMaterial3D.new()
	material.albedo_color = c
	material.roughness = 0.9
	if g > 0.0:
		material.emission_enabled = true
		material.emission = c
		material.emission_energy_multiplier = g
	_materials[key] = material
	return material
