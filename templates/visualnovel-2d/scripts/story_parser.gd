class_name StoryParser
extends RefCounted
## Reads the story: the plain text files in the story/ folder.
##
## The format is explained at the top of every story file (and in AGENTS.md).
## Nothing here crashes on a mistake in the text: every problem becomes a
## readable sentence in `errors` ("story/cafe.txt, line 12: ..."), which the
## game shows on screen and prints to the output.
##
## After loading, `scenes` maps a scene name to its data:
##   { "name", "file", "line", "steps": [ ... ] }
## and each step is a Dictionary with a "type" ("background", "say",
## "narrate", "choices", "set", "if", "goto" or "end"), a "line" number, and
## the fields that type needs.

## Named backdrops for `background: <name>`: top color, bottom color.
const BACKGROUNDS := {
	"night": ["#141a3d", "#34477a"],
	"day": ["#7fc4ee", "#e8f5fc"],
	"dawn": ["#f2a6a0", "#fde4b9"],
	"sunset": ["#3b2a5a", "#f08a5d"],
	"cafe": ["#4a2f2a", "#c98a4b"],
	"park": ["#5f9f73", "#d8efc9"],
	"home": ["#5b5a83", "#c9c3e6"],
	"black": ["#050508", "#15151f"],
}
const SHAPES := ["circle", "square", "triangle", "diamond", "none"]
const SIDES := ["left", "center", "right"]
## The most choices one choice point can show.
const MAX_CHOICES := 4

## Scene name -> scene data (see above).
var scenes: Dictionary = {}
## Character name -> { "color": Color, "shape": String, "side": String }.
var characters: Dictionary = {}
## Every problem found, as a sentence a beginner can act on.
var errors: Array[String] = []
## The distinct `end:` names in the story, in the order they were read.
var ending_names: Array[String] = []


## Reads characters.txt and every other .txt in `folder`, then checks the
## whole story fits together (every scene that is jumped to exists, ...).
func load_folder(folder: String, start_scene: String) -> void:
	var dir := DirAccess.open(folder)
	if dir == null:
		errors.append("I can't open the story folder \"%s\"." % folder)
		return
	var texts: Dictionary = {}
	var characters_text := ""
	var files: Array[String] = []
	for file_name in dir.get_files():
		if file_name.get_extension() == "txt":
			files.append(file_name)
	files.sort()
	for file_name in files:
		var path := folder.path_join(file_name)
		var text := FileAccess.get_file_as_string(path)
		if file_name == "characters.txt":
			characters_text = text
		else:
			texts[_short_path(path)] = text
	if characters_text != "":
		load_characters(characters_text, _short_path(folder.path_join("characters.txt")))
	load_texts(texts, start_scene)


## Reads story files given as { "story/cafe.txt": "text..." } and checks the
## whole story. (load_folder calls this; it is also handy for trying out
## a story without files.)
func load_texts(texts: Dictionary, start_scene: String) -> void:
	for file_name: String in texts:
		parse_text(texts[file_name], file_name)
	if scenes.is_empty() and errors.is_empty():
		errors.append("I couldn't find any scenes. A story file needs a line like \"# scene: intro\".")
	_check_story(start_scene)


## Reads the character list: `Name: #color shape side` per line.
func load_characters(text: String, file_name: String) -> void:
	var line_number := 0
	for raw_line in text.split("\n"):
		line_number += 1
		var line := raw_line.strip_edges()
		if line == "" or line.begins_with("//"):
			continue
		var colon := line.find(":")
		if colon <= 0:
			_error(file_name, line_number, "I expected \"Name: #color shape side\", like \"Mira: #ffa45c circle left\".")
			continue
		var who := line.substr(0, colon).strip_edges()
		var parts := line.substr(colon + 1).strip_edges().split(" ", false)
		if parts.size() != 3:
			_error(file_name, line_number, "%s needs a color, a shape and a side, like \"#ffa45c circle left\"." % who)
			continue
		if not Color.html_is_valid(parts[0]):
			_error(file_name, line_number, "\"%s\" is not a color. Use something like #ffa45c." % parts[0])
			continue
		if not SHAPES.has(parts[1]):
			_error(file_name, line_number, "\"%s\" is not a shape. Use one of: %s." % [parts[1], ", ".join(SHAPES)])
			continue
		if not SIDES.has(parts[2]):
			_error(file_name, line_number, "\"%s\" is not a side. Use one of: %s." % [parts[2], ", ".join(SIDES)])
			continue
		characters[who] = {"color": Color.html(parts[0]), "shape": parts[1], "side": parts[2]}


## Reads one story file's text. `file_name` is only used in error messages.
func parse_text(text: String, file_name: String) -> void:
	var scene := {}
	var line_number := 0
	for raw_line in text.split("\n"):
		line_number += 1
		var line := raw_line.strip_edges()
		if line == "" or line.begins_with("//"):
			continue

		# "# scene: name" starts a new scene.
		if line.begins_with("#"):
			var rest := line.substr(1).strip_edges()
			if not rest.begins_with("scene:"):
				_error(file_name, line_number, "A line starting with # has to look like \"# scene: name\".")
				continue
			var scene_name := rest.substr(6).strip_edges()
			if not scene_name.is_valid_identifier():
				_error(file_name, line_number, "The scene name \"%s\" can only use letters, numbers and _ (no spaces), and can't start with a number." % scene_name)
				scene = {}
				continue
			if scenes.has(scene_name):
				_error(file_name, line_number, "There is already a scene called \"%s\" (in %s, line %d). Scene names have to be different." % [scene_name, scenes[scene_name]["file"], scenes[scene_name]["line"]])
				scene = {}
				continue
			scene = {"name": scene_name, "file": file_name, "line": line_number, "steps": []}
			scenes[scene_name] = scene
			continue

		if scene.is_empty():
			_error(file_name, line_number, "This line is before any scene. Start with \"# scene: name\" first.")
			continue
		var step := _parse_step(line, file_name, line_number)
		if step.is_empty():
			continue
		var steps: Array = scene["steps"]
		# Choice lines in a row make one choice point.
		if step["type"] == "choice":
			if not steps.is_empty() and steps[-1]["type"] == "choices":
				steps[-1]["options"].append(step["option"])
			else:
				steps.append({"type": "choices", "line": line_number, "options": [step["option"]]})
		else:
			steps.append(step)


# Turns one line of a scene into a step, or reports why it can't. Returns an
# empty Dictionary for a line that was reported.
func _parse_step(line: String, file_name: String, line_number: int) -> Dictionary:
	if line.begins_with(">"):
		return {"type": "narrate", "line": line_number, "text": line.substr(1).strip_edges()}

	if line.begins_with("?"):
		var arrow := line.rfind("->")
		if arrow < 0:
			_error(file_name, line_number, "A choice needs an arrow and a scene to jump to, like \"? Say hello -> hello\".")
			return {}
		var choice_text := line.substr(1, arrow - 1).strip_edges()
		var target := line.substr(arrow + 2).strip_edges()
		if choice_text == "" or target == "":
			_error(file_name, line_number, "A choice needs the words to show and a scene after the arrow, like \"? Say hello -> hello\".")
			return {}
		return {"type": "choice", "line": line_number, "option": {"text": choice_text, "target": target, "line": line_number}}

	if line.begins_with("set "):
		var flag := line.substr(4).strip_edges()
		if not flag.is_valid_identifier():
			_error(file_name, line_number, "\"set\" needs one word to remember, like \"set helped_theo\" (letters, numbers and _).")
			return {}
		return {"type": "set", "line": line_number, "flag": flag}

	if line.begins_with("if "):
		var arrow := line.rfind("->")
		if arrow < 0:
			_error(file_name, line_number, "\"if\" needs an arrow and a scene, like \"if helped_theo -> thanks\".")
			return {}
		var condition := line.substr(3, arrow - 3).strip_edges()
		var target := line.substr(arrow + 2).strip_edges()
		var negate := false
		if condition.begins_with("not "):
			negate = true
			condition = condition.substr(4).strip_edges()
		if not condition.is_valid_identifier() or target == "":
			_error(file_name, line_number, "\"if\" has to look like \"if helped_theo -> thanks\" (or \"if not helped_theo -> ...\").")
			return {}
		return {"type": "if", "line": line_number, "flag": condition, "negate": negate, "target": target}

	if line.begins_with("goto "):
		var target := line.substr(5).strip_edges()
		if target == "":
			_error(file_name, line_number, "\"goto\" needs a scene name, like \"goto cafe\".")
			return {}
		return {"type": "goto", "line": line_number, "target": target}

	var colon := line.find(":")
	if colon > 0:
		var word := line.substr(0, colon).strip_edges()
		var value := line.substr(colon + 1).strip_edges()
		if word == "background":
			if background_colors(value).is_empty():
				_error(file_name, line_number, "I don't know the background \"%s\". Use a color like #336699, two colors like #336699 #99ccff, or one of: %s." % [value, ", ".join(BACKGROUNDS.keys())])
				return {}
			return {"type": "background", "line": line_number, "spec": value}
		if word == "end":
			if value == "":
				_error(file_name, line_number, "\"end:\" needs the name of the ending, like \"end: A New Friend\".")
				return {}
			if not ending_names.has(value):
				ending_names.append(value)
			return {"type": "end", "line": line_number, "name": value}
		if value == "":
			_error(file_name, line_number, "%s doesn't say anything on this line." % word)
			return {}
		return {"type": "say", "line": line_number, "speaker": word, "text": value}

	_error(file_name, line_number, "I don't understand this line: \"%s\". Lines look like \"Name: what they say\", \"> narration\", \"? choice -> scene\", \"set flag\", \"if flag -> scene\", \"goto scene\" or \"end: name\"." % _clip(line))
	return {}


# Checks the story as a whole once everything is read.
func _check_story(start_scene: String) -> void:
	if not scenes.is_empty() and not scenes.has(start_scene):
		errors.append("The story should start at a scene called \"%s\", but there isn't one.%s" % [start_scene, _suggestion(start_scene)])
	for scene_name: String in scenes:
		var scene: Dictionary = scenes[scene_name]
		var steps: Array = scene["steps"]
		if steps.is_empty():
			_error(scene["file"], scene["line"], "The scene \"%s\" is empty." % scene_name)
			continue
		for step: Dictionary in steps:
			match step["type"]:
				"choices":
					var options: Array = step["options"]
					if options.size() < 2:
						_error(scene["file"], step["line"], "A choice needs at least two options (there is only one).")
					elif options.size() > MAX_CHOICES:
						_error(scene["file"], step["line"], "A choice can have at most %d options (there are %d)." % [MAX_CHOICES, options.size()])
					for option: Dictionary in options:
						_check_target(scene["file"], option["line"], option["target"])
				"if", "goto":
					_check_target(scene["file"], step["line"], step["target"])
		# A scene has to end by going somewhere, or the player would be stuck.
		var last_type: String = steps[-1]["type"]
		if not ["end", "goto", "choices"].has(last_type):
			_error(scene["file"], steps[-1]["line"], "The scene \"%s\" stops here without going anywhere. End it with a choice, a \"goto\" or an \"end:\"." % scene_name)


func _check_target(file_name: String, line_number: int, target: String) -> void:
	if not scenes.has(target):
		_error(file_name, line_number, "There is no scene called \"%s\".%s" % [target, _suggestion(target)])


# ' Did you mean "peek"?' when a scene name is a near miss, else "".
func _suggestion(wanted: String) -> String:
	var best := ""
	var best_distance := 3
	for scene_name: String in scenes:
		var distance := _edit_distance(wanted, scene_name)
		if distance < best_distance:
			best_distance = distance
			best = scene_name
	return "" if best == "" else " Did you mean \"%s\"?" % best


# How many single-letter changes turn `a` into `b`.
func _edit_distance(a: String, b: String) -> int:
	var previous: Array[int] = []
	for j in b.length() + 1:
		previous.append(j)
	for i in a.length():
		var current: Array[int] = [i + 1]
		for j in b.length():
			var cost := 0 if a[i] == b[j] else 1
			current.append(mini(mini(current[j] + 1, previous[j + 1] + 1), previous[j] + cost))
		previous = current
	return previous[b.length()]


func _error(file_name: String, line_number: int, message: String) -> void:
	errors.append("%s, line %d: %s" % [file_name, line_number, message])


func _clip(text: String) -> String:
	return text if text.length() <= 60 else text.substr(0, 57) + "..."


# "res://story/cafe.txt" -> "story/cafe.txt", friendlier in messages.
func _short_path(path: String) -> String:
	return path.trim_prefix("res://")


## The colors for a `background:` value, top then bottom: a name from
## BACKGROUNDS, one color ("#336699") or two ("#336699 #99ccff"). Empty when
## the value isn't understood.
static func background_colors(spec: String) -> Array[Color]:
	var result: Array[Color] = []
	var value := spec.strip_edges()
	var hex_list: Array = []
	if BACKGROUNDS.has(value):
		hex_list = BACKGROUNDS[value]
	else:
		for part in value.split(" ", false):
			hex_list.append(part)
	if hex_list.is_empty() or hex_list.size() > 2:
		return result
	for hex: String in hex_list:
		if not Color.html_is_valid(hex):
			return []
		result.append(Color.html(hex))
	if result.size() == 1:
		result.append(result[0])
	return result
