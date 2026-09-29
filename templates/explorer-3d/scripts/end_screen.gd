extends CanvasLayer
## The ending screen: "You explored the island!" with the time taken and
## how to play again. The game (main.gd) shows it; the text and colours are
## in end_screen.tscn.

@onready var _stats: Label = %Stats


## Fills in the results. `time_text` is already formatted ("1:23").
func show_result(time_text: String, gems: int, total: int) -> void:
	_stats.text = "Time  %s\nGems  %d / %d" % [time_text, gems, total]
