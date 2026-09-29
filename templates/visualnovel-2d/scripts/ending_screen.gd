extends Control
## The ending screen: "The End", which ending you got, how many endings
## the story has, and a Play again button.

signal play_again_pressed

@onready var _ending_label: Label = $Center/Panel/Margin/Rows/EndingName
@onready var _count_label: Label = $Center/Panel/Margin/Rows/Count
@onready var _play_again: Button = $Center/Panel/Margin/Rows/PlayAgain


func _ready() -> void:
	_play_again.pressed.connect(func() -> void: play_again_pressed.emit())
	hide()


## Shows the ending called `ending_name`; the story has `ending_total` endings.
func open(ending_name: String, ending_total: int) -> void:
	_ending_label.text = ending_name
	_count_label.text = "This was 1 of %d endings. Can you find the other?" % ending_total if ending_total > 1 else "Thanks for playing!"
	show()
	modulate.a = 0.0
	create_tween().tween_property(self, "modulate:a", 1.0, 0.6)
	_play_again.grab_focus()
