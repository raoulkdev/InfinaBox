extends Area2D
## A key lying on the floor. Walking over it picks it up: the player gets one
## key (shown in the top-left corner) that opens one locked door.

## How far the key floats up and down while waiting, in pixels.
@export var bob_height := 6.0

var _bob: Tween

@onready var _visual: Node2D = $Visual
@onready var _sparkles: CPUParticles2D = $Sparkles


func _ready() -> void:
	body_entered.connect(_on_body_entered)
	_bob = create_tween().set_loops()
	_bob.tween_property(_visual, "position:y", -bob_height, 0.6).set_trans(Tween.TRANS_SINE)
	_bob.tween_property(_visual, "position:y", 0.0, 0.6).set_trans(Tween.TRANS_SINE)


func _on_body_entered(body: Node2D) -> void:
	if body is Player:
		body.add_key()
		# Stop detecting (deferred: physics is busy during this callback).
		set_deferred("monitoring", false)
		_pop()


## The pickup "pop": the key swells and fades while sparkles burst out,
## then the node removes itself once the sparkles are gone.
func _pop() -> void:
	_bob.kill()
	_sparkles.restart()
	var tween := create_tween().set_parallel()
	tween.tween_property(_visual, "scale", Vector2(1.8, 1.8), 0.25).set_trans(Tween.TRANS_BACK)
	tween.tween_property(_visual, "modulate:a", 0.0, 0.25)
	await get_tree().create_timer(_sparkles.lifetime).timeout
	queue_free()
