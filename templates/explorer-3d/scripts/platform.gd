@tool
extends StaticBody3D
## A solid block to stand on: a stepping stone, a wooden plank, a tier of
## the hill, or the island itself. The node's position is the CENTRE OF
## ITS TOP SURFACE, so a platform at y = 2 has a walking surface 2 metres
## up. It builds its own mesh and collision from `size`, so changing
## `size`, `color` or `round_shape` updates it right away (also in the
## editor). Units are metres.

## Width (x), height (y) and depth (z) in metres. A round platform is a
## cylinder: `size.x` is its diameter and `size.z` is ignored.
@export var size: Vector3 = Vector3(4.0, 1.0, 4.0):
	set(value):
		size = value
		_rebuild()

## The colour of the whole block.
@export var color: Color = Color(0.62, 0.62, 0.66):
	set(value):
		color = value
		_rebuild()

## Turn on for a round (cylinder) platform instead of a box.
@export var round_shape: bool = false:
	set(value):
		round_shape = value
		_rebuild()


func _ready() -> void:
	_rebuild()


## (Re)creates the mesh and the collision shape to match the exports.
func _rebuild() -> void:
	var mesh_node := get_node_or_null("Mesh") as MeshInstance3D
	var shape_node := get_node_or_null("Shape") as CollisionShape3D
	if mesh_node == null or shape_node == null:
		return  # the children don't exist yet; _ready() calls this again

	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.roughness = 0.9

	if round_shape:
		var cylinder := CylinderMesh.new()
		cylinder.top_radius = size.x / 2.0
		cylinder.bottom_radius = size.x / 2.0
		cylinder.height = size.y
		cylinder.radial_segments = 32
		cylinder.rings = 1
		mesh_node.mesh = cylinder
		var round_collider := CylinderShape3D.new()
		round_collider.radius = size.x / 2.0
		round_collider.height = size.y
		shape_node.shape = round_collider
	else:
		var box := BoxMesh.new()
		box.size = size
		mesh_node.mesh = box
		var box_collider := BoxShape3D.new()
		box_collider.size = size
		shape_node.shape = box_collider

	mesh_node.material_override = material
	# The block hangs down from its top surface.
	mesh_node.position.y = -size.y / 2.0
	shape_node.position.y = -size.y / 2.0
