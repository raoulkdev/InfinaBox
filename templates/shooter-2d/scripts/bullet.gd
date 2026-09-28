class_name Bullet
extends Area2D
## One bullet. It flies in a straight line, damages the first enemy it
## touches, and disappears when it hits something or after `lifetime`.
## The player sets `direction`, `speed` and `damage` when firing it.

## Seconds before a bullet that hit nothing disappears.
@export var lifetime: float = 1.2

var direction := Vector2.RIGHT
var speed := 950.0
var damage := 1


func _ready() -> void:
	rotation = direction.angle()
	# Walls and enemies are both "bodies"; see _on_body_entered.
	body_entered.connect(_on_body_entered)


func _physics_process(delta: float) -> void:
	position += direction * speed * delta
	lifetime -= delta
	if lifetime <= 0.0:
		queue_free()


func _on_body_entered(body: Node2D) -> void:
	if body is Enemy:
		(body as Enemy).take_hit(damage, direction)
	# Either an enemy or a wall: the bullet is used up.
	queue_free()
