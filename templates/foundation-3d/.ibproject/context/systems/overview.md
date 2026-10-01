---
type: other
title: "How the game is organised"
status: draft
---

# How the game is organised

**In plain words.** The game starts with a small set of behind-the-scenes
helpers that every game needs: one that carries messages between parts of the
game, one that knows whether you are in the menu, playing or paused, one that
moves between screens, one that saves and loads, and one that remembers the
player's settings. Everything you add later plugs into these instead of being
tangled together, which is what lets a game grow large without falling over.

## The helpers

| Helper | Job | Card |
|---|---|---|
| Events | Carries messages between parts of the game | `systems/events.md` |
| Game flow | Menu, playing, paused | `systems/game-flow.md` |
| Scenes | Moves between screens and levels | `systems/scenes.md` |
| Saving | Saves and loads the game | `systems/saving.md` |
| Settings | Volume and fullscreen | `systems/settings.md` |

## Where things go

`entities/` things that act in the world, `systems/` game-wide rules,
`data/` content you can edit as files, `levels/` the places, `ui/` screens,
`assets/` art and sound. Each folder has a `README.md`.

## Technical details

The helpers are Godot **autoloads** (singletons registered in
`project.godot`, loaded in the order Events, UserSettings, SceneRouter,
GameState, SaveSystem). They live in `core/`. Gameplay code should depend on
`Events` and these autoloads only through their public functions. Content
that a designer tunes belongs in `data/` as `Resource` or JSON files.
