extends CharacterBody2D
## The player: runs left and right, jumps, and respawns at the last
## checkpoint after touching a hazard or falling out of the level.
##
## The jump is tuned by how high it goes and how long it takes to reach the
## top; gravity and jump speed are worked out from those two numbers, so
## changing one never breaks the other. Three small tricks make it feel
## fair: "coyote time" (you can still jump for a moment after running off
## a ledge), a "jump buffer" (pressing jump just before landing still
## counts), and a shorter hop when you let go of jump early.

## Sent when the player touches a hazard or falls out of the level.
signal died
## Sent when the player reappears at the last checkpoint.
signal respawned

@export_group("Running")
## Top running speed, in pixels per second.
@export var run_speed: float = 320.0
## How quickly the player speeds up on the ground (pixels per second, per second).
@export var ground_acceleration: float = 2400.0
## How quickly the player stops on the ground when no key is held.
@export var ground_friction: float = 2800.0
## How quickly the player can change direction in the air. Lower = floatier.
@export var air_acceleration: float = 1500.0

@export_group("Jumping")
## How high a full jump goes, in pixels (the player is 40 pixels tall).
@export var jump_height: float = 150.0
## Seconds from leaving the ground to the top of a full jump.
@export var time_to_jump_peak: float = 0.38
## Falling is this many times faster than rising, which feels snappier.
@export var fall_gravity_multiplier: float = 1.7
## Letting go of jump early multiplies the upward speed by this (a short hop).
@export var jump_release_multiplier: float = 0.45
## The fastest the player can fall, in pixels per second.
@export var max_fall_speed: float = 900.0
## Seconds after running off a ledge during which jump still works.
@export var coyote_time: float = 0.1
## Seconds before landing during which a jump press is remembered.
@export var jump_buffer_time: float = 0.12

@export_group("Respawning")
## Seconds between dying and reappearing at the checkpoint.
@export var respawn_delay: float = 0.6

## Where the player reappears after dying. Checkpoints update it.
var respawn_point: Vector2
## Falling below this height (set by main.gd from the level size) counts as dying.
var fall_limit_y: float = INF
## Turned off by main.gd when the level is complete.
var controls_enabled: bool = true
## True between dying and respawning.
var is_dead: bool = false

var _coyote_timer: float = 0.0
var _jump_buffer_timer: float = 0.0
var _facing: float = 1.0
var _was_on_floor: bool = true
## The squash-and-stretch amount, eased back to (1, 1) every frame.
var _squash: Vector2 = Vector2.ONE

@onready var _visual: Node2D = $Visual
@onready var _camera: Camera2D = $Camera2D
@onready var _dust: CPUParticles2D = $Dust
@onready var _burst: CPUParticles2D = $Burst


func _ready() -> void:
	respawn_point = global_position


func _physics_process(delta: float) -> void:
	if is_dead:
		return

	# Gravity and jump speed come from the jump height and time to the peak.
	var gravity := 2.0 * jump_height / (time_to_jump_peak * time_to_jump_peak)
	var jump_speed := 2.0 * jump_height / time_to_jump_peak

	# Coyote time: keep "can jump" alive for a moment after leaving the ground.
	if is_on_floor():
		_coyote_timer = coyote_time
	else:
		_coyote_timer -= delta

	# Jump buffer: remember a jump press for a moment, in case we land soon.
	if controls_enabled and Input.is_action_just_pressed("jump"):
		_jump_buffer_timer = jump_buffer_time
	else:
		_jump_buffer_timer -= delta

	if _jump_buffer_timer > 0.0 and _coyote_timer > 0.0:
		velocity.y = -jump_speed
		_jump_buffer_timer = 0.0
		_coyote_timer = 0.0
		_squash = Vector2(0.75, 1.3)  # stretch upwards on take-off
		_dust.restart()

	# Variable jump height: letting go early cuts the jump short.
	if Input.is_action_just_released("jump") and velocity.y < 0.0:
		velocity.y *= jump_release_multiplier

	# Fall faster than we rise, up to a top falling speed.
	var gravity_now := gravity * (fall_gravity_multiplier if velocity.y > 0.0 else 1.0)
	velocity.y = minf(velocity.y + gravity_now * delta, max_fall_speed)

	# Running: speed up towards the held direction, or slow down to a stop.
	var direction := Input.get_axis("move_left", "move_right") if controls_enabled else 0.0
	var rate := air_acceleration
	if is_on_floor():
		rate = ground_acceleration if direction != 0.0 else ground_friction
	velocity.x = move_toward(velocity.x, direction * run_speed, rate * delta)
	if direction != 0.0:
		_facing = signf(direction)

	move_and_slide()

	# Landing: squash down and kick up some dust.
	if is_on_floor() and not _was_on_floor:
		_squash = Vector2(1.3, 0.75)
		_dust.restart()
	_was_on_floor = is_on_floor()

	# Ease the squash back to normal and face the way we're moving.
	_squash = _squash.lerp(Vector2.ONE, 1.0 - exp(-12.0 * delta))
	_visual.scale = Vector2(_squash.x * _facing, _squash.y)

	if global_position.y > fall_limit_y:
		die()


## Called by hazards (and by falling out of the level).
func die() -> void:
	if is_dead:
		return
	is_dead = true
	velocity = Vector2.ZERO
	_visual.hide()
	_burst.restart()
	died.emit()
	await get_tree().create_timer(respawn_delay).timeout
	if is_dead:
		respawn()


## Puts the player back at the last checkpoint.
func respawn() -> void:
	teleport(respawn_point)
	_squash = Vector2(0.6, 1.4)  # pop back in
	respawned.emit()


## Called by checkpoints: the player will respawn here from now on.
func set_checkpoint(point: Vector2) -> void:
	respawn_point = point


## Moves the player to `point` at once, standing still and alive, and snaps
## the camera there instead of letting it glide across the level.
func teleport(point: Vector2) -> void:
	global_position = point
	velocity = Vector2.ZERO
	is_dead = false
	_visual.show()
	_camera.reset_smoothing()


## Keeps the camera inside the level (called by main.gd for each level).
func set_camera_limits(area: Rect2) -> void:
	_camera.limit_left = int(area.position.x)
	_camera.limit_top = int(area.position.y)
	_camera.limit_right = int(area.end.x)
	_camera.limit_bottom = int(area.end.y)
	fall_limit_y = area.end.y + 64.0
