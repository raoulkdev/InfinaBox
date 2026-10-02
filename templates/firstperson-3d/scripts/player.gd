class_name Player
extends CharacterBody3D
## The player, seen through their own eyes. Walks (WASD or the left stick),
## looks around (mouse or the right stick), sprints (Shift), jumps a little
## (Space), carries keys, and uses whatever it is looking at with E.
##
## Units are metres and seconds; +Y is up and the player faces -Z.
##
## Other scripts talk to the player through a few functions: keys call
## add_key(), locked doors call use_key(). The HUD listens to the signals
## below to show keys, the "Press E" prompt and short messages.

signal keys_changed(found: int, held: int)
signal prompt_changed(text: String)
signal message(text: String)

@export_group("Walking")
## Walking speed, in metres per second.
@export var walk_speed := 3.6
## Speed while holding Shift.
@export var sprint_speed := 5.8
## How quickly the player gets up to speed (metres per second, per second).
## Lower feels heavy and slippery, higher feels snappy.
@export var acceleration := 22.0
## How quickly the player stops after you let go of the keys.
@export var friction := 28.0
## How much of your steering works in the air (0 = none, 1 = full).
@export_range(0.0, 1.0) var air_control := 0.3
## Downward pull, in metres per second, per second.
@export var gravity := 18.0
## Upward speed of a jump. The default is a small hop of about half a metre.
@export var jump_speed := 4.2

@export_group("Looking")
## How far the view turns per mouse movement.
@export var mouse_sensitivity := 0.0022
## How fast the right stick turns the view, in radians per second.
@export var stick_sensitivity := 2.8
## How far you can look up or down, in degrees (90 is straight up).
@export_range(30.0, 89.0) var max_pitch_degrees := 85.0
## The camera's field of view, in degrees.
@export_range(50.0, 110.0) var field_of_view := 75.0

@export_group("Head bob")
## The camera sways a little while you walk. Turn off if it bothers you.
@export var head_bob := true
## How far the camera moves up and down, in metres.
@export var bob_height := 0.035
## How fast the sway goes for each metre walked (in radians).
@export var bob_frequency := 2.6

@export_group("Using things")
## How close something must be for the "Press E" prompt, in metres.
@export var reach := 2.6

## Keys picked up in total, and keys in hand (not used on a door yet).
var keys_found := 0
var keys := 0

var _bob_phase := 0.0
var _prompt := ""
var _target: Node = null

@onready var _head: Node3D = $Head
@onready var _camera: Camera3D = $Head/Camera3D
@onready var _ray: RayCast3D = $Head/Camera3D/InteractRay


func _ready() -> void:
	_camera.fov = field_of_view
	# The ray points straight ahead from the camera, `reach` metres long.
	_ray.target_position = Vector3(0.0, 0.0, -reach)


func _physics_process(delta: float) -> void:
	_look_with_stick(delta)
	_move(delta)
	_animate_head(delta)
	_update_target()


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion:
		_turn(-event.relative.x * mouse_sensitivity, -event.relative.y * mouse_sensitivity)
	elif event.is_action_pressed("interact") and _target != null:
		get_viewport().set_input_as_handled()
		_target.interact(self)


## Called by key pickups.
func add_key() -> void:
	keys_found += 1
	keys += 1
	keys_changed.emit(keys_found, keys)


## Called by locked doors. Uses up one key if the player has any.
func use_key() -> bool:
	if keys <= 0:
		return false
	keys -= 1
	keys_changed.emit(keys_found, keys)
	return true


## Shows a short line of text on screen (for example "Locked.").
func show_message(text: String) -> void:
	message.emit(text)


## Turns the view: `yaw` left/right around the body, `pitch` up/down around
## the head. Both in radians (positive turns left / up).
func _turn(yaw: float, pitch: float) -> void:
	rotate_y(yaw)
	var limit := deg_to_rad(max_pitch_degrees)
	_head.rotation.x = clampf(_head.rotation.x + pitch, -limit, limit)


func _look_with_stick(delta: float) -> void:
	var stick := Input.get_vector("look_left", "look_right", "look_down", "look_up")
	if stick != Vector2.ZERO:
		_turn(-stick.x * stick_sensitivity * delta, stick.y * stick_sensitivity * delta)


func _move(delta: float) -> void:
	# Which way the player wants to go, relative to where they face. The
	# length is never above 1, so diagonals aren't faster.
	var input := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var wish := global_transform.basis * Vector3(input.x, 0.0, input.y)
	var speed := sprint_speed if Input.is_action_pressed("sprint") else walk_speed

	var control := 1.0 if is_on_floor() else air_control
	var flat := Vector3(velocity.x, 0.0, velocity.z)
	if input != Vector2.ZERO:
		flat = flat.move_toward(wish * speed, acceleration * control * delta)
	else:
		flat = flat.move_toward(Vector3.ZERO, friction * control * delta)
	velocity.x = flat.x
	velocity.z = flat.z

	if is_on_floor():
		if Input.is_action_just_pressed("jump"):
			velocity.y = jump_speed
	else:
		velocity.y -= gravity * delta
	move_and_slide()


## The little up-and-down sway of walking. It follows the distance walked, so
## sprinting sways faster, and it settles back when you stop.
func _animate_head(delta: float) -> void:
	var speed := Vector3(velocity.x, 0.0, velocity.z).length()
	if head_bob and is_on_floor() and speed > 0.5:
		_bob_phase += speed * bob_frequency * delta
		_camera.position.y = sin(_bob_phase * 2.0) * bob_height
		_camera.position.x = cos(_bob_phase) * bob_height * 0.6
	else:
		_camera.position = _camera.position.lerp(Vector3.ZERO, minf(1.0, 10.0 * delta))


## Looks at what the ray hits and tells the HUD what to say. Things you can
## use (keys, doors) have get_prompt() and interact() functions.
func _update_target() -> void:
	var hit: Node = _ray.get_collider() if _ray.is_colliding() else null
	_target = hit if hit != null and hit.has_method("interact") else null
	var text: String = _target.get_prompt(self) if _target != null else ""
	if text == "":
		_target = null
	if text != _prompt:
		_prompt = text
		prompt_changed.emit(text)
