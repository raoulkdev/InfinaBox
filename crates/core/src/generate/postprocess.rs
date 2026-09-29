//! Simple image clean-up after generation, with the `image` crate:
//! resize, snap to a small palette (pixel art) and clear a near-uniform
//! background by flood fill from the corners. Not a machine-learning
//! background remover — say so where it is offered.
//!
//! Wave 0 stub — task GN.

use anyhow::{Result, bail};
use image::RgbaImage;

/// Resizes to exactly `width` x `height` (Lanczos; nearest-neighbour when
/// `pixel_art`).
pub fn resize(img: &RgbaImage, width: u32, height: u32, pixel_art: bool) -> Result<RgbaImage> {
    let _ = (img, width, height, pixel_art);
    bail!("not implemented yet (Phase C, task GN)")
}

/// Reduces the image to at most `colors` colors (median cut). Transparent
/// pixels stay transparent.
pub fn palette_snap(img: &RgbaImage, colors: usize) -> Result<RgbaImage> {
    let _ = (img, colors);
    bail!("not implemented yet (Phase C, task GN)")
}

/// Makes pixels connected to the four corners and within `tolerance` (0-255,
/// per channel) of the corner colour transparent.
pub fn clear_background(img: &RgbaImage, tolerance: u8) -> Result<RgbaImage> {
    let _ = (img, tolerance);
    bail!("not implemented yet (Phase C, task GN)")
}
