---
type: mechanic
title: Coins
status: working
implemented_in: [scripts/coin.gd, scenes/objects/coin.tscn, scripts/main.gd, scripts/hud.gd, scenes/ui/hud.tscn, scenes/levels/level_1.tscn]
---

# Coins

Coins float in the level, bobbing gently. Touching one pops it with a
sparkle and adds it to the counter in the top-left corner ("3 / 13"). The
"Level complete!" screen says how many were found. Level 1 has 13 coins,
some placed to reward riskier jumps.

## Tuning values

In `scripts/coin.gd`:

| Value | Default | What it does |
|---|---|---|
| `bob_height` | 4 | How far a coin bobs, pixels |
| `bob_speed` | 0.5 | Bobs per second |

The pop (grow ×1.8, float up 24 px, fade over 0.25 s) is in
`_on_body_entered` in `coin.gd`; the sparkle is the `Sparkle` particles in
`coin.tscn`.

## How it works

Every coin is in the `coins` group. When a level loads, `main.gd` counts
the group and listens to each coin's `collected` signal, then updates the
HUD with `hud.gd`'s `set_coins()`.

## How to change it

- Add a coin: add a `coin.tscn` instance under `Coins` in the level, with
  a unique name and a position (the coin's center).
- Make coins do something (extra life, unlock a door): react in
  `main.gd`'s `_on_coin_collected()`.
