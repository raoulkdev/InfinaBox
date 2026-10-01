---
type: other
title: "Scenes"
status: draft
---

# Scenes

**In plain words.** One helper is in charge of moving the player from one
screen or level to another. Because everything goes through it, a loading
screen or a fade can be added in one place later.

## Technical details

`core/scene_router.gd`, autoload `SceneRouter`. `go_to(path)` checks the scene
exists, calls `change_scene_to_file`, and emits `Events.scene_changed(path)`
two frames later, when the new scene is in the tree.
