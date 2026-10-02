extends Node2D
## The puzzle board: it reads a level from levels.gd, draws the room, and
## knows the rules - the player moves one square at a time, pushes one block
## at a time, blocks can't be pulled, and a block can't go into a wall or
## another block. It also keeps the history that makes Undo possible.
##
## The board is drawn in "board units" where one square is 64 wide, then the
## whole node is scaled and centered so any room fits the window.

# The other scripts this one talks to (so their functions are type-checked).
const Levels := preload("res://scripts/levels.gd")
const Block := preload("res://scripts/block.gd")
const Player := preload("res://scripts/player.gd")
const BlockScene := preload("res://scenes/block.tscn")

## Sent after every move or undo, with the number of moves made so far.
signal moves_changed(moves: int)
## Sent (a moment after the last block lands) when every target is covered.
signal solved(moves: int)

## Seconds a step takes to slide.
@export var slide_time: float = 0.12
## Seconds between steps while an arrow key is held down.
@export var repeat_delay: float = 0.16
## Seconds to wait after the last block lands before saying "solved", so the
## slide and the pop can finish.
@export var solved_delay: float = 0.45
## The part of the window the room may use: Rect2(x, y, width, height).
## The rest is left for the HUD text at the top and bottom.
@export var fit_area: Rect2 = Rect2(40, 96, 1200, 520)
## The biggest a square may be drawn, in pixels. Small rooms stop growing here.
@export var max_tile_size: float = 96.0

@export_group("Colors")
@export var floor_color: Color = Color("f2e9d8")
@export var floor_alt_color: Color = Color("e9dec8")
@export var wall_color: Color = Color("5b5f97")
@export var wall_top_color: Color = Color("7a7fc0")
@export var wall_side_color: Color = Color("434777")
@export var target_color: Color = Color("f4a259")

## How big one square is in board units. Everything is drawn at this size.
const TILE: float = 64.0
## The four moves, by input action.
const MOVES: Dictionary = {
	"move_up": Vector2i.UP,
	"move_down": Vector2i.DOWN,
	"move_left": Vector2i.LEFT,
	"move_right": Vector2i.RIGHT,
}

## True while the board listens to the keyboard and gamepad. main.gd turns it
## off behind the start, "Level complete!" and ending screens.
var input_enabled: bool = false
## Which level (0 is the first) is loaded.
var level_index: int = 0

var _cols: int = 0
var _rows: int = 0
var _floor: Dictionary = {}       # Vector2i -> true, every square you can stand on
var _walls: Array[Vector2i] = []  # wall squares next to the floor (the ones we draw)
var _targets: Array[Vector2i] = []
var _target_set: Dictionary = {}  # Vector2i -> true, the same, for quick lookups
var _block_pos: Array[Vector2i] = []
var _blocks: Array[Block] = []
var _player_pos: Vector2i = Vector2i.ZERO
## One entry per move: where the player and blocks were BEFORE that move.
var _history: Array[Dictionary] = []
var _solved: bool = false
var _cooldown: float = 0.0
## Goes up every time a level loads, so a late "solved" timer can tell that
## the level it belonged to has been restarted or replaced.
var _serial: int = 0

@onready var _player: Player = $Player


func _process(delta: float) -> void:
	# Holding a direction (or Z) keeps stepping, one step per repeat_delay.
	_cooldown -= delta
	if not input_enabled or _cooldown > 0.0:
		return
	for action: String in MOVES:
		if Input.is_action_pressed(action):
			_step(MOVES[action])
			return
	if Input.is_action_pressed("undo"):
		undo()
		_cooldown = repeat_delay


func _unhandled_input(event: InputEvent) -> void:
	if not input_enabled or event.is_echo():
		return
	for action: String in MOVES:
		if event.is_action_pressed(action):
			_step(MOVES[action])
			get_viewport().set_input_as_handled()
			return
	if event.is_action_pressed("undo"):
		undo()
		_cooldown = repeat_delay
		get_viewport().set_input_as_handled()


## Loads level number `index` (0 is the first) and lays it out. Returns false
## (and prints why) if the level text is broken.
func load_level(index: int) -> bool:
	_serial += 1
	level_index = index
	_solved = false
	_history.clear()
	_cooldown = 0.0
	for block in _blocks:
		block.queue_free()
	_blocks.clear()
	_block_pos.clear()
	_targets.clear()
	_target_set.clear()
	_floor.clear()
	_walls.clear()

	var rows: Array = Levels.LEVELS[index]["rows"]
	_rows = rows.size()
	_cols = 0
	for row: String in rows:
		_cols = maxi(_cols, row.length())

	# Read the text picture, one character at a time.
	var walls_seen: Array[Vector2i] = []
	var player_found := false
	for y in _rows:
		var row: String = rows[y]
		for x in row.length():
			var cell := Vector2i(x, y)
			var ch := row[x]
			if ch == "#":
				walls_seen.append(cell)
			if ch == "." or ch == "*" or ch == "+":
				_targets.append(cell)
				_target_set[cell] = true
			if ch == "$" or ch == "*":
				_block_pos.append(cell)
			if ch == "@" or ch == "+":
				_player_pos = cell
				player_found = true
	if not player_found:
		push_error("Level %d ('%s') has no player: add an @ to its rows in levels.gd." \
				% [index + 1, Levels.LEVELS[index]["name"]])
		return false
	if _block_pos.size() != _targets.size():
		push_error("Level %d ('%s') has %d blocks but %d targets: they must match." \
				% [index + 1, Levels.LEVELS[index]["name"], _block_pos.size(), _targets.size()])
		return false

	# The floor is everything the player can walk to from the start. That way
	# stray spaces outside the walls are simply not drawn.
	var stack: Array[Vector2i] = [_player_pos]
	_floor[_player_pos] = true
	while not stack.is_empty():
		var cell: Vector2i = stack.pop_back()
		for dir: Vector2i in MOVES.values():
			var next := cell + dir
			if _floor.has(next) or walls_seen.has(next) or not _inside(next):
				continue
			_floor[next] = true
			stack.append(next)
	# Only draw walls that touch the floor (even diagonally).
	for wall in walls_seen:
		for dx in range(-1, 2):
			for dy in range(-1, 2):
				if _floor.has(wall + Vector2i(dx, dy)) and not _walls.has(wall):
					_walls.append(wall)

	for pos in _block_pos:
		var block: Block = BlockScene.instantiate()
		add_child(block)
		block.place_at(_cell_center(pos))
		_blocks.append(block)
	_player.place_at(_cell_center(_player_pos))
	_update_targets(false, -1)
	_fit_to_window()
	queue_redraw()
	moves_changed.emit(0)
	return true


## How many moves have been made in this level (undo takes moves back).
func get_moves() -> int:
	return _history.size()


## Tries to move the player one square in `dir`, pushing a block if there is
## one. Returns true if the player moved.
func try_move(dir: Vector2i) -> bool:
	if _solved:
		return false
	var target := _player_pos + dir
	var pushed := _block_index_at(target)
	var blocked := not _floor.has(target)   # a wall (or outside the room)
	if pushed != -1 and not blocked:
		var beyond := target + dir
		# A block can't be pushed into a wall or into another block.
		blocked = not _floor.has(beyond) or _block_index_at(beyond) != -1
	if blocked:
		_player.bump(_cell_center(_player_pos), Vector2(dir), slide_time)
		return false

	# Remember how things were, so Undo can put them back.
	_history.append({"player": _player_pos, "blocks": _block_pos.duplicate()})
	_player_pos = target
	_player.slide_to(_cell_center(_player_pos), Vector2(dir), slide_time)
	if pushed != -1:
		_block_pos[pushed] += dir
		_blocks[pushed].slide_to(_cell_center(_block_pos[pushed]), slide_time)
	_update_targets(true, pushed)
	moves_changed.emit(_history.size())
	_check_solved()
	return true


## Takes back the last move (as many times as you like, back to the start).
## Returns false if there is nothing to undo.
func undo() -> bool:
	if _solved or _history.is_empty():
		return false
	var before: Dictionary = _history.pop_back()
	var old_blocks: Array[Vector2i] = []
	old_blocks.assign(before["blocks"])
	var old_player: Vector2i = before["player"]
	_player.slide_to(_cell_center(old_player), Vector2(old_player - _player_pos), slide_time)
	_player_pos = old_player
	for i in _blocks.size():
		if _block_pos[i] != old_blocks[i]:
			_blocks[i].slide_to(_cell_center(old_blocks[i]), slide_time)
	_block_pos = old_blocks
	_update_targets(false, -1)
	moves_changed.emit(_history.size())
	return true


# One step from the keyboard or gamepad, then wait before the next one.
func _step(dir: Vector2i) -> void:
	try_move(dir)
	_cooldown = repeat_delay


# Colors each block green if it sits on a target. `pushed` is the block that
# was just pushed (-1 for none): it gets the pop when it lands on a target.
func _update_targets(allow_pop: bool, pushed: int) -> void:
	for i in _blocks.size():
		_blocks[i].set_on_target(_target_set.has(_block_pos[i]), allow_pop and i == pushed)


# The level is solved when every target has a block on it.
func _check_solved() -> void:
	for target in _targets:
		if _block_index_at(target) == -1:
			return
	_solved = true
	var serial := _serial
	await get_tree().create_timer(solved_delay).timeout
	if serial == _serial:   # not restarted in the meantime
		solved.emit(_history.size())


func _block_index_at(cell: Vector2i) -> int:
	return _block_pos.find(cell)


func _inside(cell: Vector2i) -> bool:
	return cell.x >= 0 and cell.y >= 0 and cell.x < _cols and cell.y < _rows


# The middle of a square, in board units.
func _cell_center(cell: Vector2i) -> Vector2:
	return (Vector2(cell) + Vector2(0.5, 0.5)) * TILE


# Scales and centers the board so the whole room fits in fit_area.
func _fit_to_window() -> void:
	var room := Vector2(_cols, _rows) * TILE
	var fit := minf(fit_area.size.x / room.x, fit_area.size.y / room.y)
	fit = minf(fit, max_tile_size / TILE)
	scale = Vector2(fit, fit)
	position = fit_area.get_center() - room * fit / 2.0


func _draw() -> void:
	# Floor squares in a soft checkerboard.
	for cell: Vector2i in _floor:
		var color := floor_color if (cell.x + cell.y) % 2 == 0 else floor_alt_color
		draw_rect(Rect2(Vector2(cell) * TILE, Vector2(TILE, TILE)), color)
	# Targets: a ring with a dot, so an empty target is easy to spot.
	for cell in _targets:
		var center := _cell_center(cell)
		draw_arc(center, 16.0, 0.0, TAU, 32, target_color, 5.0, true)
		draw_circle(center, 5.0, target_color)
	# Walls: a light top edge and a darker front edge where the wall is the
	# first or last of a stack.
	for cell in _walls:
		var origin := Vector2(cell) * TILE
		draw_rect(Rect2(origin, Vector2(TILE, TILE)), wall_color)
		if not _walls.has(cell + Vector2i.DOWN):
			draw_rect(Rect2(origin + Vector2(0, TILE - 12.0), Vector2(TILE, 12.0)), wall_side_color)
		if not _walls.has(cell + Vector2i.UP):
			draw_rect(Rect2(origin, Vector2(TILE, 6.0)), wall_top_color)
