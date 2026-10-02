extends Node
## Saving and loading (autoload `SaveSystem`).
##
## Saves are plain JSON files in the player's user folder, one per slot, with
## a format version so old saves can still be read after the game changes.
##
## Anything that should be saved joins the group "saveable" and provides:
##   func save_key() -> String          # a name that stays the same forever
##   func save_data() -> Dictionary     # what to remember (numbers, text, lists)
##   func load_data(data: Dictionary)   # how to restore it
## so the save system never has to know what is inside the game.

const SAVE_DIR := "user://saves"
## Raise this when the shape of the saved data changes, and handle the older
## number in `_upgrade`.
const FORMAT_VERSION := 1
const DEFAULT_SLOT := "slot1"


func has_save(slot: String = DEFAULT_SLOT) -> bool:
	return FileAccess.file_exists(_path(slot))


func save_game(slot: String = DEFAULT_SLOT) -> bool:
	DirAccess.make_dir_recursive_absolute(SAVE_DIR)
	var data := {}
	for node in get_tree().get_nodes_in_group("saveable"):
		if node.has_method("save_key") and node.has_method("save_data"):
			data[node.save_key()] = node.save_data()
	var file := FileAccess.open(_path(slot), FileAccess.WRITE)
	if file == null:
		push_error("SaveSystem: could not write %s" % _path(slot))
		return false
	file.store_string(JSON.stringify({
		"version": FORMAT_VERSION,
		"saved_at": Time.get_datetime_string_from_system(true),
		"data": data,
	}, "\t"))
	Events.save_completed.emit(slot)
	return true


func load_game(slot: String = DEFAULT_SLOT) -> bool:
	if not has_save(slot):
		return false
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(_path(slot)))
	if typeof(parsed) != TYPE_DICTIONARY:
		push_error("SaveSystem: %s is not a readable save" % _path(slot))
		return false
	var save := _upgrade(parsed)
	var data: Dictionary = save.get("data", {})
	for node in get_tree().get_nodes_in_group("saveable"):
		if node.has_method("save_key") and node.has_method("load_data"):
			var key: String = node.save_key()
			if data.has(key):
				node.load_data(data[key])
	Events.load_completed.emit(slot)
	return true


func delete_save(slot: String = DEFAULT_SLOT) -> void:
	if has_save(slot):
		DirAccess.remove_absolute(_path(slot))


## Brings an older save up to the current format. Nothing to upgrade yet.
func _upgrade(save: Dictionary) -> Dictionary:
	return save


func _path(slot: String) -> String:
	return "%s/%s.json" % [SAVE_DIR, slot]
