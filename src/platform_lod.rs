//! Render-only platform replacement. Authored geometry still drives walking,
//! picking, portals and placement. Filtering precedes per-cube visibility work.
use crate::orchard::{Asset, Cube};
use alloc::vec::Vec;

pub const ENABLED: bool = true;
pub const DETAIL_PLATFORMS: usize = 2;
const HULL_DRIFT_RADIANS: [f32; 3] = [0.038, 0.052, 0.031];
const HULL_DRIFT_SPEED: [f32; 3] = [0.13, 0.09, 0.11];
// Product of each bounded Euler rotation's maximum L-infinity norm is
// 1.12304 for the angles above. This envelope keeps rotated corners inside
// CPU visibility bounds without changing the rendered hull's authored size.
const HULL_VISIBILITY_SCALE: f32 = 1.13;

pub struct Hull {
    pub cube: Cube,
    /// Actual platform bounds, used for nearest-surface distance (not proxy size).
    pub lo: [f32; 3],
    pub hi: [f32; 3],
}
pub struct Metadata {
    pub hulls: &'static [Hull],
    /// One-based platform ID per decoded cube; zero means retain independently.
    pub owners: &'static [u8],
}
#[derive(Clone, Copy)]
enum Item {
    Source(usize),
    Hull(usize),
}
pub struct View {
    asset: Asset,
    items: Vec<Item>,
    metadata: Option<&'static Metadata>,
    nearest: [usize; DETAIL_PLATFORMS],
    source_count: usize,
}
impl View {
    pub fn new() -> Self {
        Self {
            asset: Asset {
                name: "platform-lod",
                cubes: Vec::new(),
                radius: 0.,
            },
            items: Vec::new(),
            metadata: None,
            nearest: [usize::MAX; DETAIL_PLATFORMS],
            source_count: usize::MAX,
        }
    }
    pub fn prepare(&mut self, source: &Asset, metadata: Option<&'static Metadata>, eye: [f32; 3]) {
        if metadata.is_none() {
            self.metadata = None;
            self.source_count = usize::MAX;
            return;
        }
        let mut nearest = [usize::MAX; DETAIL_PLATFORMS];
        let mut distances = [f32::INFINITY; DETAIL_PLATFORMS];
        if let Some(m) = metadata {
            for (id, h) in m.hulls.iter().enumerate() {
                let d: f32 = (0..3)
                    .map(|a| {
                        let delta = (h.lo[a] - eye[a]).max(eye[a] - h.hi[a]).max(0.);
                        delta * delta
                    })
                    .sum();
                for rank in 0..DETAIL_PLATFORMS {
                    if d < distances[rank] {
                        for k in (rank + 1..DETAIL_PLATFORMS).rev() {
                            distances[k] = distances[k - 1];
                            nearest[k] = nearest[k - 1];
                        }
                        distances[rank] = d;
                        nearest[rank] = id;
                        break;
                    }
                }
            }
        }
        let changed = self.metadata.map(|m| m as *const _) != metadata.map(|m| m as *const _)
            || self.nearest != nearest
            || self.source_count != source.cubes.len();
        self.metadata = metadata;
        if changed {
            self.items.clear();
            for id in 0..source.cubes.len() {
                let owner = metadata
                    .and_then(|m| m.owners.get(id))
                    .copied()
                    .unwrap_or(0);
                if owner == 0 || nearest.contains(&(owner as usize - 1)) {
                    self.items.push(Item::Source(id));
                }
            }
            if let Some(m) = metadata {
                for id in 0..m.hulls.len() {
                    if !nearest.contains(&id) {
                        self.items.push(Item::Hull(id));
                    }
                }
            }
            self.nearest = nearest;
            self.source_count = source.cubes.len();
        }
        self.asset.radius = source.radius;
        self.asset.cubes.clear();
        // The cached index list skips far platform cells entirely on warm frames.
        // Copy current portal animation and newly placed cubes from the live scene.
        self.asset
            .cubes
            .extend(self.items.iter().map(|item| match *item {
                Item::Source(id) => source.cubes[id],
                Item::Hull(id) => {
                    let cube = metadata.unwrap().hulls[id].cube;
                    Cube {
                        scale: cube.scale * HULL_VISIBILITY_SCALE,
                        ..cube
                    }
                }
            }));
    }
    pub fn asset<'a>(&'a self, source: &'a Asset) -> &'a Asset {
        if self.metadata.is_some() {
            &self.asset
        } else {
            source
        }
    }
    pub fn counts(&self) -> Option<(usize, usize)> {
        self.metadata
            .map(|m| (m.hulls.len(), m.hulls.len().min(DETAIL_PLATFORMS)))
    }
    pub fn source_id(&self, id: usize) -> Option<usize> {
        if self.metadata.is_none() {
            return Some(id);
        }
        match self.items[id] {
            Item::Source(id) => Some(id),
            Item::Hull(_) => None,
        }
    }
    /// Returns the stable per-world hull ID only for a replacement cube.
    pub fn hull_id(&self, id: usize) -> Option<usize> {
        match self.items.get(id) {
            Some(Item::Hull(id)) if self.metadata.is_some() => Some(*id),
            _ => None,
        }
    }
    /// Returns the authored render cube for a replacement view item. The view
    /// asset itself carries a larger, culling-only envelope for animated hulls.
    pub fn hull(&self, id: usize) -> Option<(usize, Cube)> {
        let hull = self.hull_id(id)?;
        self.metadata
            .and_then(|metadata| metadata.hulls.get(hull))
            .map(|metadata| (hull, metadata.cube))
    }
    /// Platform detail and its replacement stay solid; ordinary world cells
    /// retain the existing marker policy. Visibility and submission caps still apply.
    pub fn solid(&self, id: usize) -> bool {
        if self.metadata.is_none() {
            return false;
        }
        match self.items[id] {
            Item::Hull(_) => true,
            Item::Source(id) => self
                .metadata
                .and_then(|m| m.owners.get(id))
                .is_some_and(|&owner| owner != 0),
        }
    }
}

/// A bounded, slow local drift for visible Key5 replacement hulls. Recomputing
/// from time avoids accumulated quaternion error; callers invoke this only for
/// hulls that survived visibility and remain expanded cubes.
pub fn hull_rotation(elapsed_millis: u64, hull_id: usize) -> [f32; 4] {
    let seconds = elapsed_millis as f32 * 0.001;
    let phase = hull_id as f32 * 1.618_034;
    let angles = [
        HULL_DRIFT_RADIANS[0] * libm::sinf(seconds * HULL_DRIFT_SPEED[0] + phase),
        HULL_DRIFT_RADIANS[1] * libm::sinf(seconds * HULL_DRIFT_SPEED[1] + phase * 0.73),
        HULL_DRIFT_RADIANS[2] * libm::sinf(seconds * HULL_DRIFT_SPEED[2] + phase * 1.21),
    ];
    let axis = |a: usize| {
        let half = angles[a] * 0.5;
        let mut q = [0.; 4];
        q[a] = libm::sinf(half);
        q[3] = libm::cosf(half);
        q
    };
    let multiply = |a: [f32; 4], b: [f32; 4]| {
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    };
    let drift = multiply(axis(2), multiply(axis(1), axis(0)));
    multiply(drift, crate::orchard::WORLD_ROTATION)
}
