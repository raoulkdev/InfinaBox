extends Control
## The dialogue box at the bottom of the screen: a name plate in the
## speaker's color, the words typed out one letter at a time, and a little
## bouncing "▼" once the whole line is showing.
##
## It only shows things. main.gd decides when to complete a line or move on
## (click, Space, Enter or gamepad A).

## How many letters appear per second. 0 shows each line at once.
@export var text_speed := 45.0

## The plate's shape; only its color changes per speaker.
var _plate_style := StyleBoxFlat.new()
var _typing: Tween
var _hint_bob: Tween

@onready var _plate: PanelContainer = $NamePlate
@onready var _name_label: Label = $NamePlate/Name
@onready var _text_label: Label = $Panel/Margin/Text
@onready var _hint: Control = $Hint


func _ready() -> void:
	_plate_style.set_corner_radius_all(14)
	_plate_style.set_border_width_all(3)
	_plate_style.border_color = Color("#fff8ec")
	_plate_style.content_margin_left = 22
	_plate_style.content_margin_right = 22
	_plate_style.content_margin_top = 6
	_plate_style.content_margin_bottom = 6
	_plate.add_theme_stylebox_override("panel", _plate_style)
	# The "▼" is drawn as a triangle, so it looks the same on every computer.
	_hint.draw.connect(_draw_hint)
	# It floats up and down forever (only visible when the line is complete).
	_hint_bob = create_tween().set_loops()
	_hint_bob.tween_property(_hint, "position:y", _hint.position.y + 7.0, 0.45).set_trans(Tween.TRANS_SINE)
	_hint_bob.tween_property(_hint, "position:y", _hint.position.y, 0.45).set_trans(Tween.TRANS_SINE)
	hide()


## Shows a line. `speaker` is empty for narration (no name plate).
func show_line(speaker: String, text: String, color: Color) -> void:
	show()
	_name_label.text = speaker
	_plate.visible = speaker != ""
	_plate_style.bg_color = color
	_name_label.add_theme_color_override("font_color", Color("#2a2540") if color.get_luminance() > 0.35 else Color.WHITE)
	# Shrink the plate back down to fit a shorter name.
	_plate.size = Vector2.ZERO
	# Narration is shown slightly softer, in italics-like grey-blue.
	_text_label.add_theme_color_override("font_color", Color("#fff8ec") if speaker != "" else Color("#d6d0f0"))
	_text_label.text = text
	_hint.visible = false
	if _typing:
		_typing.kill()
	if text_speed <= 0.0:
		_text_label.visible_ratio = 1.0
		_hint.visible = true
		return
	_text_label.visible_ratio = 0.0
	_typing = create_tween()
	_typing.tween_property(_text_label, "visible_ratio", 1.0, maxf(text.length() / text_speed, 0.05))
	_typing.finished.connect(func() -> void: _hint.visible = true)


## True while letters are still appearing.
func is_typing() -> bool:
	return _text_label.visible_ratio < 1.0


## Shows the whole line right now.
func complete() -> void:
	if _typing:
		_typing.kill()
	_text_label.visible_ratio = 1.0
	_hint.visible = true


## Hides the "▼" (while the player is choosing, for example).
func hide_hint() -> void:
	_hint.visible = false


func _draw_hint() -> void:
	var points := PackedVector2Array([Vector2(0, 0), Vector2(26, 0), Vector2(13, 18)])
	_hint.draw_colored_polygon(points, Color("#ffd54f"))
