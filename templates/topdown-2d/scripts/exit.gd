extends Area2D
## The way out. When the player steps onto it, the game is won (main.gd
## listens for the `reached` signal and shows the win screen).

signal reached

## How fast the swirl turns, in turns per second.
@export var spin_speed := 0.35

var _done := false

@onready var _swirl: Node2D = $Visual/Swirl


func _ready() -> void:
	add_to_group("exits")
	body_entered.connect(_on_body_entered)


func _process(delta: float) -> void:
	_swirl.rotation += TAU * spin_speed * delta


func _on_body_entered(body: Node2D) -> void:
	if body is Player and body.is_alive() and not _done:
		_done = true
		reached.emit()
