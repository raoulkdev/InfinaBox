# Systems

Rules that run the whole game and don't belong to one object: inventory, quests, dialogue, combat rules, day and night. A system is a script (often an autoload, see `project.godot`) that talks to the rest of the game through `Events`, so other parts never depend on it directly.
