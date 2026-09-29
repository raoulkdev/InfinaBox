extends Control
## The stage: where character portraits stand. Whoever speaks walks on (if
## they have a portrait) and is lit up; everyone else on stage is dimmed.
## Narration dims everyone. A new scene clears the stage.

## Where portraits stand, as a fraction of the screen width.
const SLOT_X := {"left": 0.2, "center": 0.5, "right": 0.8}
## The y position of the portraits' feet: just behind the dialogue box.
const FLOOR_Y := 520.0

# Character name -> CharacterPortrait.
var _portraits: Dictionary = {}


## Everyone leaves the stage.
func clear() -> void:
	for portrait: Node in _portraits.values():
		portrait.queue_free()
	_portraits.clear()


## Lights up `who` (adding their portrait if they have one) and dims the
## rest. `info` is the character's entry from the StoryParser.
func focus(who: String, info: Dictionary) -> void:
	if info.get("shape", "none") != "none" and not _portraits.has(who):
		var portrait := CharacterPortrait.new()
		add_child(portrait)
		portrait.setup(info["color"], info["shape"])
		portrait.enter(Vector2(size.x * SLOT_X[info["side"]], FLOOR_Y))
		_portraits[who] = portrait
	for name_on_stage: String in _portraits:
		(_portraits[name_on_stage] as CharacterPortrait).set_speaking(name_on_stage == who)


## Dims everyone (used for narration).
func dim_all() -> void:
	focus("", {})
