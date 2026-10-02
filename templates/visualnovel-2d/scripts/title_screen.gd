extends Control
## The start screen: the game's title, a Start button and (on computers) a
## Quit button. Enter, Space or gamepad A press whichever button is focused;
## Start is focused at first.

signal start_pressed

@onready var _title: Label = $Center/Rows/Title
@onready var _subtitle: Label = $Center/Rows/Subtitle
@onready var start_button: Button = $Center/Rows/Buttons/Start
@onready var _quit_button: Button = $Center/Rows/Buttons/Quit


func _ready() -> void:
	start_button.pressed.connect(func() -> void: start_pressed.emit())
	_quit_button.pressed.connect(func() -> void: get_tree().quit())
	# A web page can't quit, so the button would do nothing there.
	_quit_button.visible = not OS.has_feature("web")


## Shows the screen with this title and subtitle.
func open(title: String, subtitle: String) -> void:
	_title.text = title
	_subtitle.text = subtitle
	show()
	start_button.grab_focus()


func _draw() -> void:
	# A paper lantern glowing behind the title: a few soft rings and a body.
	var center := Vector2(size.x / 2.0, 190.0)
	for ring in 6:
		draw_circle(center, 150.0 - ring * 22.0, Color("#ffb85c", 0.05 + ring * 0.02))
	draw_line(center + Vector2(0, -92), center + Vector2(0, -150), Color("#2a2540"), 4.0)
	var body := PackedVector2Array([center + Vector2(-46, -80), center + Vector2(46, -80), center + Vector2(60, 0), center + Vector2(46, 80), center + Vector2(-46, 80), center + Vector2(-60, 0)])
	draw_colored_polygon(body, Color("#ff9a3c"))
	draw_polyline(body + PackedVector2Array([body[0]]), Color("#7a3b1a"), 5.0, true)
	for x in [-24.0, 24.0]:
		draw_line(center + Vector2(x, -78), center + Vector2(x * 1.25, 78), Color("#7a3b1a", 0.6), 3.0)
	draw_rect(Rect2(center + Vector2(-50, -92), Vector2(100, 14)), Color("#2a2540"))
	draw_rect(Rect2(center + Vector2(-50, 78), Vector2(100, 14)), Color("#2a2540"))
