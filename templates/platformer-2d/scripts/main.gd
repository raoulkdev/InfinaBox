extends Node2D
## Runs the game: loads a level, puts the player at its start, counts the
## coins, and shows the "Level complete!" screen when the player reaches
## the goal flag. Press R (or Start on a gamepad) to restart the level.

# The other scripts this one talks to, so their functions can be called
# with type checking.
const Player := preload("res://scripts/player.gd")
const Level := preload("res://scripts/level.gd")
const Hud := preload("res://scripts/hud.gd")
const LevelComplete := preload("res://scripts/level_complete.gd")

## The levels, in order. To add a level, make a new scene in
## scenes/levels/ (duplicate level_1.tscn) and add it to this list.
@export var levels: Array[PackedScene] = []

var _level_index: int = 0
var _level: Level
var _coins_collected: int = 0
var _coins_total: int = 0
## True while a level is being swapped in (see load_level).
var _loading: bool = false

@onready var _player: Player = $Player
@onready var _hud: Hud = $HUD
@onready var _level_complete: LevelComplete = $LevelComplete


func _ready() -> void:
	_level_complete.continue_pressed.connect(_on_continue_pressed)
	load_level(0)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("restart"):
		load_level(_level_index)


## Swaps in level number `index` (0 is the first) and starts it fresh.
func load_level(index: int) -> void:
	if _loading:
		return  # already swapping levels (R pressed twice quickly)
	_level_index = index
	var level := levels[index].instantiate() as Level

	# Put the player at the new level's start. (A level's top-left corner is
	# at (0, 0), so the marker's position is already a world position.)
	var start: Vector2 = level.get_node("PlayerStart").position
	_player.set_checkpoint(start)
	_player.teleport(start)
	_player.controls_enabled = true
	_player.set_camera_limits(Rect2(Vector2.ZERO, level.size))
	_level_complete.hide()

	if _level != null:
		# Take the old level out right away, so its coins and flag are gone
		# before the new level's are counted below.
		remove_child(_level)
		_level.queue_free()
		_level = null
		# Then give the physics engine a moment to catch up with the
		# player's move: otherwise the new level's goal flag or spikes
		# could still "see" the player where it just was (say, at the old
		# level's goal flag) and trigger by mistake.
		# (The player holds still meanwhile, so it doesn't start falling.)
		_loading = true
		_player.set_physics_process(false)
		await get_tree().physics_frame
		await get_tree().physics_frame
		_player.set_physics_process(true)
		_loading = false

	_level = level
	add_child(_level)
	move_child(_level, _player.get_index())  # draw the level behind the player

	# Every coin and goal flag in the level is in a group; listen to them.
	_coins_collected = 0
	_coins_total = 0
	for coin in get_tree().get_nodes_in_group("coins"):
		coin.collected.connect(_on_coin_collected)
		_coins_total += 1
	for goal in get_tree().get_nodes_in_group("goal"):
		goal.reached.connect(_on_goal_reached)

	_hud.set_coins(_coins_collected, _coins_total)


func _on_coin_collected() -> void:
	_coins_collected += 1
	_hud.set_coins(_coins_collected, _coins_total)


func _on_goal_reached() -> void:
	_player.controls_enabled = false
	var has_next_level := _level_index + 1 < levels.size()
	_level_complete.show_result(_coins_collected, _coins_total, has_next_level)


## The button on the "Level complete!" screen: next level, or play again.
func _on_continue_pressed() -> void:
	if _level_index + 1 < levels.size():
		load_level(_level_index + 1)
	else:
		load_level(_level_index)
