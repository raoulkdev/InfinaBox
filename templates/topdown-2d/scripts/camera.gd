extends Camera2D
## Follows the player smoothly, but never shows past the edges of the room
## the player is in (see room.gd).
##
## A room the size of the window (1280×720) is shown whole, so walking
## through a doorway slides the view over to the next room, like classic
## adventure games. A bigger room scrolls along with the player.

## The node to follow (the player).
@export var target: Node2D
## How quickly the view catches up. Higher is snappier, lower is floatier.
@export var follow_speed := 7.0
## How quickly a screen shake (after a hit) calms down, in pixels per second.
@export var shake_decay := 40.0

var _room: Room = null
var _shake := 0.0


func _ready() -> void:
	add_to_group("camera")
	# Start already in place instead of sliding in from the world's corner.
	if target:
		global_position = _goal()


func _process(delta: float) -> void:
	if target == null:
		return
	# Move a fraction of the remaining distance each frame: smooth, and the
	# same feel at any frame rate.
	global_position = global_position.lerp(_goal(), 1.0 - exp(-follow_speed * delta))

	# Screen shake: jitter the view a little, less and less over time.
	offset = Vector2(randf_range(-1.0, 1.0), randf_range(-1.0, 1.0)) * _shake
	_shake = move_toward(_shake, 0.0, shake_decay * delta)


## Shakes the view by up to `amount` pixels (called by the player when hit).
func shake(amount: float) -> void:
	_shake = maxf(_shake, amount)


## Where the camera wants to be: on the target, pushed back inside its room.
func _goal() -> Vector2:
	var goal := target.global_position
	var room := _room_at(goal)
	if room == null:
		return goal
	var area := room.get_global_rect()
	var view := get_viewport_rect().size / zoom
	# If the room is narrower (or shorter) than the view, center on it;
	# otherwise follow the target but stop at the room's edges.
	if area.size.x <= view.x:
		goal.x = area.get_center().x
	else:
		goal.x = clampf(goal.x, area.position.x + view.x / 2.0, area.end.x - view.x / 2.0)
	if area.size.y <= view.y:
		goal.y = area.get_center().y
	else:
		goal.y = clampf(goal.y, area.position.y + view.y / 2.0, area.end.y - view.y / 2.0)
	return goal


## The room containing `point`. Remembers the last one, so standing right in
## a doorway (on the line between two rooms) doesn't make the view jump.
func _room_at(point: Vector2) -> Room:
	if _room and _room.get_global_rect().has_point(point):
		return _room
	for node in get_tree().get_nodes_in_group("rooms"):
		var room := node as Room
		if room and room.get_global_rect().has_point(point):
			_room = room
			break
	return _room
