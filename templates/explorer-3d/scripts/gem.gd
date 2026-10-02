extends Area3D
## A gem to collect. It spins and bobs on the spot; when the player touches
## it, it pops away and `collected` is emitted (the game counts them). The
## node's position is the gem's centre, so put it about 1 metre above the
## ground.

## Sent once when the player picks the gem up.
signal collected

## How fast it spins, in turns per second.
@export var spin_speed: float = 0.4
## How far it bobs up and down, in metres, and how fast (bobs per second).
@export var bob_height: float = 0.15
@export var bob_speed: float = 0.5

var _picked_up: bool = false
var _time: float = 0.0

@onready var _mesh: Node3D = $Mesh


func _ready() -> void:
	add_to_group("gems")
	body_entered.connect(_on_body_entered)
	# Start each gem at a different point in its bob, so they don't move
	# in lockstep.
	_time = randf() * TAU


func _process(delta: float) -> void:
	_time += delta
	_mesh.rotation.y += TAU * spin_speed * delta
	_mesh.position.y = sin(_time * TAU * bob_speed) * bob_height


func _on_body_entered(body: Node3D) -> void:
	if _picked_up or not body.is_in_group("player"):
		return
	_picked_up = true
	set_deferred("monitoring", false)
	collected.emit()
	# Pop: swell for a moment, then vanish.
	var tween := create_tween()
	tween.tween_property(_mesh, "scale", Vector3.ONE * 1.8, 0.12)
	tween.tween_property(_mesh, "scale", Vector3.ZERO, 0.12)
	tween.tween_callback(hide)


## Puts the gem back, ready to be collected again (used when the game
## restarts).
func reset() -> void:
	_picked_up = false
	_mesh.scale = Vector3.ONE
	show()
	set_deferred("monitoring", true)
