extends Node2D
## Runs one round of the game: sends waves of enemies from the edges of the
## arena, counts the score, shakes the screen on hits, and shows the
## game-over screen (then restarts) when the player runs out of health.
##
## Wave n has first_wave_size + (n - 1) * wave_size_growth enemies. The next
## wave starts time_between_waves seconds after the last enemy is gone.

@export_group("Waves")
## The basic enemy (scenes/enemy.tscn).
@export var chaser_scene: PackedScene
## The big, tough enemy (scenes/brute.tscn).
@export var brute_scene: PackedScene
## Enemies in wave 1.
@export var first_wave_size: int = 5
## Extra enemies in each wave after that.
@export var wave_size_growth: int = 3
## Seconds between two enemies appearing within a wave.
@export var spawn_interval: float = 0.55
## Seconds of calm between clearing a wave and the next one starting.
@export var time_between_waves: float = 2.0
## The first wave that can contain Brutes.
@export var brute_first_wave: int = 3
## Chance (0..1) that an enemy is a Brute, once Brutes are allowed.
@export_range(0.0, 1.0) var brute_chance: float = 0.25
## Enemies get this much faster every wave (0.06 = 6% per wave).
@export var wave_speed_growth: float = 0.06
## Enemies never appear closer to the player than this (pixels).
@export var min_spawn_distance: float = 300.0

@export_group("Screen shake")
## Shake strength (pixels) when the player is hit.
@export var shake_on_player_hit: float = 14.0
## Shake strength (pixels) when an enemy is destroyed.
@export var shake_on_kill: float = 3.0
## How fast the shake calms down (pixels per second).
@export var shake_fade: float = 45.0

var score := 0
var wave := 0

var _left_to_spawn := 0
var _alive := 0
var _spawn_timer := 0.0
var _wave_break := 1.0  # A short pause before wave 1, too.
var _shake := 0.0
var _last_health := 0
var _game_over := false

@onready var _player: Player = $Player
@onready var _enemies: Node2D = $Enemies
@onready var _floor: Control = $Arena/Floor
@onready var _camera: Camera2D = $Camera2D
@onready var _hud: Hud = $HUD


func _ready() -> void:
	# The operating system's crosshair cursor makes aiming easier.
	Input.set_default_cursor_shape(Input.CURSOR_CROSS)
	_player.health_changed.connect(_on_player_health_changed)
	_player.died.connect(_on_player_died)
	_hud.restart_requested.connect(restart)
	_last_health = _player.max_health
	_hud.set_score(score)
	_hud.set_health(_player.max_health, _player.max_health)


func _process(delta: float) -> void:
	# Screen shake: nudge the camera randomly, less and less each frame.
	_shake = move_toward(_shake, 0.0, shake_fade * delta)
	_camera.offset = Vector2(randf_range(-1, 1), randf_range(-1, 1)) * _shake


func _physics_process(delta: float) -> void:
	if _game_over:
		return
	if _left_to_spawn > 0:
		_spawn_timer -= delta
		if _spawn_timer <= 0.0:
			_spawn_enemy()
			_spawn_timer = spawn_interval
	elif _alive == 0:
		# Wave cleared: wait a moment, then start the next one.
		_wave_break -= delta
		if _wave_break <= 0.0:
			_start_next_wave()


func _start_next_wave() -> void:
	wave += 1
	_left_to_spawn = first_wave_size + (wave - 1) * wave_size_growth
	_spawn_timer = 0.8  # Let the "Wave N" banner show first.
	_wave_break = time_between_waves
	_hud.show_wave(wave)


func _spawn_enemy() -> void:
	var scene := chaser_scene
	if wave >= brute_first_wave and randf() < brute_chance:
		scene = brute_scene
	var enemy: Enemy = scene.instantiate()
	enemy.global_position = _pick_spawn_point()
	enemy.target = _player
	enemy.speed *= 1.0 + wave_speed_growth * (wave - 1)
	enemy.died.connect(_on_enemy_died)
	_enemies.add_child(enemy)
	_alive += 1
	_left_to_spawn -= 1


## A random point just inside one of the four arena edges, away from the
## player so nothing appears right on top of them.
func _pick_spawn_point() -> Vector2:
	var area := _floor.get_global_rect().grow(-40.0)
	var point := area.get_center()
	for _attempt in 12:
		match randi() % 4:
			0: point = Vector2(randf_range(area.position.x, area.end.x), area.position.y)
			1: point = Vector2(randf_range(area.position.x, area.end.x), area.end.y)
			2: point = Vector2(area.position.x, randf_range(area.position.y, area.end.y))
			_: point = Vector2(area.end.x, randf_range(area.position.y, area.end.y))
		if point.distance_to(_player.global_position) >= min_spawn_distance:
			break
	return point


func _on_enemy_died(enemy: Enemy) -> void:
	_alive -= 1
	score += enemy.score_value
	_hud.set_score(score)
	shake(shake_on_kill)


func _on_player_health_changed(current: int, maximum: int) -> void:
	_hud.set_health(current, maximum)
	if current < _last_health:
		shake(shake_on_player_hit)
	_last_health = current


func _on_player_died() -> void:
	_game_over = true
	_camera.offset = Vector2.ZERO
	_hud.show_game_over(score, wave)
	# Freeze everything except the HUD (whose process mode is "Always").
	get_tree().paused = true


## Starts a shake of `strength` pixels (a stronger shake wins).
func shake(strength: float) -> void:
	_shake = maxf(_shake, strength)


## Starts a fresh round.
func restart() -> void:
	get_tree().paused = false
	get_tree().reload_current_scene()
