extends Area2D
## A checkpoint flag: when the player walks past it, the flag turns green
## and the player will respawn here after touching a hazard.
##
## The checkpoint's position is the bottom of its pole: put it on top of a
## platform (same y as the platform's top edge).

## The flag's color before and after the player reaches it.
@export var inactive_color: Color = Color("cbd5e1")
@export var active_color: Color = Color("34d399")

var _active: bool = false

@onready var _flag: Polygon2D = $Flag
@onready var _sparkle: CPUParticles2D = $Sparkle


func _ready() -> void:
	_flag.color = inactive_color
	body_entered.connect(_on_body_entered)


func _on_body_entered(body: Node2D) -> void:
	if _active or not body.is_in_group("player"):
		return
	_active = true
	body.set_checkpoint(global_position)
	# Turn green with a little pop and a puff of sparkles.
	_flag.color = active_color
	_sparkle.restart()
	var tween := create_tween()
	tween.tween_property(_flag, "scale", Vector2(1.4, 1.4), 0.1)
	tween.tween_property(_flag, "scale", Vector2.ONE, 0.2)
