extends Control
## The history log (press H, or click "History"): everything said so far in
## this playthrough, so you can re-read a line you clicked past. Press H,
## Escape or Close to leave it.

@onready var _text: RichTextLabel = $Center/Panel/Margin/Rows/Text
@onready var _close: Button = $Center/Panel/Margin/Rows/Close

var _entries := 0


func _ready() -> void:
	_close.pressed.connect(close)
	hide()


## Forgets everything (a new playthrough).
func clear() -> void:
	_text.clear()
	_entries = 0


## Adds a spoken line. `speaker` is empty for narration.
func add_line(speaker: String, text: String, color: Color) -> void:
	if speaker == "":
		_text.append_text("[color=#b9b3d9][i]%s[/i][/color]\n\n" % _plain(text))
	else:
		_text.append_text("[b][color=#%s]%s[/color][/b]\n%s\n\n" % [color.to_html(false), _plain(speaker), _plain(text)])
	_entries += 1


## Adds a choice the player made.
func add_choice(text: String) -> void:
	_text.append_text("[color=#ffd54f]You chose: %s[/color]\n\n" % _plain(text))
	_entries += 1


func toggle() -> void:
	if visible:
		close()
	else:
		open()


func open() -> void:
	show()
	if _entries == 0:
		_text.text = "[color=#b9b3d9]Nothing has been said yet.[/color]"
	_close.grab_focus()
	# Show the newest lines first (once the box has been laid out).
	await get_tree().process_frame
	_text.scroll_to_line(maxi(_text.get_line_count() - 1, 0))


func close() -> void:
	hide()


func _unhandled_input(event: InputEvent) -> void:
	if visible and event.is_action_pressed("ui_cancel"):
		get_viewport().set_input_as_handled()
		close()


# Story text can't be allowed to sneak in formatting: turn "[" into a plain
# bracket.
func _plain(text: String) -> String:
	return text.replace("[", "[lb]")
