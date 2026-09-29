extends CharacterBody3D
## The player: a little explorer who runs and jumps around the island.
## Movement is relative to the camera (W runs away from the camera, A/D
## run left and right on screen), the body swings smoothly to face the way
## it runs, and jumping is forgiving. The node's origin is at the player's
## FEET. All tuning values below can be changed in the Inspector.

## Sent when the player falls off the world, right after being put back
## at the start.
signal fell

const CameraRig := preload("res://scripts/camera_rig.gd")

@export_group("Running")
## Top running speed, in metres per second.
@export var walk_speed: float = 6.0
## How quickly the player speeds up and slows down on the ground
## (metres per second, per second).
@export var acceleration: float = 40.0
@export var deceleration: float = 30.0
## How much of that grip you keep in the air (1 = the same as on the ground).
@export var air_control: float = 0.5
## How quickly the body turns to face the way it runs (higher = snappier).
@export var turn_speed: float = 12.0

@export_group("Jumping")
## Height of a full jump, in metres.
@export var jump_height: float = 2.0
## Seconds a full jump takes to reach its highest point.
@export var time_to_jump_peak: float = 0.4
## Falling is faster than rising by this factor (feels snappier).
@export var fall_gravity_multiplier: float = 1.4
## Letting go of jump early keeps this fraction of the upward speed (a hop).
@export var jump_release_multiplier: float = 0.5
## Fastest possible fall, in metres per second.
@export var max_fall_speed: float = 30.0
## Seconds after walking off a ledge that jumping still works.
@export var coyote_time: float = 0.12
## Seconds a jump press is remembered before landing.
@export var jump_buffer_time: float = 0.12

@export_group("World")
## Falling below this height (y, in metres) puts the player back at the start.
@export var kill_height: float = -8.0

## Turned off on the start and ending screens.
var controls_enabled: bool = false

var _gravity: float
var _jump_velocity: float
var _coyote_left: float = 0.0
var _buffer_left: float = 0.0
var _spawn_position: Vector3 = Vector3.ZERO

@onready var _visual: Node3D = $Visual
@onready var _camera_rig: CameraRig = $CameraRig


func _ready() -> void:
	add_to_group("player")
	# Gravity and jump speed come from the jump height and time to the peak,
	# so those two numbers are all you need to change the jump.
	_gravity = 2.0 * jump_height / (time_to_jump_peak * time_to_jump_peak)
	_jump_velocity = 2.0 * jump_height / time_to_jump_peak
	_spawn_position = global_position


func _physics_process(delta: float) -> void:
	var on_floor := is_on_floor()

	# Coyote time: remember that we were just on the ground.
	if on_floor:
		_coyote_left = coyote_time
	else:
		_coyote_left = maxf(_coyote_left - delta, 0.0)

	# Gravity (stronger while falling).
	if not on_floor:
		var g := _gravity * (fall_gravity_multiplier if velocity.y < 0.0 else 1.0)
		velocity.y = maxf(velocity.y - g * delta, -max_fall_speed)

	# Jumping, with a short memory of an early press.
	if controls_enabled and Input.is_action_just_pressed("jump"):
		_buffer_left = jump_buffer_time
	else:
		_buffer_left = maxf(_buffer_left - delta, 0.0)
	if _buffer_left > 0.0 and _coyote_left > 0.0:
		velocity.y = _jump_velocity
		_buffer_left = 0.0
		_coyote_left = 0.0
	if controls_enabled and Input.is_action_just_released("jump") and velocity.y > 0.0:
		velocity.y *= jump_release_multiplier

	# Running: the stick/keys give a direction relative to the camera.
	var input := Vector2.ZERO
	if controls_enabled:
		input = Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var direction := Basis(Vector3.UP, _camera_rig.yaw) * Vector3(input.x, 0.0, input.y)
	var wanted := direction * walk_speed
	var grip := acceleration if direction != Vector3.ZERO else deceleration
	if not on_floor:
		grip *= air_control
	var flat := Vector3(velocity.x, 0.0, velocity.z).move_toward(wanted, grip * delta)
	velocity.x = flat.x
	velocity.z = flat.z

	move_and_slide()

	# Swing the body round to face where we're running.
	if direction.length_squared() > 0.01:
		var facing := atan2(-direction.x, -direction.z)
		_visual.rotation.y = lerp_angle(_visual.rotation.y, facing, 1.0 - exp(-turn_speed * delta))

	if global_position.y < kill_height:
		respawn()
		fell.emit()


## Where the player comes back to after a fall (the level's PlayerStart).
func set_spawn(position_in_world: Vector3) -> void:
	_spawn_position = position_in_world


## Puts the player back at the spawn point, standing still, facing away
## from the camera, and points the camera the same way.
func respawn() -> void:
	global_position = _spawn_position
	velocity = Vector3.ZERO
	_visual.rotation.y = 0.0
	_camera_rig.snap(0.0)
