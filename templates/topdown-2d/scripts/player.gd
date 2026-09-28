class_name Player
extends CharacterBody2D
## The hero. Walks in eight directions, talks to characters (E or Space),
## carries keys, and has hearts.
##
## Other scripts talk to the player through a few functions: enemies call
## take_hit(), keys call add_key(), locked doors call use_key(). The HUD
## listens to the signals below to show hearts and keys.

signal hearts_changed(hearts: int, max_hearts: int)
signal keys_changed(keys: int)
signal died

## Top walking speed, in pixels per second.
@export var max_speed := 260.0
## How quickly the hero gets up to speed (pixels per second, per second).
## Lower feels floaty and slippery, higher feels snappy.
@export var acceleration := 2000.0
## How quickly the hero stops after you let go of the keys.
@export var friction := 2400.0
## Hearts at the start of the game.
@export var max_hearts := 3
## How hard a hit pushes the hero away (starting speed, pixels per second).
@export var knockback_strength := 620.0
## How long a knockback takes away control, in seconds.
@export var knockback_time := 0.2
## After a hit, the hero can't be hurt again for this long (and blinks).
@export var invincible_time := 1.2

var hearts := 0
var keys := 0

var _facing := Vector2.DOWN
var _knockback_left := 0.0
var _invincible_left := 0.0
var _walk_time := 0.0
## The nearest thing we can talk to (or null), shown with an "E" bubble.
var _talk_target: Node = null

@onready var _visual: Node2D = $Visual
@onready var _eyes: Node2D = $Visual/Eyes
@onready var _interact_area: Area2D = $InteractArea
@onready var _hurt_particles: CPUParticles2D = $HurtParticles


func _ready() -> void:
	hearts = max_hearts


func _physics_process(delta: float) -> void:
	_knockback_left = maxf(_knockback_left - delta, 0.0)
	_update_invincibility(delta)

	# A direction from the arrow keys / WASD / stick. get_vector keeps
	# diagonals the same speed as straight lines (length never above 1).
	var input := Vector2.ZERO
	if is_alive():
		input = Input.get_vector("move_left", "move_right", "move_up", "move_down")

	if _knockback_left > 0.0:
		# Being knocked back: glide, no control.
		velocity = velocity.move_toward(Vector2.ZERO, friction * delta)
	elif input != Vector2.ZERO:
		velocity = velocity.move_toward(input * max_speed, acceleration * delta)
		_facing = input.normalized()
	else:
		velocity = velocity.move_toward(Vector2.ZERO, friction * delta)
	move_and_slide()

	_animate(delta)
	_update_talk_target()


func _unhandled_input(event: InputEvent) -> void:
	# Talk to whatever is nearby. (While the dialogue box is open the game
	# is paused, so this doesn't run and the box handles E itself.)
	if event.is_action_pressed("interact") and is_alive() and _talk_target:
		get_viewport().set_input_as_handled()
		_talk_target.interact(self)


func is_alive() -> bool:
	return hearts > 0


## Called by enemies. Costs a heart, pushes the hero away from `from_position`
## and makes them briefly invincible. Does nothing while still invincible.
func take_hit(from_position: Vector2) -> void:
	if _invincible_left > 0.0 or not is_alive():
		return
	hearts -= 1
	hearts_changed.emit(hearts, max_hearts)

	var away := from_position.direction_to(global_position)
	if away == Vector2.ZERO:
		away = -_facing
	velocity = away * knockback_strength
	_knockback_left = knockback_time
	_invincible_left = invincible_time

	# Juice: a red flash, a puff of particles and a little screen shake.
	_visual.modulate = Color(1.0, 0.25, 0.3)
	create_tween().tween_property(_visual, "modulate", Color.WHITE, 0.3)
	_hurt_particles.restart()
	get_tree().call_group("camera", "shake", 9.0)

	if not is_alive():
		_play_death()
		died.emit()


## Called by key pickups.
func add_key() -> void:
	keys += 1
	keys_changed.emit(keys)


## Called by locked doors. Uses up one key if the hero has any.
func use_key() -> bool:
	if keys <= 0:
		return false
	keys -= 1
	keys_changed.emit(keys)
	return true


func _update_invincibility(delta: float) -> void:
	if _invincible_left <= 0.0:
		return
	_invincible_left = maxf(_invincible_left - delta, 0.0)
	# Blink: hidden for a moment every 0.12 s, visible again once it's over.
	_visual.visible = _invincible_left == 0.0 or fmod(_invincible_left, 0.12) > 0.05


## Small bits of life: a bouncy waddle while walking, eyes looking where
## the hero is heading.
func _animate(delta: float) -> void:
	if not is_alive():
		return
	var speed_ratio := velocity.length() / max_speed
	if speed_ratio > 0.1:
		_walk_time += delta
		var squash := sin(_walk_time * 18.0) * 0.07 * speed_ratio
		_visual.scale = Vector2(1.0 + squash, 1.0 - squash)
	else:
		_walk_time = 0.0
		_visual.scale = _visual.scale.lerp(Vector2.ONE, 0.3)
	_eyes.position = _eyes.position.lerp(_facing * 5.0, 0.25)


func _play_death() -> void:
	velocity = Vector2.ZERO
	_invincible_left = 0.0
	_visual.visible = true
	var tween := create_tween().set_parallel()
	tween.tween_property(_visual, "rotation", TAU, 0.6)
	tween.tween_property(_visual, "scale", Vector2(0.2, 0.2), 0.6)


## Picks the closest talkable thing whose TalkZone overlaps our InteractArea
## (both on physics layer 4, "interactables") and shows its "E" bubble,
## hiding the previous one's.
func _update_talk_target() -> void:
	var closest: Node2D = null
	if is_alive():
		for zone in _interact_area.get_overlapping_areas():
			var thing := zone.get_parent() as Node2D
			if thing and thing.is_in_group("talkable"):
				if closest == null or _is_closer(thing, closest):
					closest = thing
	if closest == _talk_target:
		return
	if _talk_target:
		_talk_target.set_prompt_visible(false)
	_talk_target = closest
	if _talk_target:
		_talk_target.set_prompt_visible(true)


func _is_closer(a: Node2D, b: Node2D) -> bool:
	return global_position.distance_to(a.global_position) < global_position.distance_to(b.global_position)
