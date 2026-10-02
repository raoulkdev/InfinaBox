extends Node
## Runtime side of the InfinaBox addon, registered as the "InfinaBox"
## autoload. It is the game's debug channel to the InfinaBox app and is
## inert in release exports.
##
## It does two things:
##
## - It prints "[infinabox] ready <version>", which the app watches for to
##   know the game booted with this addon loaded.
## - When the app started the game for the AI to playtest (it sets
##   INFINABOX_PLAYTEST_PORT, _TOKEN and _DIR), it listens on that loopback
##   port for a list of steps (press an input, wait, take a picture, read a
##   value, check it) and answers with a report. Only the app, which holds the
##   token, can send them.

## Bumped together with the app's copy of this addon
## (infinabox_core::scaffold::ADDON_VERSION) whenever the addon changes.
const VERSION := "2"

const MAX_STEPS := 60
const MAX_WAIT := 30.0
const MAX_SHOTS_PER_REQUEST := 8
const MAX_TREE_NODES := 200
const MAX_REQUEST_BYTES := 1048576
const SHOT_WIDTH := 1280

var _server: TCPServer
var _token := ""
var _shot_dir := ""
var _shot_count := 0
var _clients: Array = []
var _busy := false


func _ready() -> void:
	# Debug builds only: running from the editor or from InfinaBox counts,
	# a release export does not.
	if not OS.is_debug_build():
		set_process(false)
		return
	# Keeps answering while the game is paused.
	process_mode = Node.PROCESS_MODE_ALWAYS
	print("[infinabox] ready %s" % VERSION)
	var port := OS.get_environment("INFINABOX_PLAYTEST_PORT")
	_token = OS.get_environment("INFINABOX_PLAYTEST_TOKEN")
	_shot_dir = OS.get_environment("INFINABOX_PLAYTEST_DIR")
	if not port.is_valid_int() or _token == "":
		set_process(false)
		return
	_server = TCPServer.new()
	var err := _server.listen(int(port), "127.0.0.1")
	if err != OK:
		print("[infinabox] playtest unavailable (port %s: %s)" % [port, error_string(err)])
		_server = null
		set_process(false)
		return
	print("[infinabox] playtest listening")


func _process(_delta: float) -> void:
	if _server == null:
		return
	while _server.is_connection_available():
		_clients.append({"peer": _server.take_connection(), "buf": PackedByteArray()})
	for c in _clients.duplicate():
		var peer: StreamPeerTCP = c["peer"]
		peer.poll()
		if peer.get_status() != StreamPeerTCP.STATUS_CONNECTED:
			_clients.erase(c)
			continue
		var n := peer.get_available_bytes()
		if n > 0:
			var got := peer.get_data(n)
			if got[0] == OK:
				c["buf"].append_array(got[1])
		if c["buf"].size() > MAX_REQUEST_BYTES:
			peer.disconnect_from_host()
			_clients.erase(c)
			continue
		var newline: int = c["buf"].find(10)
		if newline >= 0 and not _busy:
			var line: String = c["buf"].slice(0, newline).get_string_from_utf8()
			c["buf"] = c["buf"].slice(newline + 1)
			_busy = true
			_handle(c, line)


func _handle(client: Dictionary, line: String) -> void:
	var peer: StreamPeerTCP = client["peer"]
	var response := {"ok": false, "error": "Not a playtest request."}
	var parsed = JSON.parse_string(line)
	if typeof(parsed) == TYPE_DICTIONARY and str(parsed.get("token", "")) == _token:
		var steps = parsed.get("steps", [])
		if typeof(steps) != TYPE_ARRAY or steps.size() > MAX_STEPS:
			response = {"ok": false, "error": "Send between 1 and %d steps." % MAX_STEPS}
		else:
			var report := []
			var shots := 0
			for step in steps:
				if typeof(step) == TYPE_DICTIONARY and str(step.get("action", "")) == "screenshot":
					shots += 1
					if shots > MAX_SHOTS_PER_REQUEST:
						report.append(_fail("screenshot", "At most %d screenshots in one test." % MAX_SHOTS_PER_REQUEST))
						continue
				report.append(await _step(step))
			response = {"ok": true, "report": report}
	if peer.get_status() == StreamPeerTCP.STATUS_CONNECTED:
		peer.put_data((JSON.stringify(response) + "\n").to_utf8_buffer())
		peer.poll()
		peer.disconnect_from_host()
	_clients.erase(client)
	_busy = false


# ---- Steps ----

func _fail(action: String, message: String) -> Dictionary:
	return {"action": action, "ok": false, "message": message}


func _done(action: String, extra: Dictionary = {}) -> Dictionary:
	var out := {"action": action, "ok": true}
	out.merge(extra)
	return out


func _wait(seconds: float) -> void:
	seconds = clampf(seconds, 0.0, MAX_WAIT)
	if seconds <= 0.0:
		await get_tree().process_frame
	else:
		# Ignores pausing and time scale: a test waits in real time.
		await get_tree().create_timer(seconds, true, false, true).timeout


func _step(step) -> Dictionary:
	if typeof(step) != TYPE_DICTIONARY:
		return _fail("?", "A step must be an object with an action.")
	var action := str(step.get("action", ""))
	match action:
		"press":
			var input := str(step.get("input", ""))
			if not InputMap.has_action(input):
				return _fail(action, "There is no input action called '%s'. The game has: %s" % [input, ", ".join(_actions())])
			var down := InputEventAction.new()
			down.action = input
			down.pressed = true
			down.strength = 1.0
			Input.parse_input_event(down)
			await _wait(float(step.get("seconds", 0.1)))
			var up := InputEventAction.new()
			up.action = input
			up.pressed = false
			Input.parse_input_event(up)
			return _done(action, {"input": input})
		"key":
			var name := str(step.get("input", ""))
			var code := OS.find_keycode_from_string(name)
			if code == KEY_NONE:
				return _fail(action, "'%s' is not a key name (try Space, Enter, A, Left, Escape)." % name)
			var key_down := InputEventKey.new()
			key_down.keycode = code
			key_down.physical_keycode = code
			key_down.pressed = true
			Input.parse_input_event(key_down)
			await _wait(float(step.get("seconds", 0.1)))
			var key_up := InputEventKey.new()
			key_up.keycode = code
			key_up.physical_keycode = code
			key_up.pressed = false
			Input.parse_input_event(key_up)
			return _done(action, {"input": name})
		"click":
			var at := Vector2(float(step.get("x", 0)), float(step.get("y", 0)))
			var motion := InputEventMouseMotion.new()
			motion.position = at
			motion.global_position = at
			Input.parse_input_event(motion)
			var button := MOUSE_BUTTON_RIGHT if str(step.get("button", "left")) == "right" else MOUSE_BUTTON_LEFT
			var press := InputEventMouseButton.new()
			press.position = at
			press.global_position = at
			press.button_index = button
			press.pressed = true
			Input.parse_input_event(press)
			await _wait(0.05)
			var release := InputEventMouseButton.new()
			release.position = at
			release.global_position = at
			release.button_index = button
			release.pressed = false
			Input.parse_input_event(release)
			return _done(action, {"x": at.x, "y": at.y})
		"wait":
			await _wait(float(step.get("seconds", 0.5)))
			return _done(action)
		"screenshot":
			return await _screenshot(step)
		"tree":
			var root := _node(str(step.get("node", "")))
			if root == null:
				return _fail(action, "There is no node at '%s'." % str(step.get("node", "")))
			var budget := [MAX_TREE_NODES]
			var depth := clampi(int(step.get("depth", 3)), 0, 8)
			return _done(action, {"tree": _tree(root, depth, budget), "truncated": budget[0] <= 0})
		"get":
			var target := _node(str(step.get("node", "")))
			var prop := str(step.get("property", ""))
			if target == null:
				return _fail(action, "There is no node at '%s'." % str(step.get("node", "")))
			if not (prop in target):
				return _fail(action, "%s has no property called '%s'." % [target.name, prop])
			return _done(action, {"value": _jsonable(target.get(prop))})
		"expect":
			return _expect(step)
		"info":
			var scene := get_tree().current_scene
			return _done(action, {
				"actions": _actions(),
				"scene": scene.scene_file_path if scene else "",
				"fps": Engine.get_frames_per_second(),
				"paused": get_tree().paused,
				"seconds_running": snappedf(Time.get_ticks_msec() / 1000.0, 0.1),
				"viewport": [get_viewport().get_visible_rect().size.x, get_viewport().get_visible_rect().size.y],
			})
		_:
			return _fail(action, "Unknown action '%s'." % action)


func _screenshot(step: Dictionary) -> Dictionary:
	if _shot_dir == "":
		return _fail("screenshot", "The app gave the game no place to save pictures.")
	# Nothing is ever drawn without a display, so the wait below would hang.
	if DisplayServer.get_name() == "headless":
		return _fail("screenshot", "The game has no picture to capture (it is running without a display).")
	await RenderingServer.frame_post_draw
	var texture := get_viewport().get_texture()
	var img: Image = texture.get_image() if texture else null
	if img == null or img.is_empty():
		return _fail("screenshot", "The game has no picture to capture (it is running without a display).")
	if img.get_width() > SHOT_WIDTH:
		img.resize(SHOT_WIDTH, int(float(img.get_height()) * SHOT_WIDTH / img.get_width()))
	_shot_count += 1
	var label := str(step.get("label", "")).to_lower()
	var safe := ""
	for ch in label:
		if (ch >= "a" and ch <= "z") or (ch >= "0" and ch <= "9"):
			safe += ch
		elif safe != "" and not safe.ends_with("-"):
			safe += "-"
	safe = safe.substr(0, 30).rstrip("-")
	var path := "%s/shot-%03d%s.png" % [_shot_dir, _shot_count, ("-" + safe) if safe != "" else ""]
	var err := img.save_png(path)
	if err != OK:
		return _fail("screenshot", "The picture couldn't be saved (%s)." % error_string(err))
	return _done("screenshot", {"path": path})


func _expect(step: Dictionary) -> Dictionary:
	var node_path := str(step.get("node", ""))
	var target := _node(node_path)
	var op := str(step.get("op", "=="))
	var label := str(step.get("label", ""))
	if op == "exists" or str(step.get("property", "")) == "":
		var found := target != null
		return {"action": "expect", "ok": found, "message": "" if found else "There is no node at '%s'." % node_path, "label": label}
	if target == null:
		return {"action": "expect", "ok": false, "message": "There is no node at '%s'." % node_path, "label": label}
	var prop := str(step.get("property", ""))
	if not (prop in target):
		return {"action": "expect", "ok": false, "message": "%s has no property called '%s'." % [target.name, prop], "label": label}
	var actual = _jsonable(target.get(prop))
	# "x", "y" or "z" picks one part of a position or other vector.
	var part := str(step.get("component", ""))
	if part != "" and typeof(actual) == TYPE_ARRAY:
		var index: int = {"x": 0, "y": 1, "z": 2}.get(part, -1)
		if index < 0 or index >= actual.size():
			return {"action": "expect", "ok": false, "message": "%s has no '%s' part." % [prop, part], "label": label}
		actual = actual[index]
	var wanted = step.get("value")
	var passed := false
	match op:
		"==":
			passed = _same(actual, wanted)
		"!=":
			passed = not _same(actual, wanted)
		">", "<", ">=", "<=":
			if (typeof(actual) == TYPE_FLOAT or typeof(actual) == TYPE_INT) and (typeof(wanted) == TYPE_FLOAT or typeof(wanted) == TYPE_INT):
				var a := float(actual)
				var b := float(wanted)
				passed = (op == ">" and a > b) or (op == "<" and a < b) or (op == ">=" and a >= b) or (op == "<=" and a <= b)
			else:
				return {"action": "expect", "ok": false, "message": "'%s' needs numbers; %s is %s." % [op, prop, JSON.stringify(actual)], "label": label}
		_:
			return {"action": "expect", "ok": false, "message": "Unknown comparison '%s'." % op, "label": label}
	var out := {"action": "expect", "ok": passed, "actual": actual, "label": label}
	if not passed:
		out["message"] = "%s.%s is %s, expected %s %s." % [node_path, prop, JSON.stringify(actual), op, JSON.stringify(wanted)]
	return out


# ---- Helpers ----

func _actions() -> Array:
	var out := []
	for a in InputMap.get_actions():
		if not str(a).begins_with("ui_"):
			out.append(str(a))
	return out


## A node by path: absolute ("/root/GameState" for an autoload), or relative
## to the running scene ("Player/Sprite"); empty or "." is the scene itself.
func _node(path: String) -> Node:
	if path.begins_with("/"):
		return get_tree().root.get_node_or_null(path.trim_prefix("/root").trim_prefix("/")) if path != "/root" else get_tree().root
	var scene := get_tree().current_scene
	if scene == null:
		return null
	if path == "" or path == ".":
		return scene
	return scene.get_node_or_null(path)


func _tree(node: Node, depth: int, budget: Array) -> Dictionary:
	budget[0] -= 1
	var out := {"name": node.name, "class": node.get_class()}
	var script: Script = node.get_script()
	if script:
		out["script"] = script.resource_path
	if "visible" in node:
		out["visible"] = node.visible
	if "position" in node:
		out["position"] = _jsonable(node.position)
	if depth > 0 and node.get_child_count() > 0:
		var kids := []
		for child in node.get_children():
			if budget[0] <= 0:
				break
			kids.append(_tree(child, depth - 1, budget))
		out["children"] = kids
	elif node.get_child_count() > 0:
		out["children_hidden"] = node.get_child_count()
	return out


func _same(a, b) -> bool:
	var a_num := typeof(a) == TYPE_FLOAT or typeof(a) == TYPE_INT
	var b_num := typeof(b) == TYPE_FLOAT or typeof(b) == TYPE_INT
	if a_num and b_num:
		return absf(float(a) - float(b)) < 0.0001
	if typeof(a) == TYPE_ARRAY and typeof(b) == TYPE_ARRAY:
		if a.size() != b.size():
			return false
		for i in a.size():
			if not _same(a[i], b[i]):
				return false
		return true
	return typeof(a) == typeof(b) and a == b


## A value as JSON can hold it.
func _jsonable(v, depth: int = 0):
	match typeof(v):
		TYPE_NIL, TYPE_BOOL, TYPE_INT, TYPE_STRING:
			return v
		TYPE_FLOAT:
			return v if (not is_nan(v) and not is_inf(v)) else str(v)
		TYPE_VECTOR2, TYPE_VECTOR2I:
			return [v.x, v.y]
		TYPE_VECTOR3, TYPE_VECTOR3I:
			return [v.x, v.y, v.z]
		TYPE_COLOR:
			return "#" + v.to_html()
		TYPE_NODE_PATH, TYPE_STRING_NAME:
			return str(v)
		TYPE_ARRAY, TYPE_PACKED_STRING_ARRAY, TYPE_PACKED_INT32_ARRAY, TYPE_PACKED_FLOAT32_ARRAY:
			if depth > 3:
				return str(v)
			var out := []
			for i in mini(v.size(), 50):
				out.append(_jsonable(v[i], depth + 1))
			return out
		TYPE_DICTIONARY:
			if depth > 3:
				return str(v)
			var out := {}
			for k in v.keys():
				out[str(k)] = _jsonable(v[k], depth + 1)
			return out
		TYPE_OBJECT:
			if v is Node:
				return str(v.get_path())
			return str(v)
		_:
			return str(v)
