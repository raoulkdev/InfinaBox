extends Node3D
## The third-person camera. It follows its parent (the player) from behind
## and above, and you swing it around the player with the mouse or the
## right stick. A spring arm pulls the camera closer whenever a tree, rock
## or wall would be in the way.
##
## Click in the game to capture the mouse (it disappears and turns the
## camera); press Esc to give the mouse back.
##
## This node is "top level" (see camera_rig in player.tscn), so it doesn't
## turn when the player turns: it only follows the player's position.

## How far behind the player the camera sits, in metres.
@export var distance: float = 6.0
## How far above the player's feet the camera looks at, in metres.
@export var height: float = 1.5
## How steeply the camera looks down at the start, in degrees.
@export var start_pitch_degrees: float = 20.0
## The lowest and highest the camera can swing, in degrees (0 is level with
## the player, 90 is straight down).
@export var min_pitch_degrees: float = -10.0
@export var max_pitch_degrees: float = 70.0
## How tightly the camera follows: higher is snappier, lower is floatier.
@export var follow_smoothing: float = 12.0
## Mouse look: radians of turn per pixel of mouse movement.
@export var mouse_sensitivity: float = 0.003
## Right-stick look: radians per second at full tilt.
@export var stick_sensitivity: float = 2.5
## Turn on to flip up/down for the mouse and the stick.
@export var invert_y: bool = false
## How fast the camera drifts around the player on the start screen
## (radians per second).
@export var idle_orbit_speed: float = 0.25

## Which way the camera looks around the player, in radians. 0 means it
## sits behind the player looking toward -Z. The player reads this to move
## relative to the camera.
var yaw: float = 0.0
## Whether the mouse and stick turn the camera (off on the start and
## ending screens).
var look_enabled: bool = false
## Whether the camera slowly drifts around by itself (the start screen).
var idle_orbit: bool = false

var _pitch: float = 0.0

@onready var _arm: SpringArm3D = $SpringArm3D
@onready var _target: Node3D = get_parent()


func _ready() -> void:
	_arm.spring_length = distance
	_pitch = deg_to_rad(start_pitch_degrees)
	snap()


func _unhandled_input(event: InputEvent) -> void:
	if not look_enabled:
		return
	if event is InputEventMouseButton and event.pressed:
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	elif event.is_action_pressed("ui_cancel"):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	elif event is InputEventMouseMotion and Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
		_turn(event.relative * mouse_sensitivity)


func _process(delta: float) -> void:
	if look_enabled:
		_turn(Input.get_vector("look_left", "look_right", "look_up", "look_down") * stick_sensitivity * delta)
	elif idle_orbit:
		yaw += idle_orbit_speed * delta

	rotation.y = yaw
	_arm.rotation.x = -_pitch
	# Glide toward the player rather than sticking rigidly to them.
	var goal := _target.global_position + Vector3.UP * height
	global_position = global_position.lerp(goal, 1.0 - exp(-follow_smoothing * delta))


## Puts the camera right behind the player, looking in the direction
## `new_yaw` (used at the start and after a respawn).
func snap(new_yaw: float = 0.0) -> void:
	yaw = new_yaw
	rotation.y = yaw
	_arm.rotation.x = -_pitch
	global_position = _target.global_position + Vector3.UP * height


## Turns the camera by `amount` (x = turn right, y = look down), in radians.
func _turn(amount: Vector2) -> void:
	yaw -= amount.x
	var y := -amount.y if invert_y else amount.y
	_pitch = clampf(_pitch + y, deg_to_rad(min_pitch_degrees), deg_to_rad(max_pitch_degrees))
