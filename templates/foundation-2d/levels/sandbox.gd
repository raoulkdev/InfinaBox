extends Node2D
## An empty level to build in. Nothing here yet, on purpose: replace it with
## the game's real first level, then point `GameState.FIRST_LEVEL` at that.

@onready var _note: Label = %Note


func _ready() -> void:
	Events.game_paused.connect(_on_paused)


func _on_paused(paused: bool) -> void:
	_note.text = "Paused. Esc to continue." if paused else "Empty level. Esc to pause."
