extends Area2D
## The goal flag at the end of a level: reaching it finishes the level.
## main.gd listens for `reached` (every goal is in the "goal" group) and
## shows the "Level complete!" screen.
##
## The flag's position is the bottom of its pole: put it on top of a
## platform (same y as the platform's top edge).

## Sent once, when the player reaches the flag.
signal reached

## How much the flag waves.
@export var wave_amount: float = 0.12
## How many waves per second.
@export var wave_speed: float = 1.2

var _reached: bool = false
var _time: float = 0.0

@onready var _flag: Polygon2D = $Flag
@onready var _confetti: CPUParticles2D = $Confetti


func _ready() -> void:
	body_entered.connect(_on_body_entered)


func _process(delta: float) -> void:
	# Waving: lean the flag back and forth.
	_time += delta * wave_speed
	_flag.skew = sin(_time * TAU) * wave_amount


func _on_body_entered(body: Node2D) -> void:
	if _reached or not body.is_in_group("player"):
		return
	_reached = true
	_confetti.restart()
	reached.emit()
