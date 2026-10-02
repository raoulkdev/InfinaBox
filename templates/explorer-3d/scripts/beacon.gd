extends Area3D
## The goal: a beacon on top of the hill. It stays dim and grey until every
## gem is collected (the game calls `set_active(true)`), then glows gold.
## Touching it while it glows emits `reached`; touching it too early emits
## `touched_too_early` so the game can give a hint.

## Sent when the player touches the glowing beacon.
signal reached
## Sent when the player touches the beacon before it is switched on.
signal touched_too_early

## Colours of the beam and crystal when off and on.
@export var idle_color: Color = Color(0.55, 0.62, 0.72)
@export var active_color: Color = Color(1.0, 0.82, 0.25)
## How fast the crystal and ring turn, in turns per second.
@export var spin_speed: float = 0.3
## How fast the glow pulses once switched on, in pulses per second.
@export var pulse_speed: float = 0.8

var active: bool = false
var _time: float = 0.0

@onready var _crystal: MeshInstance3D = $Crystal
@onready var _ring: MeshInstance3D = $Ring
@onready var _beam: MeshInstance3D = $Beam


func _ready() -> void:
	add_to_group("goal")
	body_entered.connect(_on_body_entered)
	set_active(false)


func _process(delta: float) -> void:
	_time += delta
	_crystal.rotation.y += TAU * spin_speed * delta
	_ring.rotation.y -= TAU * spin_speed * 0.5 * delta
	if active:
		# A gentle breathing glow.
		var pulse := 0.5 + 0.5 * sin(_time * TAU * pulse_speed)
		var material := _material_of(_beam)
		material.albedo_color.a = 0.3 + 0.25 * pulse


## Switches the beacon on (gold, glowing) or off (grey, dim).
func set_active(value: bool) -> void:
	active = value
	var color := active_color if active else idle_color
	for part: MeshInstance3D in [_crystal, _ring]:
		var material := _material_of(part)
		material.albedo_color = color
		material.emission = color
		material.emission_energy_multiplier = 1.5 if active else 0.2
	var beam_material := _material_of(_beam)
	beam_material.albedo_color = Color(color, 0.5 if active else 0.12)
	# Someone already standing on it when it switches on counts as a touch.
	if active:
		for body in get_overlapping_bodies():
			_on_body_entered(body)


## The material that colours one of the beacon's parts.
func _material_of(part: MeshInstance3D) -> StandardMaterial3D:
	return (part.mesh as PrimitiveMesh).material as StandardMaterial3D


func _on_body_entered(body: Node3D) -> void:
	if not body.is_in_group("player"):
		return
	if active:
		reached.emit()
	else:
		touched_too_early.emit()
