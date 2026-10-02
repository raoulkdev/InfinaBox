//! Simple image clean-up after generation, with the `image` crate:
//! resize, snap to a small palette (pixel art) and clear a near-uniform
//! background by flood fill from the corners. Not a machine-learning
//! background remover — say so where it is offered.

use std::collections::HashMap;

use anyhow::{Result, bail};
use image::{Rgba, RgbaImage, imageops};

/// Resizes to exactly `width` x `height` (Lanczos; nearest-neighbour when
/// `pixel_art`).
pub fn resize(img: &RgbaImage, width: u32, height: u32, pixel_art: bool) -> Result<RgbaImage> {
    if width == 0 || height == 0 {
        bail!("The size has to be at least 1 pixel wide and tall.");
    }
    if width > 8192 || height > 8192 {
        bail!("That size is too big. Keep it under 8192 pixels.");
    }
    if img.dimensions() == (width, height) {
        return Ok(img.clone());
    }
    let filter = if pixel_art { imageops::FilterType::Nearest } else { imageops::FilterType::Lanczos3 };
    Ok(imageops::resize(img, width, height, filter))
}

/// Reduces the image to at most `colors` colors (median cut). Transparent
/// pixels stay transparent. Deterministic: the same input always gives the
/// same output.
pub fn palette_snap(img: &RgbaImage, colors: usize) -> Result<RgbaImage> {
    if colors == 0 {
        bail!("A palette needs at least one colour.");
    }
    // Count each distinct opaque-ish colour, in a fixed order.
    let mut counts: HashMap<[u8; 3], u64> = HashMap::new();
    for p in img.pixels().filter(|p| p[3] != 0) {
        *counts.entry([p[0], p[1], p[2]]).or_insert(0) += 1;
    }
    let mut all: Vec<([u8; 3], u64)> = counts.into_iter().collect();
    all.sort();
    if all.is_empty() {
        return Ok(img.clone());
    }

    // Median cut: split the box with the widest channel range at the
    // count-weighted median until there are `colors` boxes (or nothing to split).
    let mut boxes: Vec<Vec<([u8; 3], u64)>> = vec![all];
    while boxes.len() < colors {
        let pick = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(i, b)| (widest(b).1, std::cmp::Reverse(*i)))
            .map(|(i, _)| i);
        let Some(i) = pick else { break };
        let mut b = boxes.swap_remove(i);
        let (channel, _) = widest(&b);
        b.sort_by_key(|(c, _)| (c[channel], *c));
        let total: u64 = b.iter().map(|(_, n)| n).sum();
        let mut acc = 0;
        let mut cut = 1;
        for (k, (_, n)) in b.iter().enumerate() {
            acc += n;
            if acc * 2 >= total {
                cut = (k + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let right = b.split_off(cut);
        boxes.push(b);
        boxes.push(right);
    }

    let mut palette: Vec<[u8; 3]> = boxes
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|(_, n)| n).sum();
            let avg = |ch: usize| {
                let sum: u64 = b.iter().map(|(c, n)| c[ch] as u64 * n).sum();
                ((sum + total / 2) / total) as u8
            };
            [avg(0), avg(1), avg(2)]
        })
        .collect();
    palette.sort();
    palette.dedup();

    let mut cache: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    let mut out = img.clone();
    for p in out.pixels_mut() {
        if p[3] == 0 {
            continue;
        }
        let key = [p[0], p[1], p[2]];
        let snapped = *cache.entry(key).or_insert_with(|| nearest(&palette, key));
        *p = Rgba([snapped[0], snapped[1], snapped[2], p[3]]);
    }
    Ok(out)
}

/// (channel index, range) of the channel with the widest spread in a box.
fn widest(b: &[([u8; 3], u64)]) -> (usize, u8) {
    let mut best = (0, 0u8);
    for ch in 0..3 {
        let lo = b.iter().map(|(c, _)| c[ch]).min().unwrap_or(0);
        let hi = b.iter().map(|(c, _)| c[ch]).max().unwrap_or(0);
        if hi - lo > best.1 {
            best = (ch, hi - lo);
        }
    }
    best
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> [u8; 3] {
    let dist = |p: &[u8; 3]| -> i32 {
        (0..3).map(|i| (p[i] as i32 - c[i] as i32).pow(2)).sum()
    };
    *palette.iter().min_by_key(|p| dist(p)).unwrap_or(&c)
}

/// Makes pixels connected to the four corners and within `tolerance` (0-255,
/// per channel) of the corner colour transparent. Only acts when at least
/// three of the four corners agree (within `tolerance` of each other), so a
/// picture that fills its frame is returned unchanged.
pub fn clear_background(img: &RgbaImage, tolerance: u8) -> Result<RgbaImage> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Ok(img.clone());
    }
    let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
    let close = |a: &Rgba<u8>, b: &Rgba<u8>| (0..3).all(|i| a[i].abs_diff(b[i]) <= tolerance);
    let colors: Vec<Rgba<u8>> = corners.iter().map(|&(x, y)| *img.get_pixel(x, y)).collect();
    // The corner colour most of the others agree with.
    let agree = |i: usize| colors.iter().filter(|c| close(c, &colors[i])).count();
    let reference = (0..4).max_by_key(|&i| (agree(i), std::cmp::Reverse(i))).unwrap_or(0);
    if agree(reference) < 3 {
        return Ok(img.clone());
    }

    let mut out = img.clone();
    let mut seen = vec![false; (w * h) as usize];
    for (i, &(cx, cy)) in corners.iter().enumerate() {
        if !close(&colors[i], &colors[reference]) {
            continue; // an outlier corner: probably part of the picture
        }
        let target = colors[i];
        let mut stack = vec![(cx, cy)];
        while let Some((x, y)) = stack.pop() {
            let idx = (y * w + x) as usize;
            if seen[idx] {
                continue;
            }
            let p = *img.get_pixel(x, y);
            if p[3] != 0 && !close(&p, &target) {
                continue;
            }
            seen[idx] = true;
            out.put_pixel(x, y, Rgba([p[0], p[1], p[2], 0]));
            if x > 0 {
                stack.push((x - 1, y));
            }
            if x + 1 < w {
                stack.push((x + 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if y + 1 < h {
                stack.push((x, y + 1));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distinct(img: &RgbaImage) -> usize {
        let mut set = std::collections::HashSet::new();
        for p in img.pixels().filter(|p| p[3] != 0) {
            set.insert([p[0], p[1], p[2]]);
        }
        set.len()
    }

    #[test]
    fn resize_gives_exact_size_and_nearest_keeps_hard_edges() {
        let mut img = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 0, 255, 255]));
        let big = resize(&img, 8, 8, true).unwrap();
        assert_eq!(big.dimensions(), (8, 8));
        assert_eq!(distinct(&big), 2);
        assert_eq!(*big.get_pixel(7, 0), Rgba([0, 0, 255, 255]));
        let smooth = resize(&img, 9, 5, false).unwrap();
        assert_eq!(smooth.dimensions(), (9, 5));
        assert!(resize(&img, 0, 4, false).is_err());
    }

    #[test]
    fn palette_snap_limits_colours_keeps_alpha_and_is_deterministic() {
        let mut img = RgbaImage::new(16, 16);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = if x == 0 && y == 0 {
                Rgba([9, 9, 9, 0])
            } else {
                Rgba([(x * 16) as u8, (y * 16) as u8, ((x + y) * 8) as u8, 255])
            };
        }
        assert!(distinct(&img) > 100);
        let a = palette_snap(&img, 16).unwrap();
        let b = palette_snap(&img, 16).unwrap();
        assert_eq!(a, b);
        assert!(distinct(&a) <= 16 && distinct(&a) > 4);
        assert_eq!(a.get_pixel(0, 0)[3], 0);
        assert!(a.pixels().skip(1).all(|p| p[3] == 255));
    }

    #[test]
    fn palette_snap_with_few_colours_is_unchanged() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        img.put_pixel(2, 2, Rgba([200, 100, 50, 255]));
        assert_eq!(palette_snap(&img, 16).unwrap(), img);
        assert!(palette_snap(&img, 0).is_err());
    }

    #[test]
    fn clear_background_removes_connected_background_only() {
        // White background, a black ring with a white centre (enclosed white stays).
        let mut img = RgbaImage::from_pixel(9, 9, Rgba([250, 250, 250, 255]));
        for i in 2..=6 {
            for (x, y) in [(i, 2), (i, 6), (2, i), (6, i)] {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let out = clear_background(&img, 24).unwrap();
        assert_eq!(out.get_pixel(0, 0)[3], 0);
        assert_eq!(out.get_pixel(8, 8)[3], 0);
        assert_eq!(out.get_pixel(2, 2)[3], 255);
        assert_eq!(out.get_pixel(4, 4)[3], 255, "enclosed centre is not connected to a corner");
    }

    #[test]
    fn clear_background_uses_tolerance() {
        let mut img = RgbaImage::from_pixel(6, 6, Rgba([100, 100, 100, 255]));
        img.put_pixel(3, 3, Rgba([120, 100, 100, 255])); // within 24
        img.put_pixel(4, 4, Rgba([200, 100, 100, 255])); // far
        let out = clear_background(&img, 24).unwrap();
        assert_eq!(out.get_pixel(3, 3)[3], 0);
        assert_eq!(out.get_pixel(4, 4)[3], 255);
    }

    #[test]
    fn clear_background_needs_three_agreeing_corners() {
        let mut img = RgbaImage::from_pixel(6, 6, Rgba([255, 255, 255, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        img.put_pixel(5, 0, Rgba([0, 255, 0, 255]));
        assert_eq!(clear_background(&img, 24).unwrap(), img);
        // Three agree, one differs: the outlier stays.
        let mut img = RgbaImage::from_pixel(6, 6, Rgba([255, 255, 255, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        let out = clear_background(&img, 24).unwrap();
        assert_eq!(out.get_pixel(0, 0)[3], 255);
        assert_eq!(out.get_pixel(5, 5)[3], 0);
    }
}
