extends Node
## The player's own settings: volume and fullscreen (autoload `UserSettings`).
##
## Kept in a small file in the user folder and applied when the game starts.
## Add a setting by giving it a default in `DEFAULTS` and reading or writing
## it with `get_value` / `set_value`.

const FILE := "user://settings.cfg"
const SECTION := "game"
const DEFAULTS := {
	"master_volume": 1.0,   # 0.0 (silent) to 1.0 (full)
	"fullscreen": false,
}

var _config := ConfigFile.new()


func _ready() -> void:
	_config.load(FILE)  # a missing file is fine: defaults are used
	_apply()


func get_value(key: String) -> Variant:
	return _config.get_value(SECTION, key, DEFAULTS.get(key))


func set_value(key: String, value: Variant) -> void:
	_config.set_value(SECTION, key, value)
	_config.save(FILE)
	_apply()
	Events.settings_changed.emit()


func _apply() -> void:
	AudioServer.set_bus_volume_db(0, linear_to_db(float(get_value("master_volume"))))
	var mode := DisplayServer.WINDOW_MODE_FULLSCREEN if get_value("fullscreen") else DisplayServer.WINDOW_MODE_WINDOWED
	if DisplayServer.window_get_mode() != mode:
		DisplayServer.window_set_mode(mode)
