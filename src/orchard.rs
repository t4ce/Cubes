//! Compact static assets and conservative pre-HS visibility. No cursor dependency.
extern crate alloc;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
pub const CUSTOM_RGB555: u32 = 1 << 15;
#[derive(Clone, Copy, Debug)]
pub struct Cube {
    pub center: [f32; 3],
    pub scale: f32,
    pub flags: u32,
}
pub struct Asset {
    pub name: &'static str,
    pub cubes: Vec<Cube>,
    pub radius: f32,
}

/// Arrange an asset pair on a shared base, without changing authored scales.
/// The combined seeds share the existing visibility pass and orbit bounds.
pub fn side_by_side(assets: &[Asset]) -> Result<Asset, &'static str> {
    let count: usize = assets.iter().map(|a| a.cubes.len()).sum();
    if count == 0 || count > 1024 {
        return Err("cubes-pair-seed-limit");
    }
    let mut cubes = Vec::with_capacity(count);
    let mut cursor = 0.0;
    for asset in assets {
        let left = asset
            .cubes
            .iter()
            .map(|c| c.center[0] - c.scale)
            .fold(f32::INFINITY, f32::min);
        let right = asset
            .cubes
            .iter()
            .map(|c| c.center[0] + c.scale)
            .fold(f32::NEG_INFINITY, f32::max);
        // Demo +Y points down: largest Y is the bottom of the asset.
        let bottom = asset
            .cubes
            .iter()
            .map(|c| c.center[1] + c.scale)
            .fold(f32::NEG_INFINITY, f32::max);
        for &cube in &asset.cubes {
            let mut placed = cube;
            placed.center[0] += cursor - left;
            placed.center[1] -= bottom;
            cubes.push(placed);
        }
        cursor += right - left + 1.0; // One world-unit clear gap between asset bounds.
    }
    let lo: [f32; 3] = core::array::from_fn(|a| {
        cubes
            .iter()
            .map(|c| c.center[a] - c.scale)
            .fold(f32::INFINITY, f32::min)
    });
    let hi: [f32; 3] = core::array::from_fn(|a| {
        cubes
            .iter()
            .map(|c| c.center[a] + c.scale)
            .fold(f32::NEG_INFINITY, f32::max)
    });
    for cube in &mut cubes {
        for a in 0..3 {
            cube.center[a] -= (lo[a] + hi[a]) * 0.5;
        }
    }
    Ok(Asset {
        name: if assets.len() == 1 {
            assets[0].name
        } else {
            "side-by-side pair"
        },
        cubes,
        radius: (0..3).map(|a| (hi[a] - lo[a]) * 0.5).sum(),
    })
}
pub fn decode(name: &'static str, bytes: &[u8]) -> Result<Asset, &'static str> {
    if bytes.len() < 16 || &bytes[..4] != b"CUBE" || bytes[4] != 1 {
        return Err("cubes-header");
    }
    if bytes[7] != 8 || bytes[11] != 4 {
        return Err("cubes-header");
    }
    if bytes[5] != 0 {
        return Err("cubes-static-only");
    }
    if bytes[6] == 0 || bytes[6] >= 100 {
        return Err("cubes-gap");
    }
    let count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
    let colors = bytes[10] as usize;
    let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if count == 0 || count > 1024 || colors == 0 || !unit.is_finite() || unit <= 0.0 {
        return Err("cubes-limits");
    }
    let start = 16 + colors * 4;
    if bytes.len() != start + count * 8 {
        return Err("cubes-length");
    }
    if bytes[16..start].chunks_exact(4).any(|c| c[3] != 255) {
        return Err("cubes-opaque-only");
    }
    let mut cubes = Vec::with_capacity(count);
    let mut occupied = BTreeSet::new();
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for r in bytes[start..].chunks_exact(8) {
        if !(1..=4).contains(&r[3]) || r[4] as usize >= colors || r[6] != 0 || r[7] != 0 {
            return Err("cubes-record");
        }
        let origin = [r[0] as i8 as i16, r[1] as i8 as i16, r[2] as i8 as i16];
        for x in 0..r[3] as i16 {
            for y in 0..r[3] as i16 {
                for z in 0..r[3] as i16 {
                    if !occupied.insert([origin[0] + x, origin[1] + y, origin[2] + z]) {
                        return Err("cubes-overlap");
                    }
                }
            }
        }
        let scale = (r[3] as f32 - bytes[6] as f32 / 100.0) * unit * 0.5;
        if !scale.is_finite() || scale < 0.001 {
            return Err("cubes-scale");
        }
        // Asset +Y is up; this demo's camera convention has -Y up.
        let center = [
            (origin[0] as f32 + r[3] as f32 * 0.5) * unit,
            -(origin[1] as f32 + r[3] as f32 * 0.5) * unit,
            (origin[2] as f32 + r[3] as f32 * 0.5) * unit,
        ];
        if center.iter().any(|x| !x.is_finite()) {
            return Err("cubes-position");
        }
        for a in 0..3 {
            lo[a] = lo[a].min(center[a] - scale);
            hi[a] = hi[a].max(center[a] + scale);
        }
        let p = &bytes[16 + r[4] as usize * 4..][..4];
        let flags = CUSTOM_RGB555
            | ((p[0] as u32 * 31 + 127) / 255)
            | (((p[1] as u32 * 31 + 127) / 255) << 5)
            | (((p[2] as u32 * 31 + 127) / 255) << 10);
        cubes.push(Cube {
            center,
            scale,
            flags,
        });
    }
    let midpoint: [f32; 3] = core::array::from_fn(|a| (lo[a] + hi[a]) * 0.5);
    let mut radius2: f32 = 0.0;
    for c in &mut cubes {
        for a in 0..3 {
            c.center[a] -= midpoint[a];
        }
        radius2 = radius2.max(
            c.center
                .iter()
                .map(|v| (v.abs() + c.scale) * (v.abs() + c.scale))
                .sum(),
        );
    }
    // L1 bound avoids requiring libm in the standalone parser tests.
    let radius = (0..3).map(|a| (hi[a] - lo[a]) * 0.5).sum::<f32>();
    if !radius2.is_finite() || !radius.is_finite() {
        return Err("cubes-bounds");
    }
    Ok(Asset {
        name,
        cubes,
        radius,
    })
}
fn corners(c: Cube) -> [[f32; 3]; 8] {
    core::array::from_fn(|i| {
        core::array::from_fn(|a| c.center[a] + if i & (1 << a) == 0 { -c.scale } else { c.scale })
    })
}
fn in_frustum(c: Cube, matrix: &[f32; 16]) -> bool {
    let clips: [[f32; 4]; 8] = corners(c).map(|p| {
        core::array::from_fn(|r| {
            matrix[r] * p[0] + matrix[4 + r] * p[1] + matrix[8 + r] * p[2] + matrix[12 + r]
        })
    });
    !(0..6).any(|plane| {
        clips.iter().all(|c| match plane {
            0 => c[0] < -c[3],
            1 => c[0] > c[3],
            2 => c[1] < -c[3],
            3 => c[1] > c[3],
            4 => c[2] < 0.0,
            _ => c[2] > c[3],
        })
    })
}
fn blocked_ray(eye: [f32; 3], end: [f32; 3], blocker: Cube) -> bool {
    // A 0.8-half-scale box lies strictly inside all 26 bevel planes.
    // Never use the outer AABB as an occluder: its corners are empty space.
    let half = blocker.scale * 0.8;
    if (0..3).all(|a| (eye[a] - blocker.center[a]).abs() <= half) {
        return false;
    }
    let mut entry: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for a in 0..3 {
        let d = end[a] - eye[a];
        let rel = eye[a] - blocker.center[a];
        if d.abs() < 1e-10 {
            if rel.abs() >= half {
                return false;
            }
        } else {
            let a = (-half - rel) / d;
            let b = (half - rel) / d;
            entry = entry.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    entry > 0.00001 && entry < exit - 0.00001 && exit < 0.99999
}
fn hidden(candidate: Cube, blocker: Cube, eye: [f32; 3]) -> bool {
    blocked_ray(eye, candidate.center, blocker)
        && corners(candidate)
            .iter()
            .all(|&p| blocked_ray(eye, p, blocker))
}
/// The shadow volume of a convex blocker is convex. If every outer-box
/// corner is strictly behind its inner box, the whole beveled cube is hidden.
/// This deliberately misses occlusion by multiple separate blockers.
pub fn visible(asset: &Asset, eye: [f32; 3], matrix: &[f32; 16]) -> Vec<usize> {
    let mut order: Vec<_> = (0..asset.cubes.len())
        .filter(|&i| in_frustum(asset.cubes[i], matrix))
        .collect();
    let distance = |i: usize| {
        asset.cubes[i]
            .center
            .iter()
            .zip(eye)
            .map(|(p, e)| (p - e) * (p - e))
            .sum::<f32>()
    };
    order.sort_by(|&a, &b| distance(a).total_cmp(&distance(b)));
    let mut kept: Vec<usize> = Vec::with_capacity(order.len());
    for i in order {
        if !kept
            .iter()
            .any(|&j| hidden(asset.cubes[i], asset.cubes[j], eye))
        {
            kept.push(i);
        }
    }
    kept
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pair_keeps_both_assets_sizes_colors_and_a_clear_gap() {
        let a = decode("orchard", include_bytes!("../Cube/cube_orchard.cubes")).unwrap();
        let b = decode("pine", include_bytes!("../Cube/plant_pine.cubes")).unwrap();
        let split = a.cubes.len();
        let originals = [a, b];
        let pair = side_by_side(&originals).unwrap();
        assert_eq!(pair.cubes.len(), 900);
        for (original, placed) in originals.iter().flat_map(|a| &a.cubes).zip(&pair.cubes) {
            assert_eq!(original.scale, placed.scale);
            assert_eq!(original.flags, placed.flags);
        }
        let right = pair.cubes[..split]
            .iter()
            .map(|c| c.center[0] + c.scale)
            .fold(f32::NEG_INFINITY, f32::max);
        let left = pair.cubes[split..]
            .iter()
            .map(|c| c.center[0] - c.scale)
            .fold(f32::INFINITY, f32::min);
        assert!((left - right - 1.0).abs() < 1e-5);
        let bottom = |cubes: &[Cube]| {
            cubes
                .iter()
                .map(|c| c.center[1] + c.scale)
                .fold(f32::NEG_INFINITY, f32::max)
        };
        assert!((bottom(&pair.cubes[..split]) - bottom(&pair.cubes[split..])).abs() < 1e-5);
    }
    #[test]
    fn orchard_visibility_removes_hidden_seeds_without_emptying_the_view() {
        let asset = decode("orchard", include_bytes!("../Cube/cube_orchard.cubes")).unwrap();
        // Perspective camera at -20Z looking along +Z, enclosing the asset.
        let matrix = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 1., 0., 0., 19.9, 20.,
        ];
        let kept = visible(&asset, [0., 0., -20.], &matrix);
        assert!(!kept.is_empty());
        assert!(kept.len() < asset.cubes.len());
        std::println!(
            "orchard visible={} source={}",
            kept.len(),
            asset.cubes.len()
        );
        let offscreen = Cube {
            center: [100., 0., 0.],
            scale: 1.,
            flags: 0,
        };
        assert!(!in_frustum(offscreen, &matrix));
        assert!(in_frustum(
            Cube {
                center: [0.; 3],
                ..offscreen
            },
            &matrix
        ));
    }
    #[test]
    fn orchard_decodes_all_four_sizes_and_rejects_bad_records() {
        let bytes = include_bytes!("../Cube/cube_orchard.cubes");
        let asset = decode("orchard", bytes).unwrap();
        assert!(asset.cubes.len() > 100);
        let mut sizes: Vec<_> = asset.cubes.iter().map(|c| c.scale.to_bits()).collect();
        sizes.sort();
        sizes.dedup();
        assert_eq!(sizes.len(), 4);
        assert!(asset.cubes.iter().all(|c| c.flags & CUSTOM_RGB555 != 0));
        for n in 0..bytes.len() {
            assert!(decode("bad", &bytes[..n]).is_err());
        }
        let mut bad = bytes.to_vec();
        bad[19] = 128;
        assert!(decode("alpha", &bad).is_err());
        bad = bytes.to_vec();
        bad[5] = 1;
        assert!(decode("rig", &bad).is_err());
    }
    #[test]
    fn strict_grid_pine_has_correct_spacing_and_gap() {
        let asset = decode("pine", include_bytes!("../Cube/plant_pine.cubes")).unwrap();
        assert_eq!(asset.cubes.len(), 423);
        assert!((asset.cubes[0].scale - 0.199).abs() < 1e-6);
        assert!((asset.cubes[0].center[1] - asset.cubes[1].center[1] - 0.4).abs() < 1e-6);
        assert!(asset.cubes.iter().all(|c| c.flags & CUSTOM_RGB555 != 0));
    }
    #[test]
    fn rejects_old_layout_overlap_reserved_bytes_and_invalid_gap() {
        let bytes = include_bytes!("../Cube/plant_pine.cubes");
        let start = 16 + bytes[10] as usize * 4;
        for gap in [0, 100, 255] {
            let mut bad = bytes.to_vec();
            bad[6] = gap;
            assert_eq!(decode("bad", &bad).err(), Some("cubes-gap"));
        }
        for offset in [6, 7] {
            let mut bad = bytes.to_vec();
            bad[start + offset] = 1;
            assert_eq!(decode("bad", &bad).err(), Some("cubes-record"));
        }
        let mut bad = bytes.to_vec();
        bad.copy_within(start..start + 8, start + 8);
        assert_eq!(decode("bad", &bad).err(), Some("cubes-overlap"));
        let mut old = bytes.to_vec();
        old[7] = 0;
        old[11] = 8;
        assert_eq!(decode("old", &old).err(), Some("cubes-header"));
    }
    #[test]
    fn occlusion_keeps_partial_silhouettes_and_inside_views() {
        let big = Cube {
            center: [0., 0., 4.],
            scale: 1.,
            flags: 0,
        };
        let small = Cube {
            center: [0., 0., 8.],
            scale: 0.3,
            flags: 0,
        };
        assert!(hidden(small, big, [0.; 3]));
        assert!(!hidden(
            Cube {
                center: [2., 0., 8.],
                ..small
            },
            big,
            [0.; 3]
        ));
        assert!(!hidden(small, big, big.center));
        assert!(!hidden(big, small, [0.; 3]));
    }
}
