extends PanelContainer
## The dialogue box at the bottom of the screen. Something calls open() with
## a name and a list of lines; the box types each line out, and E (or
## Space) shows the next one, then closes the box.
##
## It keeps working while the game is paused (process_mode is "Always"),
## because main.gd pauses the world while it's open.

signal opened
signal closed

## How many letters appear per second. 0 shows each page at once.
@export var letters_per_second := 60.0

var is_open := false

var _lines: Array[String] = []
var _page := 0
var _opened_frame := -1
var _typing: Tween

@onready var _name_label: Label = $Margin/Rows/Name
@onready var _text_label: Label = $Margin/Rows/Text
@onready var _hint_label: Label = $Margin/Rows/Hint


func _ready() -> void:
	# Talkable things find the box through this group.
	add_to_group("dialogue_box")
	hide()


## Opens the box with `lines`, one page each. `speaker` goes on top
## (hidden when empty, e.g. for signs).
func open(speaker: String, lines: Array[String]) -> void:
	if is_open or lines.is_empty():
		return
	is_open = true
	_lines = lines.duplicate()
	_page = 0
	# The E press that opened the box must not also turn the first page.
	_opened_frame = Engine.get_process_frames()
	_name_label.text = speaker
	_name_label.visible = speaker != ""
	show()
	# A small pop-in.
	pivot_offset = size / 2.0
	scale = Vector2(0.9, 0.9)
	modulate.a = 0.0
	var tween := create_tween().set_parallel()
	tween.tween_property(self, "scale", Vector2.ONE, 0.15).set_trans(Tween.TRANS_BACK)
	tween.tween_property(self, "modulate:a", 1.0, 0.1)
	_show_page()
	opened.emit()


func close() -> void:
	is_open = false
	hide()
	closed.emit()


func _unhandled_input(event: InputEvent) -> void:
	if not is_open or not event.is_action_pressed("interact"):
		return
	get_viewport().set_input_as_handled()
	if Engine.get_process_frames() == _opened_frame:
		return
	# Still typing: show the whole page first.
	if _text_label.visible_ratio < 1.0:
		_typing.kill()
		_text_label.visible_ratio = 1.0
		return
	_page += 1
	if _page >= _lines.size():
		close()
	else:
		_show_page()


func _show_page() -> void:
	_text_label.text = _lines[_page]
	var last := _page == _lines.size() - 1
	_hint_label.text = "E  Close" if last else "E  Next  (%d/%d)" % [_page + 1, _lines.size()]
	if _typing:
		_typing.kill()
	if letters_per_second <= 0.0:
		_text_label.visible_ratio = 1.0
		return
	_text_label.visible_ratio = 0.0
	_typing = create_tween()
	_typing.tween_property(_text_label, "visible_ratio", 1.0,
			_lines[_page].length() / letters_per_second)
