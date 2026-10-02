extends "res://scripts/screen.gd"
## The "Level complete!" panel between levels. It shows how many moves the
## level took and the best (fewest) so far this session. The best is kept
## in memory only, so it starts fresh every time the game is launched.

@onready var _moves_label: Label = %MovesLabel
@onready var _best_label: Label = %BestLabel
@onready var _button: Button = %ActionButton


## Fills in the result and shows the panel. `is_last` changes the button to
## "Finish" because there is no next level.
func show_result(moves: int, best: int, is_new_best: bool, is_last: bool) -> void:
	_moves_label.text = "Moves: %d" % moves
	if is_new_best:
		_best_label.text = "New best!"
	else:
		_best_label.text = "Best so far: %d" % best
	_button.text = "Finish" if is_last else "Next level"
	open()
