---
type: character
title: The slime
status: working
implemented_in: [scenes/enemy.tscn, scripts/enemy.gd, scenes/main.tscn]
---

# The slime

A grumpy coral-red blob with angry eyebrows that hops around the hall, near
where the key lies. It wanders in random directions, rests now and then, and
goes back toward its starting spot if it strays too far. Touching it costs
the hero a heart. It can't be defeated yet.

Behavior and tuning: `mechanics/enemies-and-hearts.md`. Look: `Visual` in
`scenes/enemy.tscn` (body `#ef5a6f`).
