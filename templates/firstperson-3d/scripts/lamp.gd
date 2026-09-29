@tool
extends Node3D
## A ceiling lamp: a glowing bulb plus a light that fills the room around
## it. Place lamps just under the ceiling (y = 3.0).

## How bright the light is.
@export_range(0.0, 8.0, 0.1) var energy := 1.6:
	set(value):
		energy = value
		_apply()
## How far the light reaches, in metres.
@export_range(1.0, 30.0, 0.5) var light_range := 9.0:
	set(value):
		light_range = value
		_apply()
## The colour of the light and the bulb.
@export var color := Color(1.0, 0.86, 0.62):
	set(value):
		color = value
		_apply()
## Lets the light cast shadows (prettier, but heavier on slow computers).
@export var shadows := false:
	set(value):
		shadows = value
		_apply()
## A spooky lamp that stutters.
@export var flicker := false

var _flicker_left := 0.0


func _ready() -> void:
	_apply()


func _process(delta: float) -> void:
	if not flicker or Engine.is_editor_hint():
		return
	_flicker_left -= delta
	if _flicker_left > 0.0:
		return
	_flicker_left = randf_range(0.04, 0.2)
	var light := $Light as OmniLight3D
	# Mostly steady, sometimes dips, rarely goes almost dark.
	light.light_energy = energy * (randf_range(0.45, 1.0) if randf() < 0.85 else 0.08)


func _apply() -> void:
	if not is_inside_tree():
		return
	var light := get_node_or_null("Light") as OmniLight3D
	var bulb := get_node_or_null("Bulb") as MeshInstance3D
	if light == null or bulb == null:
		return
	light.light_energy = energy
	light.omni_range = light_range
	light.light_color = color
	light.shadow_enabled = shadows
	var material := StandardMaterial3D.new()
	material.albedo_color = color
	material.emission_enabled = true
	material.emission = color
	material.emission_energy_multiplier = 1.3
	bulb.material_override = material
