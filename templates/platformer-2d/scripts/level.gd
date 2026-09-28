extends Node2D
## A level: its platforms, coins, hazards, checkpoints and goal flag are
## all children of this node. The level starts at (0, 0) in its top-left
## corner and is `size` pixels wide and tall. The camera stays inside that
## area, and falling below its bottom edge counts as dying.
##
## Every level needs a child Marker2D called "PlayerStart" (where the player
## begins) and at least one goal flag.

## Width and height of the level, in pixels. The window is 1280×720.
@export var size: Vector2 = Vector2(4200, 720)
