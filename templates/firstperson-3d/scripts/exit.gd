extends Area3D
## The way out. When the player walks into the glowing doorway the game is
## won (main.gd listens for the `reached` signal and shows the ending).

signal reached

var _done := false

@onready var _glow: OmniLight3D = $Glow


func _ready() -> void:
	add_to_group("exits")
	body_entered.connect(_on_body_entered)


func _process(_delta: float) -> void:
	# A gentle pulse so the exit draws the eye.
	_glow.light_energy = 2.2 + sin(Time.get_ticks_msec() / 400.0) * 0.5


func _on_body_entered(body: Node3D) -> void:
	if body is Player and not _done:
		_done = true
		reached.emit()
