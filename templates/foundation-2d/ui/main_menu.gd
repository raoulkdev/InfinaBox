extends Control
## The main menu: a title and the buttons. Add buttons in the scene and
## connect them here.

@onready var _continue: Button = %Continue


func _ready() -> void:
	%Title.text = ProjectSettings.get_setting("application/config/name")
	_continue.visible = SaveSystem.has_save()
	%Play.pressed.connect(GameState.start_new_game)
	_continue.pressed.connect(_on_continue)
	%Quit.pressed.connect(get_tree().quit)
	%Play.grab_focus()


func _on_continue() -> void:
	GameState.start_new_game()
	# Wait for the level to load, then restore the saved game into it.
	await Events.scene_changed
	SaveSystem.load_game()
