class_name StoryRunner
extends RefCounted
## Walks through the story one step at a time. It remembers where we are and
## which flags ("set peeked") have been set, and knows nothing about the
## screen: main.gd asks for the next event and shows it.
##
## next() returns a Dictionary with a "type":
##   "scene"       we moved to a new scene ("name")
##   "background"  change the backdrop ("spec")
##   "say"         a character speaks ("speaker", "text")
##   "narrate"     narration ("text")
##   "choices"     show these choices ("options": [{"text", "target"}])
##   "end"         the story is over ("name" of the ending)
##   "error"       something is wrong ("message"); shown on screen

## The flags that have been set so far, e.g. { "peeked": true }.
var flags: Dictionary = {}

var _story: StoryParser
var _scene: Dictionary = {}
var _index := 0


func _init(story: StoryParser) -> void:
	_story = story


## The name of the scene we're in ("" before begin()).
func current_scene() -> String:
	return _scene.get("name", "")


## Starts (or restarts) the story at `scene_name`, forgetting all flags.
## Returns the first "scene" event, or an "error".
func begin(scene_name: String) -> Dictionary:
	flags.clear()
	return _jump(scene_name)


## The next thing to show. Quietly handles "set", "if" and "goto" itself.
func next() -> Dictionary:
	while true:
		var steps: Array = _scene.get("steps", [])
		if _index >= steps.size():
			return _error("The scene \"%s\" ran out of lines without an ending or a choice." % current_scene())
		var step: Dictionary = steps[_index]
		_index += 1
		match step["type"]:
			"set":
				flags[step["flag"]] = true
			"if":
				if flags.has(step["flag"]) != step["negate"]:
					return _jump(step["target"])
			"goto":
				return _jump(step["target"])
			_:
				return step
	return {}


## Picks option number `option_index` of the choices just shown. Returns the
## "scene" event for where it leads.
func choose(option_index: int, options: Array) -> Dictionary:
	return _jump(options[option_index]["target"])


func _jump(scene_name: String) -> Dictionary:
	if not _story.scenes.has(scene_name):
		return _error("There is no scene called \"%s\"." % scene_name)
	_scene = _story.scenes[scene_name]
	_index = 0
	return {"type": "scene", "name": scene_name}


func _error(message: String) -> Dictionary:
	printerr("[story] ", message)
	return {"type": "error", "message": message}
