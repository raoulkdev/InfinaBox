extends Node
## The first scene that runs. It waits a moment for the autoloads to be ready,
## then hands over to the main menu. Put one-time startup work here.


func _ready() -> void:
	GameState.show_menu.call_deferred()
