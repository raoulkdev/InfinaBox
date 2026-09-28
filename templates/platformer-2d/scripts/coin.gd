extends Area2D
## A coin: bobs gently in place, and when the player touches it, pops,
## sparkles and disappears. main.gd counts collected coins for the
## on-screen counter (every coin is in the "coins" group).

## Sent once, when the player collects this coin.
signal collected

## How far the coin bobs up and down, in pixels.
@export var bob_height: float = 4.0
## How many bobs per second.
@export var bob_speed: float = 0.5

var _time: float = 0.0

@onready var _visual: Node2D = $Visual
@onready var _sparkle: CPUParticles2D = $Sparkle


func _ready() -> void:
	# Start each coin at a different point of its bob, so they don't all
	# move in step.
	_time = fposmod(global_position.x / 97.0, 1.0)
	body_entered.connect(_on_body_entered)


func _process(delta: float) -> void:
	_time += delta * bob_speed
	_visual.position.y = sin(_time * TAU) * bob_height


func _on_body_entered(body: Node2D) -> void:
	if not body.is_in_group("player"):
		return
	collected.emit()
	set_process(false)  # stop bobbing, so the pop below can move it
	# Stop detecting the player, then pop: grow, float up and fade out.
	set_deferred("monitoring", false)
	_sparkle.restart()
	var tween := create_tween().set_parallel()
	tween.tween_property(_visual, "scale", Vector2(1.8, 1.8), 0.25)
	tween.tween_property(_visual, "modulate:a", 0.0, 0.25)
	tween.tween_property(_visual, "position:y", _visual.position.y - 24.0, 0.25)
	# Remove the coin once the sparkles have finished.
	await get_tree().create_timer(_sparkle.lifetime).timeout
	queue_free()
