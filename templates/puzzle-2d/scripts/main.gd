extends Node2D
## Runs the whole game from start to ending: the start screen, the levels in
## order, the "Level complete!" panel between them, and the final "You
## solved them all!" screen. Press R at any time to restart the level.
##
## The puzzle rules live in board.gd and the level layouts in levels.gd; this
## script only decides which screen is showing.

# The other scripts this one talks to, so their functions can be called with
# type checking.
const Levels := preload("res://scripts/levels.gd")
const Board := preload("res://scripts/board.gd")
const Hud := preload("res://scripts/hud.gd")
const StartScreen := preload("res://scripts/start_screen.gd")
const LevelComplete := preload("res://scripts/level_complete.gd")
const EndingScreen := preload("res://scripts/ending_screen.gd")

## What the game is doing right now.
enum State { START, PLAYING, LEVEL_COMPLETE, ENDING }

var _state: State = State.START
var _level_index: int = 0
## The fewest moves used on each level so far, by level index. Kept in memory
## only: it is gone when the game closes.
var _best: Dictionary = {}
## The moves used on each level in the current run through the game.
var _run_moves: Dictionary = {}

@onready var _board: Board = $Board
@onready var _hud: Hud = $Interface/HUD
@onready var _start_screen: StartScreen = $Screens/StartScreen
@onready var _level_complete: LevelComplete = $Screens/LevelComplete
@onready var _ending_screen: EndingScreen = $Screens/EndingScreen


func _ready() -> void:
	_board.moves_changed.connect(_hud.set_moves)
	_board.solved.connect(_on_solved)
	_start_screen.confirmed.connect(_on_start_confirmed)
	_level_complete.confirmed.connect(_on_next_confirmed)
	_ending_screen.confirmed.connect(_on_play_again_confirmed)
	# Show the first level behind the title, waiting for the player.
	_show_level(0)
	_board.input_enabled = false
	_hud.hide()
	_start_screen.open()


func _unhandled_input(event: InputEvent) -> void:
	# R restarts the level you are on (also from the "Level complete!" panel,
	# to have another go at a better score).
	if event.is_action_pressed("restart") and not event.is_echo():
		if _state == State.PLAYING or _state == State.LEVEL_COMPLETE:
			_level_complete.close()
			_show_level(_level_index)
			_state = State.PLAYING
			_board.input_enabled = true


# Loads a level onto the board and updates the HUD for it.
func _show_level(index: int) -> void:
	_level_index = index
	_board.load_level(index)
	_hud.set_level(index + 1, Levels.LEVELS.size(), Levels.LEVELS[index]["name"])
	_hud.set_best(_best.get(index, -1))


func _on_start_confirmed() -> void:
	_start_screen.close()
	_hud.show()
	_run_moves.clear()
	_show_level(0)
	_state = State.PLAYING
	_board.input_enabled = true


func _on_solved(moves: int) -> void:
	_board.input_enabled = false
	_state = State.LEVEL_COMPLETE
	var previous: int = _best.get(_level_index, -1)
	var is_new_best := previous < 0 or moves < previous
	if is_new_best:
		_best[_level_index] = moves
	_run_moves[_level_index] = moves
	_hud.set_best(_best[_level_index])
	var is_last := _level_index == Levels.LEVELS.size() - 1
	_level_complete.show_result(moves, _best[_level_index], is_new_best, is_last)


func _on_next_confirmed() -> void:
	_level_complete.close()
	if _level_index + 1 < Levels.LEVELS.size():
		_show_level(_level_index + 1)
		_state = State.PLAYING
		_board.input_enabled = true
	else:
		_state = State.ENDING
		var total := 0
		for moves: int in _run_moves.values():
			total += moves
		_ending_screen.show_result(Levels.LEVELS.size(), total)


func _on_play_again_confirmed() -> void:
	_ending_screen.close()
	_run_moves.clear()
	_show_level(0)
	_state = State.PLAYING
	_board.input_enabled = true
