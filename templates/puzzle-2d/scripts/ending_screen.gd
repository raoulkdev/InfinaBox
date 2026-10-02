extends "res://scripts/screen.gd"
## The ending: shown after the last level is solved. The Play again button
## (or Space) starts over from level 1.

@onready var _summary_label: Label = %SummaryLabel


## Shows the ending. `levels` is how many levels were solved this time
## through and `moves` the total number of moves they took.
func show_result(levels: int, moves: int) -> void:
	_summary_label.text = "All %d levels done in %d moves." % [levels, moves]
	open()
