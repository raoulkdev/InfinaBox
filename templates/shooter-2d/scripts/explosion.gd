class_name Explosion
extends CPUParticles2D
## A one-off burst of particles, left behind when an enemy is destroyed.
## It plays as soon as it's added to the scene and removes itself when
## done. The enemy tints it with its own colour (the `color` property);
## size, speed and count are set on the node in scenes/explosion.tscn.


func _ready() -> void:
	emitting = true
	finished.connect(queue_free)
