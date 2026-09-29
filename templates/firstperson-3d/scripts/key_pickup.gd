extends StaticBody3D
## A key on the floor. Look at it and press E to pick it up. It spins and
## floats so it is easy to spot. Each key opens any one locked door.

## How fast the key spins, in turns per second.
@export var spin_speed := 0.4
## How far the key floats up and down, in metres.
@export var bob_height := 0.08

var _time := 0.0
var _taken := false

@onready var _model: Node3D = $Model
@onready var _glow: OmniLight3D = $Glow
@onready var _model_y := _model.position.y


func _ready() -> void:
	# main.gd counts the keys in this group for the HUD.
	add_to_group("keys")
	_time = randf() * TAU


func _process(delta: float) -> void:
	_time += delta
	_model.rotation.y += TAU * spin_speed * delta
	_model.position.y = _model_y + sin(_time * 2.0) * bob_height


## The text shown when the player looks at this (see player.gd).
func get_prompt(_player: Player) -> String:
	return "" if _taken else "[E] Pick up the key"


func interact(player: Player) -> void:
	if _taken:
		return
	_taken = true
	collision_layer = 0
	player.add_key()
	player.show_message("You found a key!")
	# A quick pop: the key grows and vanishes while its light flashes.
	var tween := create_tween().set_parallel()
	tween.tween_property(_model, "scale", _model.scale * 1.8, 0.2)
	tween.tween_property(_glow, "light_energy", 3.0, 0.2)
	tween.chain().tween_callback(queue_free)
