class_name Enemy
extends CharacterBody2D
## An enemy that chases the player. Bullets knock it back and take away its
## health; at zero it bursts into particles and tells the game it died
## (which adds its score). Touching the player hurts the player.
##
## scenes/enemy.tscn (the Chaser) uses this script as is; scenes/brute.tscn
## is the same scene with bigger numbers. New enemy types start the same way.

## Sent once, just before the enemy disappears.
signal died(enemy: Enemy)

## Chasing speed, in pixels per second. Later waves multiply this a little
## (see wave_speed_growth in scripts/game.gd).
@export var speed: float = 150.0
## Bullets (of 1 damage) needed to destroy it.
@export var max_health: int = 1
## Health the player loses when this enemy touches it.
@export var contact_damage: int = 1
## Points the player gets for destroying it.
@export var score_value: int = 10
## How far one bullet pushes it back. Lower = heavier.
@export var knockback_per_hit: float = 260.0
## How fast the body spins, in turns per second (purely for looks).
@export var spin_speed: float = 0.6
## The burst of particles left behind (scenes/explosion.tscn).
@export var death_effect: PackedScene

## What to chase. Set by the game when it spawns the enemy.
var target: Node2D
var health: int = 0

var _knockback := Vector2.ZERO
var _dead := false

@onready var _body: Polygon2D = $Body
@onready var _outline: Line2D = $Body/Outline


func _ready() -> void:
	health = max_health
	# Pop in from small to full size so new enemies are easy to notice.
	_body.scale = Vector2(0.2, 0.2)
	create_tween().tween_property(_body, "scale", Vector2.ONE, 0.3)


func _physics_process(delta: float) -> void:
	var chase := Vector2.ZERO
	if is_instance_valid(target):
		chase = global_position.direction_to(target.global_position) * speed
	# Knockback fades out quickly, then the enemy is back to chasing.
	_knockback = _knockback.move_toward(Vector2.ZERO, 1400.0 * delta)
	velocity = chase + _knockback
	move_and_slide()
	_body.rotation += TAU * spin_speed * delta


## Shoves the enemy (used by bullets and by touching the player).
func push(impulse: Vector2) -> void:
	_knockback += impulse


## Takes `damage` from a bullet travelling in `from_direction`.
func take_hit(damage: int, from_direction: Vector2) -> void:
	if _dead:
		return
	health -= damage
	push(from_direction * knockback_per_hit)
	# Flash white for a split second: colours brighter than 1 turn white.
	_body.modulate = Color(3, 3, 3)
	create_tween().tween_property(_body, "modulate", Color.WHITE, 0.12)
	if health <= 0:
		die()


func die() -> void:
	if _dead:
		return
	_dead = true
	if death_effect:
		var burst: Explosion = death_effect.instantiate()
		burst.global_position = global_position
		burst.color = _outline.default_color
		# Added next to the enemy, so the burst outlives it.
		get_parent().add_child(burst)
	died.emit(self)
	queue_free()
