//! Render-only platform replacement. Authored geometry still drives walking,
//! picking, portals and placement. Filtering precedes per-cube visibility work.
use crate::orchard::{Asset, Cube};
use alloc::vec::Vec;

pub const ENABLED: bool = true;
pub const DETAIL_PLATFORMS: usize = 2;

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
                Item::Hull(id) => metadata.unwrap().hulls[id].cube,
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
