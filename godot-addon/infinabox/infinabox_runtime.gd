extends Node
## Runtime side of the InfinaBox addon, registered as the "InfinaBox"
## autoload. It is the game's debug channel to the InfinaBox app and is
## inert in release exports.
##
## For now it only announces itself: the app watches the game's stdout for
## the "[infinabox] ready <version>" line to know the game booted with this
## addon loaded. Screenshots and live tuning come later.

## Bumped together with the app's copy of this addon
## (infinabox_core::scaffold::ADDON_VERSION) whenever the addon changes.
const VERSION := "1"


func _ready() -> void:
	# Debug builds only: running from the editor or from InfinaBox counts,
	# a release export does not.
	if not OS.is_debug_build():
		return
	print("[infinabox] ready %s" % VERSION)
