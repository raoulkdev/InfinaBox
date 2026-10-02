class_name Player
extends CharacterBody2D
## The player's ship. It moves with WASD / the arrow keys / the left stick,
## turns to face the mouse (or the right stick), and shoots while the shoot
## button is held. Enemies that touch it take away health; after a hit it
## blinks and can't be hurt again for a moment.

## Sent whenever health changes, so the HUD can redraw the health bar.
signal health_changed(current: int, maximum: int)
## Sent once, when health reaches zero.
signal died

@export_group("Movement")
## Top speed, in pixels per second.
@export var speed: float = 340.0
## How fast the ship speeds up and slows down (pixels per second, per
## second). Higher feels snappier, lower feels floaty, like ice.
@export var acceleration: float = 2800.0

@export_group("Weapon")
## The bullet this ship fires (scenes/bullet.tscn).
@export var bullet_scene: PackedScene
## Shots per second while the shoot button is held.
@export var fire_rate: float = 8.0
## How fast bullets fly, in pixels per second.
@export var bullet_speed: float = 950.0
## Damage per bullet. Enemies have 1 (Chaser) or 4 (Brute) health.
@export var bullet_damage: int = 1
## Random wobble added to each shot, in degrees. 0 = perfectly straight.
@export var spread_degrees: float = 3.0

@export_group("Health")
## Hits the player can take before the game is over.
@export var max_health: int = 5
## Seconds of safety after being hit (the ship blinks meanwhile).
@export var invincibility_time: float = 1.0
## How hard the player shoves away an enemy that touches it.
@export var contact_push: float = 520.0

var health: int = 0

# Time left before the next shot, the end of the safety blink, and the
# muzzle flash.
var _shot_cooldown := 0.0
var _invincible_left := 0.0
var _flash_left := 0.0
# Which way the ship points, and whether the mouse (rather than the right
# stick) is currently aiming.
var _aim := Vector2.RIGHT
var _aiming_with_mouse := true

@onready var _body: Node2D = $Body
@onready var _muzzle: Marker2D = $Muzzle
@onready var _flash: Polygon2D = $Muzzle/Flash
@onready var _hurtbox: Area2D = $Hurtbox


func _ready() -> void:
	health = max_health
	_flash.visible = false


func _input(event: InputEvent) -> void:
	# Moving the mouse hands aiming back to the mouse after using a gamepad.
	if event is InputEventMouseMotion:
		_aiming_with_mouse = true


func _physics_process(delta: float) -> void:
	_move(delta)
	_aim_ship()
	_handle_shooting(delta)
	_check_enemy_contact(delta)


func _move(delta: float) -> void:
	# A direction of length 0..1 from the keys or the left stick; ease the
	# velocity towards it so starting and stopping feel smooth.
	var direction := Input.get_vector("move_left", "move_right", "move_up", "move_down")
	velocity = velocity.move_toward(direction * speed, acceleration * delta)
	# move_and_slide() stops the ship at the arena walls.
	move_and_slide()


func _aim_ship() -> void:
	var stick := Input.get_vector("aim_left", "aim_right", "aim_up", "aim_down")
	if stick.length() > 0.3:
		_aiming_with_mouse = false
		_aim = stick.normalized()
	elif _aiming_with_mouse:
		var to_mouse := get_global_mouse_position() - global_position
		# Ignore the mouse when it sits right on top of the ship.
		if to_mouse.length() > 8.0:
			_aim = to_mouse.normalized()
	rotation = _aim.angle()


func _handle_shooting(delta: float) -> void:
	_shot_cooldown -= delta
	_flash_left -= delta
	_flash.visible = _flash_left > 0.0

	# Hold the shoot button, or push the right stick far (twin-stick style).
	var stick := Input.get_vector("aim_left", "aim_right", "aim_up", "aim_down")
	var wants_to_shoot := Input.is_action_pressed("shoot") or stick.length() > 0.7
	if wants_to_shoot and _shot_cooldown <= 0.0:
		_shoot()
		_shot_cooldown = 1.0 / fire_rate


func _shoot() -> void:
	var bullet: Bullet = bullet_scene.instantiate()
	var wobble := deg_to_rad(randf_range(-spread_degrees, spread_degrees))
	bullet.direction = _aim.rotated(wobble)
	bullet.speed = bullet_speed
	bullet.damage = bullet_damage
	bullet.global_position = _muzzle.global_position
	# Bullets live next to the player (in the main scene), not inside it,
	# so they keep flying straight when the ship turns.
	get_parent().add_child(bullet)
	_flash_left = 0.05


func _check_enemy_contact(delta: float) -> void:
	# While safe after a hit, blink by hiding the ship every other 0.1s.
	_invincible_left -= delta
	if _invincible_left > 0.0:
		_body.visible = fmod(_invincible_left, 0.2) > 0.1
		return
	_body.visible = true

	for body in _hurtbox.get_overlapping_bodies():
		if body is Enemy:
			var enemy := body as Enemy
			enemy.push(global_position.direction_to(enemy.global_position) * contact_push)
			take_damage(enemy.contact_damage)
			return


## Loses `amount` health (never below zero) and starts the safety blink.
func take_damage(amount: int) -> void:
	if health <= 0:
		return
	health = maxi(health - amount, 0)
	_invincible_left = invincibility_time
	health_changed.emit(health, max_health)
	if health == 0:
		died.emit()
