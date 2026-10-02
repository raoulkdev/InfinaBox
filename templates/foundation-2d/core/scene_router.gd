extends Node
## Changes the current scene (autoload `SceneRouter`).
##
## Every move between screens and levels goes through `go_to`, so there is
## one place to add a loading screen, a fade, or "save before leaving" later.

var current_path: String = ""


func go_to(path: String) -> void:
	if not ResourceLoader.exists(path):
		push_error("SceneRouter: there is no scene at %s" % path)
		return
	var error := get_tree().change_scene_to_file(path)
	if error != OK:
		push_error("SceneRouter: could not open %s (error %d)" % [path, error])
		return
	current_path = path
	# The new scene is in place after a couple of frames.
	await get_tree().process_frame
	await get_tree().process_frame
	Events.scene_changed.emit(path)
