---
type: other
title: "Events"
status: draft
---

# Events

**In plain words.** A message board. When something happens (the game
started, the player paused), the part responsible posts a message, and any
part that cares reads it. Two parts never have to know about each other, so
you can add or remove features without breaking the rest.

## How you use it

Post a message, or listen for one. Add a new message type the first time two
parts need to talk.

## Technical details

`core/events.gd`, autoload `Events`. It only declares signals.
`Events.game_started.emit()` announces; `Events.game_started.connect(callable)`
listens. Name signals for what happened, past tense, not for the reaction.
Avoid passing nodes through signals; pass ids or plain data so listeners do
not hold on to freed objects.
