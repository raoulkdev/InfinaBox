extends CharacterBody2D
## A wandering slime. It hops around near where it started and hurts the
## player on touch: the player loses a heart and is knocked back. It can't
## be defeated yet — giving the player a way to fight back is a good next step.

## Walking speed, in pixels per second.
@export var speed := 90.0
## How far from its starting spot it may roam, in pixels.
@export var wander_radius := 220.0
## It keeps going one way for a random time between these (seconds)…
@export var min_wander_time := 0.8
@export var max_wander_time := 2.0
## …and sometimes stops for a moment instead (0 = never, 1 = always).
@export_range(0.0, 1.0) var rest_chance := 0.25

var _home := Vector2.ZERO
var _direction := Vector2.ZERO
var _time_left := 0.0
var _hop_time := 0.0

@onready var _visual: Node2D = $Visual
@onready var _eyes: Node2D = $Visual/Eyes
@onready var _hitbox: Area2D = $Hitbox


func _ready() -> void:
	_home = global_position
	_pick_direction()


func _physics_process(delta: float) -> void:
	_time_left -= delta
	if _time_left <= 0.0:
		_pick_direction()
	# Wandered too far? Head back home.
	if global_position.distance_to(_home) > wander_radius:
		_direction = global_position.direction_to(_home)

	velocity = _direction * speed
	move_and_slide()
	# Bumped into a wall: bounce off it.
	if get_slide_collision_count() > 0:
		_direction = _direction.bounce(get_slide_collision(0).get_normal())

	# Anything player-shaped touching the hitbox gets hurt. (The player
	# ignores hits while it's blinking, so this can run every frame.)
	for body in _hitbox.get_overlapping_bodies():
		if body is Player:
			body.take_hit(global_position)

	_animate(delta)


## Picks a new random direction (one of eight) or a short rest.
func _pick_direction() -> void:
	_time_left = randf_range(min_wander_time, max_wander_time)
	if randf() < rest_chance:
		_direction = Vector2.ZERO
	else:
		_direction = Vector2.RIGHT.rotated(randi_range(0, 7) * TAU / 8.0)


## A squishy hop while moving, eyes looking where it's going.
func _animate(delta: float) -> void:
	if _direction == Vector2.ZERO:
		_visual.scale = _visual.scale.lerp(Vector2.ONE, 0.2)
		return
	_hop_time += delta
	var squash := sin(_hop_time * 10.0) * 0.12
	_visual.scale = Vector2(1.0 + squash, 1.0 - squash)
	_eyes.position = _eyes.position.lerp(_direction * 4.0, 0.2)
