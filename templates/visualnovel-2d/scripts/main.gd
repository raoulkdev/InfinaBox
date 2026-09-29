extends Control
## The visual novel's conductor. It loads the story (story/*.txt), shows the
## start screen, then asks the StoryRunner for one thing at a time and shows
## it: a backdrop, a line of dialogue, a set of choices, an ending.
##
## Controls: click, Space, Enter or gamepad A finish the line being typed,
## then go to the next one. The number keys 1 to 4 (or the mouse, or the
## arrows + Enter) pick a choice. H opens the history log.

## The story folder: plain text files, see the top of any of them.
@export var story_folder := "res://story"
## The scene the story starts at.
@export var start_scene := "intro"
## The name and subtitle on the start screen.
@export var game_title := "Lantern Café"
@export var game_subtitle := "A short story for a rainy evening"
## Seconds to fade to black (and back) between scenes.
@export var fade_time := 0.5
## Seconds a backdrop takes to change color inside a scene.
@export var background_time := 0.8

enum Mode { TITLE, PLAYING, CHOOSING, ENDED, BROKEN }

## What the game is doing right now (the play-test reads this).
var mode := Mode.TITLE

var _story := StoryParser.new()
var _runner: StoryRunner
var _options: Array = []
var _busy := false
var _fade_in_pending := false
var _instant_background := false
var _fade: Tween

@onready var _background: Control = $Background
@onready var _stage: Control = $Stage
@onready var _dialogue: Control = $Dialogue
@onready var _choices: VBoxContainer = $ChoicesArea/Choices
@onready var _history_button: Button = $HistoryButton
@onready var _history: Control = $HistoryLog
@onready var _title_screen: Control = $TitleScreen
@onready var _ending_screen: Control = $EndingScreen
@onready var _error_screen: Control = $ErrorScreen
@onready var _fader: ColorRect = $Fader


func _ready() -> void:
	_story.load_folder(story_folder, start_scene)
	# Mistakes in the story are reported, never crash the game.
	if not _story.errors.is_empty():
		_show_problems(_story.errors)
		return
	_runner = StoryRunner.new(_story)
	_title_screen.start_pressed.connect(_start_story)
	_ending_screen.play_again_pressed.connect(_start_story)
	_history_button.pressed.connect(_history.toggle)
	_history.visibility_changed.connect(_history_visibility_changed)
	_title_screen.open(game_title, game_subtitle)
	_history_button.hide()
	_choices.hide()
	# Start with the first backdrop already in place behind the title.
	_background.set_colors(Color("#141a3d"), Color("#34477a"), 0.0)


func _unhandled_input(event: InputEvent) -> void:
	if _busy:
		return
	if (mode == Mode.PLAYING or mode == Mode.CHOOSING) and event.is_action_pressed("history"):
		get_viewport().set_input_as_handled()
		_history.toggle()
		return
	if _history.visible:
		return
	if mode == Mode.PLAYING:
		var click := event as InputEventMouseButton
		var clicked: bool = click != null and click.pressed and click.button_index == MOUSE_BUTTON_LEFT
		if event.is_action_pressed("advance") or clicked:
			get_viewport().set_input_as_handled()
			_advance()
	elif mode == Mode.CHOOSING:
		# The number keys pick a choice: 1 is the first one.
		var key := event as InputEventKey
		if key != null and key.pressed and not key.echo:
			var number: int = key.keycode - KEY_1
			if number >= 0 and number < _options.size():
				get_viewport().set_input_as_handled()
				_choose(number)


# Plays a fresh story from the beginning (Start and Play again).
func _start_story() -> void:
	if _busy:
		return
	_history.clear()
	_history.hide()
	var first := _runner.begin(start_scene)
	_busy = true
	mode = Mode.PLAYING
	_options = []
	# Fade to black (hiding whichever screen we came from), then play.
	await _fade_to(1.0)
	_title_screen.hide()
	_ending_screen.hide()
	_stage.clear()
	_dialogue.hide()
	_history_button.show()
	_fade_in_pending = true
	_instant_background = true
	await _play_on(first)


# The player asked for the next line: finish typing, or move on.
func _advance() -> void:
	if _dialogue.is_typing():
		_dialogue.complete()
		return
	_busy = true
	await _play_on(_runner.next())


# Handles `event`, then keeps asking the runner for more until something
# needs the player: a line to read, a choice to make, or the end.
func _play_on(first_event: Dictionary) -> void:
	_busy = true
	var event := first_event
	var waiting_for_player := false
	while not waiting_for_player:
		match event["type"]:
			"scene":
				# Only fade when we're in the middle of playing (Start already did).
				if not _fade_in_pending:
					await _fade_to(1.0)
					_stage.clear()
					_dialogue.hide()
					_fade_in_pending = true
					_instant_background = true
				event = _runner.next()
			"background":
				var colors := StoryParser.background_colors(event["spec"])
				_background.set_colors(colors[0], colors[1], 0.0 if _instant_background else background_time)
				event = _runner.next()
			"say", "narrate":
				await _reveal()
				_show_line(event)
				waiting_for_player = true
			"choices":
				await _reveal()
				_show_choices(event["options"])
				waiting_for_player = true
			"end":
				await _reveal()
				_show_ending(event["name"])
				waiting_for_player = true
			_:
				_show_problems([String(event.get("message", "Something went wrong in the story."))])
				waiting_for_player = true
	_busy = false


# Fades back in if we faded out for a scene change.
func _reveal() -> void:
	_instant_background = false
	if _fade_in_pending:
		_fade_in_pending = false
		await _fade_to(0.0)


func _show_line(event: Dictionary) -> void:
	mode = Mode.PLAYING
	var speaker: String = event.get("speaker", "")
	var text: String = event["text"]
	var info: Dictionary = _story.characters.get(speaker, {})
	var color: Color = info.get("color", Color("#b8b4c8"))
	if speaker == "":
		_stage.dim_all()
	else:
		_stage.focus(speaker, info)
	_dialogue.show_line(speaker, text, color)
	_history.add_line(speaker, text, color)


func _show_choices(options: Array) -> void:
	mode = Mode.CHOOSING
	_options = options
	_dialogue.hide_hint()
	for child in _choices.get_children():
		child.queue_free()
	for i in options.size():
		var button := Button.new()
		button.text = "%d   %s" % [i + 1, options[i]["text"]]
		button.custom_minimum_size = Vector2(760, 64)
		button.pressed.connect(_choose.bind(i))
		_choices.add_child(button)
	_choices.show()
	# Slide the choices in.
	_choices.modulate.a = 0.0
	create_tween().tween_property(_choices, "modulate:a", 1.0, 0.2)
	(_choices.get_child(0) as Button).grab_focus()


func _choose(index: int) -> void:
	if _busy or mode != Mode.CHOOSING:
		return
	_busy = true
	_history.add_choice(_options[index]["text"])
	var event := _runner.choose(index, _options)
	_options = []
	_choices.hide()
	for child in _choices.get_children():
		child.queue_free()
	mode = Mode.PLAYING
	await _play_on(event)


func _show_ending(ending_name: String) -> void:
	mode = Mode.ENDED
	_history_button.hide()
	_dialogue.hide()
	_ending_screen.open(ending_name, _story.ending_names.size())


# Shows the error screen and prints the same problems to the output.
func _show_problems(problems: Array[String]) -> void:
	mode = Mode.BROKEN
	for problem in problems:
		printerr("[story] ", problem)
	_title_screen.hide()
	_history_button.hide()
	_error_screen.show_errors(problems)


func _history_visibility_changed() -> void:
	# Back from the log while choosing: put the focus back on the choices.
	if not _history.visible and mode == Mode.CHOOSING and _choices.get_child_count() > 0:
		(_choices.get_child(0) as Button).grab_focus()


# Moves the black cover to `alpha` and waits until it gets there.
func _fade_to(alpha: float) -> void:
	if _fade:
		_fade.kill()
	if fade_time <= 0.0:
		_fader.modulate.a = alpha
		return
	_fade = create_tween()
	_fade.tween_property(_fader, "modulate:a", alpha, fade_time)
	await _fade.finished
