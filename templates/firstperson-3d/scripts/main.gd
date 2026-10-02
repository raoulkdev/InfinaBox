extends Node3D
## The game's rules in one place: the start screen, pausing, the clock, the
## key and door counters, and the ending. The level itself (walls, lamps,
## keys, doors, the exit) lives under the World node of scenes/main.tscn.
##
## This node keeps running while the game is paused ("Always"); the World
## node is the part that freezes.

enum State { START, PLAYING, PAUSED, ENDED }

## What the ending screen says.
@export var ending_title := "You escaped!"

var state := State.START
## Seconds spent playing (the clock only runs while State.PLAYING).
var elapsed := 0.0

var _doors_opened := 0

@onready var _player: Player = $World/Player
@onready var _hud: CanvasLayer = $HUD


func _ready() -> void:
	var total_keys := get_tree().get_nodes_in_group("keys").size()
	var total_doors := get_tree().get_nodes_in_group("doors").size()
	_hud.setup(ProjectSettings.get_setting("application/config/name"), total_keys, total_doors)

	# Keep the HUD in sync with the player and the level.
	_player.keys_changed.connect(func(found: int, _held: int) -> void: _hud.set_keys(found, total_keys))
	_player.prompt_changed.connect(_hud.set_prompt)
	_player.message.connect(_hud.show_message)
	for door in get_tree().get_nodes_in_group("doors"):
		door.opened.connect(_on_door_opened.bind(total_doors))
	# Every exit (scenes/exit.tscn adds itself to the "exits" group) wins.
	for exit in get_tree().get_nodes_in_group("exits"):
		exit.reached.connect(_on_exit_reached)

	_hud.start_screen.clicked.connect(_begin_play)
	# The game waits on the start screen until you click.
	_set_state(State.START)


func _process(delta: float) -> void:
	if state == State.PLAYING:
		elapsed += delta


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel") and state == State.PLAYING:
		_set_state(State.PAUSED)
	elif event.is_action_pressed("restart") and (state == State.PAUSED or state == State.ENDED):
		restart()


func _notification(what: int) -> void:
	# Clicking away from the window pauses, so the mouse isn't left grabbed.
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT and state == State.PLAYING:
		_set_state(State.PAUSED)


## Starts the game over from the beginning.
func restart() -> void:
	# Pausing survives a scene reload, so unpause first.
	get_tree().paused = false
	get_tree().reload_current_scene()


func _begin_play() -> void:
	_set_state(State.PLAYING)


func _set_state(new_state: State) -> void:
	state = new_state
	var playing := state == State.PLAYING
	get_tree().paused = not playing
	# While playing the mouse is captured: it turns the view and stays put.
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if playing else Input.MOUSE_MODE_VISIBLE
	match state:
		State.START:
			_hud.show_start_screen(false)
		State.PAUSED:
			_hud.show_start_screen(true)
		State.PLAYING:
			_hud.hide_start_screen()
		State.ENDED:
			_hud.show_end_screen(ending_title, format_time(elapsed))


func _on_door_opened(total_doors: int) -> void:
	_doors_opened += 1
	_hud.set_doors(_doors_opened, total_doors)


func _on_exit_reached() -> void:
	if state == State.PLAYING:
		_set_state(State.ENDED)


## 83.4 seconds becomes "1:23".
static func format_time(seconds: float) -> String:
	var whole := int(seconds)
	return "%d:%02d" % [floori(whole / 60.0), whole % 60]
