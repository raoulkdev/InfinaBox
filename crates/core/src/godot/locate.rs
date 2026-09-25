//! Finds a usable Godot: the managed install first, then a user-configured
//! path. Phase A Task C fills this in.

use std::path::Path;

use super::types::GodotStatus;

pub fn status(_app_data: &Path) -> GodotStatus {
    GodotStatus {
        installed: false,
        version: None,
        path: None,
        managed: false,
    }
}
