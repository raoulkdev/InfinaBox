extends Node
## Where the player is in the game right now (autoload `GameState`).
##
## The game is always in exactly one "flow" state: starting up, in the menu,
## playing or paused. Everything that depends on that (showing the menu,
## freezing the world while paused) asks this script instead of keeping its
## own idea of it.

enum Flow { BOOT, MENU, PLAYING, PAUSED }

## The first level. Change this one line when the real first level exists.
const FIRST_LEVEL := "res://levels/sandbox.tscn"
const MAIN_MENU := "res://ui/main_menu.tscn"

var flow: Flow = Flow.BOOT


func _ready() -> void:
	# This script must keep running while the game is paused, to unpause it.
	process_mode = Node.PROCESS_MODE_ALWAYS


func show_menu() -> void:
	flow = Flow.MENU
	get_tree().paused = false
	SceneRouter.go_to(MAIN_MENU)


func start_new_game() -> void:
	flow = Flow.PLAYING
	get_tree().paused = false
	Events.game_started.emit()
	SceneRouter.go_to(FIRST_LEVEL)


func set_paused(paused: bool) -> void:
	if flow != Flow.PLAYING and flow != Flow.PAUSED:
		return
	flow = Flow.PAUSED if paused else Flow.PLAYING
	get_tree().paused = paused
	Events.game_paused.emit(paused)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("pause"):
		set_paused(flow != Flow.PAUSED)
