extends Node3D
## Runs the game: the start screen, the gem count, the timer, the beacon
## and the ending. The flow is START -> PLAYING -> ENDED. Press Space to
## start, R to start over at any time.

# The other scripts this one talks to, so their functions can be called
# with type checking.
const Player := preload("res://scripts/player.gd")
const Hud := preload("res://scripts/hud.gd")
const StartScreen := preload("res://scripts/start_screen.gd")
const EndScreen := preload("res://scripts/end_screen.gd")
const CameraRig := preload("res://scripts/camera_rig.gd")

enum State { START, PLAYING, ENDED }

var _state: State = State.START
var _gems_collected: int = 0
var _gems_total: int = 0
var _elapsed: float = 0.0

@onready var _player: Player = $Player
@onready var _camera_rig: CameraRig = $Player/CameraRig
@onready var _hud: Hud = $Hud
@onready var _start_screen: StartScreen = $StartScreen
@onready var _end_screen: EndScreen = $EndScreen
@onready var _spawn: Marker3D = $Level/PlayerStart


func _ready() -> void:
	_player.set_spawn(_spawn.global_position)
	_player.global_position = _spawn.global_position
	_camera_rig.snap()
	_player.fell.connect(_on_player_fell)

	# Every gem and the beacon are in a group; listen to them.
	for gem in get_tree().get_nodes_in_group("gems"):
		gem.collected.connect(_on_gem_collected)
		_gems_total += 1
	for goal in get_tree().get_nodes_in_group("goal"):
		goal.reached.connect(_on_goal_reached)
		goal.touched_too_early.connect(_on_goal_touched_too_early)

	_show_start_screen()


func _process(delta: float) -> void:
	if _state == State.PLAYING:
		_elapsed += delta
		_hud.set_time(_elapsed)


func _unhandled_input(event: InputEvent) -> void:
	if _state == State.START and event.is_action_pressed("jump"):
		_start_game()
	elif _state != State.START and event.is_action_pressed("restart"):
		_start_game()


## The title screen: the camera drifts slowly round the island.
func _show_start_screen() -> void:
	_state = State.START
	_start_screen.show()
	_end_screen.hide()
	_hud.hide()
	_player.controls_enabled = false
	_camera_rig.idle_orbit = true
	_camera_rig.look_enabled = false


## Starts (or restarts) a run: everything back to how it was at the start.
func _start_game() -> void:
	get_viewport().set_input_as_handled()
	_gems_collected = 0
	_elapsed = 0.0
	for gem in get_tree().get_nodes_in_group("gems"):
		gem.reset()
	get_tree().call_group("goal", "set_active", false)
	_player.respawn()

	_start_screen.hide()
	_end_screen.hide()
	_hud.show()
	_hud.set_gems(0, _gems_total)
	_hud.set_time(0.0)
	_hud.show_message("Find all %d gems!" % _gems_total)

	_camera_rig.idle_orbit = false
	_camera_rig.look_enabled = true
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	_state = State.PLAYING

	# Give the key press that started the game a moment to pass, so the
	# Space that starts the game doesn't also make the player jump.
	await get_tree().create_timer(0.15).timeout
	if _state == State.PLAYING:
		_player.controls_enabled = true


func _on_gem_collected() -> void:
	if _state != State.PLAYING:
		return
	_gems_collected += 1
	_hud.set_gems(_gems_collected, _gems_total)
	if _gems_collected == _gems_total:
		get_tree().call_group("goal", "set_active", true)
		_hud.show_message("All gems found! Run to the glowing beacon.")


func _on_goal_touched_too_early() -> void:
	if _state == State.PLAYING:
		_hud.show_message("The beacon needs all %d gems first." % _gems_total)


func _on_goal_reached() -> void:
	if _state != State.PLAYING:
		return
	_state = State.ENDED
	_player.controls_enabled = false
	_camera_rig.look_enabled = false
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	_hud.hide()
	_end_screen.show_result(Hud.format_time(_elapsed), _gems_collected, _gems_total)
	_end_screen.show()


func _on_player_fell() -> void:
	if _state == State.PLAYING:
		_hud.show_message("Oops! Back to the start.")
