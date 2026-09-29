---
type: mechanic
title: Pushing blocks
status: working
links: [mechanics/grid-movement.md, mechanics/undo-and-restart.md, mechanics/levels.md]
implemented_in: [scripts/board.gd, scripts/block.gd, scenes/block.tscn]
---

# Pushing blocks

Walking into a block pushes it one square in the same direction, and the
player follows into the square it left. One block at a time: a block can't
be pulled, and it can't be pushed into a wall or into another block (then
nothing moves, and the player just nudges). When a block lands on a target
it turns green, shows a tick and does a little pop. A level is solved when
every target has a block on it, a moment after the last one lands.

## Tuning values

In `scripts/block.gd`:

| Value | Default | What it does |
|---|---|---|
| `pop_size` | 1.25 | How big a block gets in its pop (1.0 = no pop) |
| `pop_time` | 0.22 | Seconds the pop takes |
| `body_color` / `border_color` | orange crate | Colors while off a target |
| `done_color` / `done_border_color` | green | Colors on a target |

In `scripts/board.gd`: `solved_delay` = 0.45 seconds between the last block
landing and the "Level complete!" panel.

## How it works

`board.gd` keeps one `Vector2i` per block in `_block_pos`, matching the
`Block` nodes in `_blocks`. `try_move()` looks at the square the player is
walking into; if a block is there it checks the square beyond it is floor
and free, then moves both. `_update_targets()` tells each block whether it
sits on a target, and only the block just pushed gets the pop.
`_check_solved()` emits `solved` when every target square has a block.

## How to change it

- New block behavior (heavy blocks, blocks that slide on ice): change the
  push rule in `try_move()` in `scripts/board.gd`.
- Look of the block: `_draw()` in `scripts/block.gd` (and its color exports).
