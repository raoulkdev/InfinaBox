extends Node2D
## The game's rules in one place: connects the player to the HUD, pauses
## the world while someone is talking, and shows the win / game-over
## screens. The world itself (rooms, characters, items) lives under the
## World node of scenes/main.tscn.

@onready var _player: Player = $World/Player
@onready var _hud: CanvasLayer = $HUD


func _ready() -> void:
	# Keep the HUD in sync with the player.
	_player.hearts_changed.connect(_hud.set_hearts)
	_player.keys_changed.connect(_hud.set_keys)
	_hud.set_hearts(_player.hearts, _player.max_hearts)
	_hud.set_keys(_player.keys)

	_player.died.connect(_on_player_died)
	# Every exit (scenes/exit.tscn adds itself to the "exits" group) wins.
	for exit in get_tree().get_nodes_in_group("exits"):
		exit.reached.connect(_on_exit_reached)

	# The world stands still while the dialogue box is open.
	_hud.dialogue_box.opened.connect(func() -> void: get_tree().paused = true)
	_hud.dialogue_box.closed.connect(func() -> void: get_tree().paused = false)


func _on_exit_reached() -> void:
	get_tree().paused = true
	_hud.show_end_screen("You escaped!", "You found the key and made it out.")


func _on_player_died() -> void:
	# Give the last hit a moment to land before the game-over screen.
	await get_tree().create_timer(0.8).timeout
	get_tree().paused = true
	_hud.show_end_screen("Oh no!", "You ran out of hearts.")
