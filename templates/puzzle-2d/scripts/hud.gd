extends Control
## The always-visible text while you play: which level you are on, the move
## counter, your best for this level, and a reminder of the keys. main.gd
## tells it what to show; it holds no game logic of its own. To change the
## wording, edit the labels here or the text in scenes/ui/hud.tscn.

@onready var _level_label: Label = %LevelLabel
@onready var _moves_label: Label = %MovesLabel
@onready var _best_label: Label = %BestLabel


## Shows "Level 2 of 5" and the level's name.
func set_level(number: int, total: int, level_name: String) -> void:
	_level_label.text = "Level %d of %d  -  %s" % [number, total, level_name]


func set_moves(moves: int) -> void:
	_moves_label.text = "Moves: %d" % moves


## `best` is the fewest moves so far this session, or -1 if this level hasn't
## been solved yet.
func set_best(best: int) -> void:
	_best_label.text = "Best: %d" % best if best >= 0 else "Best: -"
