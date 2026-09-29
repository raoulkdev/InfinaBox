extends Control
## Shown instead of the game when the story has a mistake in it (a scene that
## doesn't exist, a line the game can't read, ...). It lists every problem in
## plain words so the story can be fixed; it never crashes the game.

@onready var _list: Label = $Center/Panel/Margin/Rows/Scroll/List


func _ready() -> void:
	hide()


## Shows the screen with one paragraph per problem.
func show_errors(problems: Array[String]) -> void:
	_list.text = "\n\n".join(problems)
	show()
