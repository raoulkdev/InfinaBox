extends RefCounted
## Every level of the game, written as plain text pictures. This is the ONLY
## place levels live, so to add or change a level you just edit the text
## below - no scenes to touch. The game plays them in this order.
##
## Each level is a "name" plus "rows" (one text line per row of the room).
## Each character is one square of the grid:
##
##   #  wall           (can't walk or push through)
##   .  target         (an empty spot a block should end up on)
##   $  block          (the player can push it)
##   *  block that already sits on a target
##   @  where the player starts
##   +  player that starts standing on a target
##   (space)  empty floor
##
## Rules for a level that can be finished:
##   - it has exactly one @ (or +);
##   - it has as many blocks as targets (a * counts as both);
##   - the room is closed in by walls, so nothing can be pushed outside;
##   - it can actually be solved! Test it yourself before you keep it.
## Rows may be different lengths. Bigger than about 16 wide x 12 tall will
## just be drawn smaller (see the fit settings in scripts/board.gd).

const LEVELS: Array[Dictionary] = [
	{
		"name": "First push",
		"rows": [
			"#######",
			"#     #",
			"# @$. #",
			"#     #",
			"#######",
		],
	},
	{
		"name": "Side by side",
		"rows": [
			"#######",
			"#.    #",
			"#.$ $ #",
			"#  @  #",
			"#######",
		],
	},
	{
		"name": "Three in a row",
		"rows": [
			"  ####  ",
			"###  ###",
			"#  $.  #",
			"# .$$@ #",
			"#  .   #",
			"########",
		],
	},
	{
		"name": "Mind the corner",
		"rows": [
			"########",
			"#     ##",
			"##  # .#",
			"#  $   #",
			"#.@$.$ #",
			"########",
		],
	},
	{
		"name": "The long way round",
		"rows": [
			"########",
			"#   .  #",
			"##$#   #",
			"#@ .   #",
			"# $# $ #",
			"##   . #",
			"########",
		],
	},
]
