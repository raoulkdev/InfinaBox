extends StaticBody2D
## Something the player can talk to or read: walk up to it and press E (or
## Space). Its lines are shown one page at a time in the dialogue box.
## Used by both the villager (scenes/npc.tscn) and the sign (scenes/sign.tscn).

## The name shown at the top of the dialogue box. Leave empty for signs.
@export var speaker_name := "Moss"
## What it says: one entry per page. Edit them in the Inspector (or here).
@export_multiline var lines: Array[String] = ["Hello!"]

@onready var _prompt: Control = $Prompt


func _ready() -> void:
	# The player looks for this group to find things to talk to.
	add_to_group("talkable")
	_prompt.visible = false
	# The "E" bubble bobs gently so it catches the eye.
	var bob := create_tween().set_loops()
	bob.tween_property(_prompt, "position:y", _prompt.position.y - 6.0, 0.5).set_trans(Tween.TRANS_SINE)
	bob.tween_property(_prompt, "position:y", _prompt.position.y, 0.5).set_trans(Tween.TRANS_SINE)


## Shows or hides the "E" bubble (the player calls this when it's nearest).
func set_prompt_visible(value: bool) -> void:
	_prompt.visible = value


## Called by the player when E is pressed nearby: opens the dialogue box.
func interact(_player: Node) -> void:
	get_tree().call_group("dialogue_box", "open", speaker_name, lines)
