extends CharacterBody2D
## The player: a square that moves in all four directions with the arrow
## keys (Godot's built-in "ui_*" input actions, so no input setup needed)
## and stays inside the window. A starting point to change freely.

## Movement speed in pixels per second.
@export var speed: float = 300.0


func _physics_process(_delta: float) -> void:
	# A vector of length 0..1 from the arrow keys (or a gamepad stick).
	var direction := Input.get_vector("ui_left", "ui_right", "ui_up", "ui_down")
	velocity = direction * speed
	move_and_slide()

	# Keep the player's centre inside the window.
	var screen := get_viewport_rect().size
	position = position.clamp(Vector2.ZERO, screen)
