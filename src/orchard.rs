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
const OCCLUSION_W: usize = 320;
const OCCLUSION_H: usize = 180;
const INNER_SCALE: f32 = 0.8;
const CLIP_EPSILON: f32 = 1e-5;
const COVER_EPSILON: f32 = 1e-5;
const DEPTH_EPSILON: f32 = 1e-5;

#[derive(Clone, Copy, Debug, Default)]
pub struct VisibilityStats {
    pub source: usize,
    /// Cubes remaining after frustum rejection, including uncertain projections.
    pub frustum: usize,
    pub occluded: usize,
    pub visible: usize,
}

#[derive(Clone, Copy, Default)]
struct Point2 {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Projection {
    points: [Point2; 8],
    min: Point2,
    max: Point2,
    near: f32,
    far: f32,
}

struct Occluder {
    cube: Cube,
    min: Point2,
    max: Point2,
}

struct ProjectedCube {
    id: usize,
    projection: Option<Projection>,
}

/// Reused across frames and asset switches. No per-frame allocation after warmup.
pub struct VisibilityScratch {
    depth: Vec<f32>,
    projected: Vec<ProjectedCube>,
    visible: Vec<usize>,
    blockers: Vec<Occluder>,
}

impl VisibilityScratch {
    pub fn new() -> Self {
        Self {
            depth: alloc::vec![f32::INFINITY; OCCLUSION_W * OCCLUSION_H],
            projected: Vec::new(),
            visible: Vec::new(),
            blockers: Vec::new(),
        }
    }
}

fn clip_corners(c: Cube, matrix: &[f32; 16]) -> [[f32; 4]; 8] {
    corners(c).map(|p| {
        core::array::from_fn(|r| {
            matrix[r] * p[0] + matrix[4 + r] * p[1] + matrix[8 + r] * p[2] + matrix[12 + r]
        })
    })
}

fn in_frustum(clips: &[[f32; 4]; 8]) -> bool {
    // An invalid transform cannot prove that a cube is outside the view.
    if clips.iter().flatten().any(|v| !v.is_finite()) {
        return true;
    }
    !(0..6).any(|plane| {
        clips.iter().all(|c| match plane {
            0 => c[0] < -c[3] - CLIP_EPSILON,
            1 => c[0] > c[3] + CLIP_EPSILON,
            2 => c[1] < -c[3] - CLIP_EPSILON,
            3 => c[1] > c[3] + CLIP_EPSILON,
            4 => c[2] < -CLIP_EPSILON,
            _ => c[2] > c[3] + CLIP_EPSILON,
        })
    })
}

fn project(clips: &[[f32; 4]; 8]) -> Option<Projection> {
    let mut projection = Projection {
        points: [Point2::default(); 8],
        min: Point2 {
            x: f32::INFINITY,
            y: f32::INFINITY,
        },
        max: Point2 {
            x: f32::NEG_INFINITY,
            y: f32::NEG_INFINITY,
        },
        near: f32::INFINITY,
        far: f32::NEG_INFINITY,
    };
    for (i, c) in clips.iter().enumerate() {
        if c.iter().any(|v| !v.is_finite()) || c[3] <= CLIP_EPSILON {
            return None;
        }
        let [x, y, z] = [c[0] / c[3], c[1] / c[3], c[2] / c[3]];
        // Near-plane intersections (including camera-inside views) must draw
        // and must not stamp coverage: the opaque entry surface may be clipped.
        if !x.is_finite() || !y.is_finite() || !z.is_finite() || z <= CLIP_EPSILON {
            return None;
        }
        projection.points[i] = Point2 { x, y };
        projection.min.x = projection.min.x.min(x);
        projection.min.y = projection.min.y.min(y);
        projection.max.x = projection.max.x.max(x);
        projection.max.y = projection.max.y.max(y);
        projection.near = projection.near.min(z);
        projection.far = projection.far.max(z);
    }
    Some(projection)
}

/// Half-open cell ranges touching the padded outer screen rectangle. Padding
/// also visits both sides of an exact cell boundary. Clipping is to the viewport.
fn cells(p: &Projection) -> Option<([usize; 2], [usize; 2])> {
    let range = |lo: f32, hi: f32, size: usize| {
        let start = (((lo - COVER_EPSILON).clamp(-1.0, 1.0) + 1.0) * 0.5 * size as f32) as usize;
        let end = ((((hi + COVER_EPSILON).clamp(-1.0, 1.0) + 1.0) * 0.5 * size as f32) as usize
            + 1)
        .min(size);
        (start, end)
    };
    let (x0, x1) = range(p.min.x, p.max.x, OCCLUSION_W);
    let (y0, y1) = range(p.min.y, p.max.y, OCCLUSION_H);
    (x0 < x1 && y0 < y1).then_some(([x0, y0], [x1, y1]))
}

fn fully_occluded(depth: &[f32], p: &Projection) -> bool {
    let Some(([x0, y0], [x1, y1])) = cells(p) else {
        return false;
    };
    (y0..y1).all(|y| (x0..x1).all(|x| depth[y * OCCLUSION_W + x] + DEPTH_EPSILON < p.near))
}

// f64 hull predicates avoid overflow/cancellation with very large offscreen
// projections; coverage still keeps an explicit inward margin in NDC units.
fn cross(a: Point2, b: Point2, p: Point2) -> f64 {
    (b.x as f64 - a.x as f64) * (p.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (p.x as f64 - a.x as f64)
}

fn convex_hull(mut points: [Point2; 8]) -> ([Point2; 16], usize) {
    points.sort_unstable_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let mut hull = [Point2::default(); 16];
    let mut len = 0;
    for &p in &points {
        while len >= 2 && cross(hull[len - 2], hull[len - 1], p) <= 0.0 {
            len -= 1;
        }
        hull[len] = p;
        len += 1;
    }
    let lower = len;
    for &p in points[..7].iter().rev() {
        while len > lower && cross(hull[len - 2], hull[len - 1], p) <= 0.0 {
            len -= 1;
        }
        hull[len] = p;
        len += 1;
    }
    (hull, len - 1) // Last point repeats the first.
}

fn cell_fully_inside(hull: &[Point2], x: usize, y: usize) -> bool {
    let lo = Point2 {
        x: 2.0 * x as f32 / OCCLUSION_W as f32 - 1.0,
        y: 2.0 * y as f32 / OCCLUSION_H as f32 - 1.0,
    };
    let hi = Point2 {
        x: 2.0 * (x + 1) as f32 / OCCLUSION_W as f32 - 1.0,
        y: 2.0 * (y + 1) as f32 / OCCLUSION_H as f32 - 1.0,
    };
    (0..hull.len()).all(|i| {
        let a = hull[i];
        let b = hull[(i + 1) % hull.len()];
        let dx = b.x as f64 - a.x as f64;
        let dy = b.y as f64 - a.y as f64;
        // The corner minimizing this edge's half-plane equation proves all
        // four corners inside; convexity then proves the entire cell covered.
        let least = Point2 {
            x: if dy > 0.0 { hi.x } else { lo.x },
            y: if dx > 0.0 { lo.y } else { hi.y },
        };
        cross(a, b, least) > COVER_EPSILON as f64 * (dx.abs() + dy.abs())
    })
}

fn stamp(depth: &mut [f32], p: &Projection) {
    let (hull, len) = convex_hull(p.points);
    if len < 3 {
        return;
    }
    let Some(([x0, y0], [x1, y1])) = cells(p) else {
        return;
    };
    for y in y0..y1 {
        for x in x0..x1 {
            if cell_fully_inside(&hull[..len], x, y) {
                let cell = &mut depth[y * OCCLUSION_W + x];
                // A cell is opaque no later than the inner box's farthest
                // depth, irrespective of which face its rays intersect.
                *cell = cell.min(p.far);
            }
        }
    }
}

// The coarse grid can miss even a single blocker around cell boundaries.
// Retain the exact shadow-volume proof as a fallback, with cheap projected
// bounds rejection before any ray tests. This preserves sub-cell occlusion.
fn blocked_ray(eye: [f32; 3], end: [f32; 3], blocker: Cube) -> bool {
    // A 0.8-half-scale box lies strictly inside all 26 bevel planes.
    // Never use the outer AABB as an occluder: its corners are empty space.
    let half = blocker.scale * INNER_SCALE;
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
fn single_occluded(
    candidate: Cube,
    outer: &Projection,
    eye: [f32; 3],
    blockers: &[Occluder],
) -> bool {
    blockers.iter().any(|b| {
        b.min.x <= outer.min.x
            && b.min.y <= outer.min.y
            && b.max.x >= outer.max.x
            && b.max.y >= outer.max.y
            && hidden(candidate, b.cube, eye)
    })
}

/// Cull by collective screen coverage or an exact single-blocker shadow proof.
/// Both prove the entire outer candidate hidden; uncertainty retains a cube.
/// This is whole-seed visibility; each survivor still submits all 44 patches.
pub fn visible<'a>(
    scratch: &'a mut VisibilityScratch,
    asset: &Asset,
    eye: [f32; 3],
    matrix: &[f32; 16],
) -> (&'a [usize], VisibilityStats) {
    scratch.depth.fill(f32::INFINITY);
    scratch.projected.clear();
    scratch.visible.clear();
    scratch.blockers.clear();
    for (id, &cube) in asset.cubes.iter().enumerate() {
        let clips = clip_corners(cube, matrix);
        if in_frustum(&clips) {
            scratch.projected.push(ProjectedCube {
                id,
                projection: project(&clips),
            });
        }
    }
    scratch.projected.sort_unstable_by(|a, b| {
        let near = |c: &ProjectedCube| c.projection.map_or(f32::NEG_INFINITY, |p| p.near);
        near(a).total_cmp(&near(b)).then(a.id.cmp(&b.id))
    });
    let mut stats = VisibilityStats {
        source: asset.cubes.len(),
        frustum: scratch.projected.len(),
        ..VisibilityStats::default()
    };
    for candidate in &scratch.projected {
        if let Some(p) = &candidate.projection {
            if fully_occluded(&scratch.depth, p)
                || single_occluded(asset.cubes[candidate.id], p, eye, &scratch.blockers)
            {
                stats.occluded += 1;
                continue;
            }
            let cube = asset.cubes[candidate.id];
            let inner = Cube {
                scale: cube.scale * INNER_SCALE,
                ..cube
            };
            if let Some(p) = project(&clip_corners(inner, matrix)) {
                stamp(&mut scratch.depth, &p);
                scratch.blockers.push(Occluder {
                    cube,
                    min: p.min,
                    max: p.max,
                });
            }
        }
        scratch.visible.push(candidate.id);
    }
    stats.visible = scratch.visible.len();
    (&scratch.visible, stats)
}
#[cfg(test)]
#[path = "picking.rs"]
mod picking;
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
        let mut scratch = VisibilityScratch::new();
        let (kept, stats) = visible(&mut scratch, &asset, [0., 0., -20.], &matrix);
        assert_eq!(stats.source, asset.cubes.len());
        assert_eq!(stats.frustum, stats.occluded + stats.visible);
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
        assert!(!in_frustum(&clip_corners(offscreen, &matrix)));
        assert!(in_frustum(&clip_corners(
            Cube {
                center: [0.; 3],
                ..offscreen
            },
            &matrix
        )));
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
    const PERSPECTIVE: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 1., 0., 0., -0.1, 0.,
    ];

    fn cube(center: [f32; 3], scale: f32) -> Cube {
        Cube {
            center,
            scale,
            flags: CUSTOM_RGB555,
        }
    }

    fn scene(cubes: Vec<Cube>) -> Asset {
        Asset {
            name: "test",
            cubes,
            radius: 100.0,
        }
    }

    fn assert_stats(stats: VisibilityStats, ids: &[usize], source: usize) {
        assert_eq!(stats.source, source);
        assert_eq!(stats.frustum, stats.occluded + stats.visible);
        assert_eq!(stats.visible, ids.len());
        assert!(stats.frustum <= source);
    }

    #[test]
    fn four_disjoint_blockers_collectively_hide_a_cube() {
        // Stagger depth so separate solids overlap in projection. No one
        // blocker contains the candidate's silhouette; their union does.
        let candidate = cube([0., 0., 24.], 3.);
        let blockers = [
            cube([-0.7, -0.7, 4.], 1.),
            cube([1.1, -1.1, 7.], 1.7),
            cube([-1.2, 1.2, 10.5], 1.7),
            cube([1.8, 1.8, 15.], 2.6),
        ];
        for (i, &a) in blockers.iter().enumerate() {
            assert!(!hidden(candidate, a, [0.; 3]));
            for b in &blockers[i + 1..] {
                assert!(
                    (0..3).any(|axis| (a.center[axis] - b.center[axis]).abs() > a.scale + b.scale)
                );
            }
        }
        // Put the far candidate first to exercise depth sorting as well.
        let asset = scene(core::iter::once(candidate).chain(blockers).collect());
        let mut scratch = VisibilityScratch::new();
        let (ids, stats) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
        assert_eq!(ids, &[1, 2, 3, 4]);
        assert_eq!(stats.occluded, 1);
        assert_stats(stats, ids, 5);
    }

    #[test]
    fn partial_coverage_and_pixel_or_cell_gaps_keep_the_candidate() {
        for gap in [2. / 784., 2. / OCCLUSION_W as f32] {
            let candidate = cube([0., 0., 15.], 1.5);
            // Even the outer boxes leave this real screen-space crack.
            let left = cube([-1. - gap * 2.5, 0., 4.], 1.);
            let right = cube([1.75 + gap * 4.875, 0., 8.], 1.75);
            let asset = scene(alloc::vec![left, right, candidate]);
            let mut scratch = VisibilityScratch::new();
            let (ids, stats) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
            assert!(ids.contains(&2));
            assert_stats(stats, ids, 3);
        }
        let asset = scene(alloc::vec![cube([0., 0., 4.], 1.), cube([2., 0., 8.], 0.3)]);
        let mut scratch = VisibilityScratch::new();
        let (ids, _) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
        assert_eq!(ids, &[0, 1]);
    }

    #[test]
    fn buffer_requires_every_cell_and_strictly_nearer_depth() {
        let p = project(&clip_corners(cube([0., 0., 8.], 1.), &PERSPECTIVE)).unwrap();
        let mut depth = alloc::vec![p.near - 0.1; OCCLUSION_W * OCCLUSION_H];
        assert!(fully_occluded(&depth, &p));
        let ([x0, y0], _) = cells(&p).unwrap();
        depth[y0 * OCCLUSION_W + x0] = f32::INFINITY;
        assert!(!fully_occluded(&depth, &p));
        for z in [p.near, p.near - DEPTH_EPSILON * 0.5, p.near + 0.1] {
            depth.fill(z);
            assert!(!fully_occluded(&depth, &p));
        }
    }

    #[test]
    fn a_rear_blocker_cannot_hide_a_foreground_cube() {
        let near = cube([0., 0., 4.], 0.3);
        let far = cube([0., 0., 8.], 1.);
        let asset = scene(alloc::vec![far, near]);
        let mut scratch = VisibilityScratch::new();
        let (ids, _) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
        assert!(ids.contains(&1));
    }

    #[test]
    fn near_plane_and_camera_inside_cubes_never_supply_coverage() {
        for uncertain in [cube([0., 0., 0.15], 0.1), cube([0., 0., 0.], 1.)] {
            let asset = scene(alloc::vec![uncertain, cube([0., 0., 4.], 0.1)]);
            assert!(project(&clip_corners(uncertain, &PERSPECTIVE)).is_none());
            let mut scratch = VisibilityScratch::new();
            let (ids, stats) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
            assert_eq!(ids, &[0, 1]);
            assert_eq!(stats.occluded, 0);
        }
    }

    #[test]
    fn invalid_projection_keeps_seeds_and_empty_views_reset_scratch() {
        let asset = scene(alloc::vec![cube([0., 0., 4.], 1.), cube([0., 0., 8.], 0.3)]);
        let mut scratch = VisibilityScratch::new();
        assert_eq!(visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE).0, &[0]);
        for invalid in [f32::NAN, f32::INFINITY] {
            let mut matrix = PERSPECTIVE;
            matrix[0] = invalid;
            let (ids, stats) = visible(&mut scratch, &asset, [0.; 3], &matrix);
            assert_eq!(ids, &[0, 1]);
            assert_eq!(stats.occluded, 0);
        }
        let offscreen = scene(alloc::vec![cube([100., 0., 4.], 1.)]);
        let (ids, stats) = visible(&mut scratch, &offscreen, [0.; 3], &PERSPECTIVE);
        assert!(ids.is_empty());
        assert_eq!(
            (stats.source, stats.frustum, stats.occluded, stats.visible),
            (1, 0, 0, 0)
        );
        let lone = scene(alloc::vec![asset.cubes[1]]);
        assert_eq!(visible(&mut scratch, &lone, [0.; 3], &PERSPECTIVE).0, &[0]);
        assert!(
            visible(&mut scratch, &scene(Vec::new()), [0.; 3], &PERSPECTIVE)
                .0
                .is_empty()
        );
    }

    #[test]
    fn authored_size_tiers_keep_the_same_visibility_at_matching_distance() {
        for size in 1..=4 {
            let scale = (size as f32 - 0.01) * 0.2;
            let asset = scene(alloc::vec![
                cube([0., 0., 4. * scale], scale),
                cube([0., 0., 12. * scale], scale),
            ]);
            let mut scratch = VisibilityScratch::new();
            let (ids, stats) = visible(&mut scratch, &asset, [0.; 3], &PERSPECTIVE);
            assert_eq!(ids, &[0], "size tier {size}");
            assert_eq!(stats.occluded, 1);
        }
    }

    fn planes() -> impl Iterator<Item = ([f32; 3], f32)> {
        (-1i32..=1).flat_map(|x| {
            (-1i32..=1).flat_map(move |y| {
                (-1i32..=1).filter_map(move |z| {
                    let count = (x.abs() + y.abs() + z.abs()) as usize;
                    (count != 0)
                        .then_some(([x as f32, y as f32, z as f32], picking::PLANE_BOUNDS[count]))
                })
            })
        })
    }

    #[test]
    fn inner_box_corners_are_strictly_inside_every_picking_bevel_plane() {
        for p in corners(cube([0.; 3], INNER_SCALE)) {
            for (n, bound) in planes() {
                assert!((0..3).map(|a| n[a] * p[a]).sum::<f32>() < bound);
            }
        }
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        (0..3).map(|i| a[i] * b[i]).sum()
    }

    fn orbit_matrix(
        yaw: f32,
        pitch: f32,
        radius: f32,
        far: f32,
    ) -> ([f32; 3], [f32; 16], [[f32; 3]; 3]) {
        let eye = [
            radius * pitch.cos() * yaw.sin(),
            radius * pitch.sin(),
            radius * pitch.cos() * yaw.cos(),
        ];
        let forward = eye.map(|v| -v / radius);
        let right = [-yaw.cos(), 0., yaw.sin()];
        let up = [
            -pitch.sin() * yaw.sin(),
            pitch.cos(),
            -pitch.sin() * yaw.cos(),
        ];
        let fy = 1. / (core::f32::consts::PI / 6.).tan();
        let fx = fy / (784. / 441.);
        let depth = far / (far - 0.1);
        let mut matrix = [0.; 16];
        for a in 0..3 {
            matrix[a * 4] = right[a] * fx;
            matrix[a * 4 + 1] = up[a] * fy;
            matrix[a * 4 + 2] = forward[a] * depth;
            matrix[a * 4 + 3] = forward[a];
        }
        matrix[14] = (radius - 0.1) * depth;
        matrix[15] = radius;
        (
            eye,
            matrix,
            [forward, right.map(|v| v / fx), up.map(|v| v / fy)],
        )
    }

    // Independent ray intersection with the real convex bevel, after a cheap
    // AABB rejection. Used as a software image oracle, not by the culler.
    fn bevel_hit(cube: Cube, eye: [f32; 3], dir: [f32; 3], far: f32) -> Option<f32> {
        let origin: [f32; 3] = core::array::from_fn(|a| (eye[a] - cube.center[a]) / cube.scale);
        let dir = dir.map(|d| d / cube.scale);
        let mut entry = 0.1f32;
        let mut exit = far;
        for a in 0..3 {
            if dir[a].abs() < 1e-8 {
                if origin[a].abs() > 1. {
                    return None;
                }
            } else {
                let lo = (-1. - origin[a]) / dir[a];
                let hi = (1. - origin[a]) / dir[a];
                entry = entry.max(lo.min(hi));
                exit = exit.min(lo.max(hi));
            }
        }
        if entry > exit {
            return None;
        }
        for (n, bound) in planes() {
            let distance = bound - dot(n, origin);
            let denominator = dot(n, dir);
            if denominator.abs() < 1e-8 {
                if distance < 0. {
                    return None;
                }
            } else if denominator > 0. {
                exit = exit.min(distance / denominator);
            } else {
                entry = entry.max(distance / denominator);
            }
            if entry > exit {
                return None;
            }
        }
        Some(entry)
    }

    #[test]
    fn orbit_views_preserve_all_sampled_visible_bevel_surfaces() {
        let assets = [
            decode("orchard", include_bytes!("../Cube/cube_orchard.cubes")).unwrap(),
            decode("pine", include_bytes!("../Cube/plant_pine.cubes")).unwrap(),
        ];
        let asset = side_by_side(&assets).unwrap();
        let mut scratch = VisibilityScratch::new();
        let far = (asset.radius * 10.).max(100.);
        let mut rays_hitting = 0;
        let mut collectively_removed = 0;
        for view in 0..24 {
            let yaw = core::f32::consts::PI + (view % 12) as f32 * core::f32::consts::TAU / 12.;
            let pitch = if view < 12 { -0.15 } else { 0.7 };
            let radius = asset.radius * if view < 12 { 2.5 } else { 1.0 };
            let (eye, matrix, [forward, right, up]) = orbit_matrix(yaw, pitch, radius, far);
            let (ids, stats) = visible(&mut scratch, &asset, eye, &matrix);
            assert_stats(stats, ids, asset.cubes.len());
            let mut selected = alloc::vec![false; asset.cubes.len()];
            for &id in ids {
                selected[id] = true;
            }
            // Reference the former distance-sorted single-blocker behavior.
            let mut order: Vec<_> = (0..asset.cubes.len())
                .filter(|&i| in_frustum(&clip_corners(asset.cubes[i], &matrix)))
                .collect();
            order.sort_by(|&a, &b| {
                let distance = |i: usize| {
                    (0..3)
                        .map(|a| (asset.cubes[i].center[a] - eye[a]).powi(2))
                        .sum::<f32>()
                };
                distance(a).total_cmp(&distance(b))
            });
            let mut old: Vec<usize> = Vec::new();
            for i in order {
                if !old
                    .iter()
                    .any(|&j| hidden(asset.cubes[i], asset.cubes[j], eye))
                {
                    old.push(i);
                }
            }
            assert!(
                ids.len() <= old.len(),
                "view {view}: new={} old={}",
                ids.len(),
                old.len()
            );
            collectively_removed += old.len() - ids.len();
            if view < 12 {
                std::println!(
                    "orbit={view} S{} F{} O{} V{} old={}",
                    stats.source,
                    stats.frustum,
                    stats.occluded,
                    stats.visible,
                    old.len()
                );
            }
            for y in 0..54 {
                for x in 0..96 {
                    let sx = 2. * (x as f32 + 0.5) / 96. - 1.;
                    let sy = 2. * (y as f32 + 0.5) / 54. - 1.;
                    let ray = core::array::from_fn(|a| forward[a] + right[a] * sx + up[a] * sy);
                    let mut nearest = far;
                    let mut hit = None;
                    for (id, &c) in asset.cubes.iter().enumerate() {
                        if let Some(t) = bevel_hit(c, eye, ray, nearest) {
                            nearest = t;
                            hit = Some(id);
                        }
                    }
                    if let Some(id) = hit {
                        rays_hitting += 1;
                        assert!(
                            selected[id],
                            "visible cube {id} removed at view {view} pixel {x},{y}"
                        );
                    }
                }
            }
        }
        assert!(
            rays_hitting > 1000,
            "oracle must exercise actual visible geometry"
        );
        assert!(
            collectively_removed > 0,
            "collective culling must improve real assets"
        );
    }
}
