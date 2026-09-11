//! Incremental A* over oriented voxel faces. The caller supplies traversable
//! edges, so different agents can share the search without sharing locomotion.
//! Costs are walking distances, including time spent stepping/turning corners.
//! Supplied cardinal edges must cost at least their face-center Manhattan
//! distance. Search costs use millivoxel precision to remove contact-skin noise.
use alloc::{
    collections::{BTreeMap, BinaryHeap},
    vec::Vec,
};
use core::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Face {
    pub cell: [i32; 3],
    /// Axis * 2, plus one for the positive face.
    pub side: u8,
}
impl Face {
    pub fn normal(self) -> [f32; 3] {
        let mut n = [0.; 3];
        n[self.side as usize / 2] = if self.side % 2 == 0 { -1. } else { 1. };
        n
    }
    pub fn center(self, grid: f32) -> [f32; 3] {
        let n = self.normal();
        core::array::from_fn(|a| (self.cell[a] as f32 + 0.5 + n[a] * 0.5) * grid)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Leg {
    pub direction: [f32; 3],
    pub distance: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub to: Face,
    pub leg: Leg,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Searching,
    Ready,
    Unreachable,
    Limit,
}

pub struct Search {
    start: Face,
    goal: Face,
    grid: f32,
    open: BinaryHeap<Reverse<(u64, Reverse<u64>, Face)>>,
    best: BTreeMap<Face, (u64, Option<(Face, Leg)>)>,
    pub status: Status,
}
impl Search {
    pub fn new(start: Face, goal: Face, grid: f32) -> Self {
        let mut out = Self {
            start,
            goal,
            grid,
            open: BinaryHeap::new(),
            best: BTreeMap::new(),
            status: Status::Searching,
        };
        out.best.insert(start, (0, None));
        out.open
            .push(Reverse((out.estimate(start), Reverse(0), start)));
        out
    }
    fn estimate(&self, node: Face) -> u64 {
        let a = node.center(self.grid);
        let b = self.goal.center(self.grid);
        let d: f32 = (0..3).map(|i| (a[i] - b[i]).abs()).sum();
        // Cardinal surface edges cost at least their face-center Manhattan
        // distance, including vertical steps and rotations. Prefer progress
        // toward the goal on ties, avoiding a flood across large flat faces.
        libm::floorf(d * 1000.) as u64
    }
    pub fn advance(&mut self, work: usize, mut neighbors: impl FnMut(Face) -> Vec<Edge>) {
        if self.status != Status::Searching {
            return;
        }
        // Bound both memory and per-frame work; exhaustion never returns a
        // partial route masquerading as a reachable destination.
        const MAX_NODES: usize = 65_536;
        for _ in 0..work {
            let Some(Reverse((_, Reverse(cost), from))) = self.open.pop() else {
                self.status = Status::Unreachable;
                return;
            };
            if self.best[&from].0 != cost {
                continue;
            }
            if from == self.goal {
                self.status = Status::Ready;
                return;
            }
            for edge in neighbors(from) {
                if !edge.leg.distance.is_finite() || edge.leg.distance <= 0. {
                    continue;
                }
                let cost = cost + (libm::roundf(edge.leg.distance * 1000.) as u64).max(1);
                if self.best.get(&edge.to).is_some_and(|b| b.0 <= cost) {
                    continue;
                }
                if self.best.len() >= MAX_NODES || self.open.len() >= MAX_NODES * 4 {
                    self.status = Status::Limit;
                    return;
                }
                self.best.insert(edge.to, (cost, Some((from, edge.leg))));
                self.open.push(Reverse((
                    cost + self.estimate(edge.to),
                    Reverse(cost),
                    edge.to,
                )));
            }
        }
    }
    pub fn route(&self) -> Option<Vec<Leg>> {
        if self.status != Status::Ready {
            return None;
        }
        let mut node = self.goal;
        let mut route = Vec::new();
        while node != self.start {
            let (parent, leg) = self.best[&node].1?;
            route.push(leg);
            node = parent;
        }
        route.reverse();
        Some(route)
    }
}

/// Samples of the real contact trajectory, in renderer coordinates.
#[derive(Clone, Copy)]
pub struct Sample {
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

/// Distribute separated, full-size cube seeds across the entire polyline.
/// Longer paths increase spacing rather than losing their far end to a budget.
pub fn dashes(
    samples: &[Sample],
    scale: f32,
    flags: u32,
    budget: usize,
) -> Vec<crate::orchard::Cube> {
    let length =
        |a: Sample, b: Sample| libm::sqrtf((0..3).map(|i| (b.point[i] - a.point[i]).powi(2)).sum());
    let total: f32 = samples.windows(2).map(|s| length(s[0], s[1])).sum();
    if budget == 0 || total < 0.0001 {
        return Vec::new();
    }
    let spacing = (scale * 5.).max(total / budget as f32);
    let mut next = spacing * 0.5;
    let mut walked = 0.;
    let mut cubes = Vec::new();
    for pair in samples.windows(2) {
        let segment = length(pair[0], pair[1]);
        while next <= walked + segment && cubes.len() < budget {
            let t = ((next - walked) / segment.max(0.000001)).clamp(0., 1.);
            let normal: [f32; 3] =
                core::array::from_fn(|a| pair[0].normal[a] * (1. - t) + pair[1].normal[a] * t);
            cubes.push(crate::orchard::Cube {
                center: core::array::from_fn(|a| {
                    pair[0].point[a] * (1. - t)
                        + pair[1].point[a] * t
                        + normal[a] * (scale * 1.8 + 0.002)
                }),
                scale,
                flags,
            });
            next += spacing;
        }
        walked += segment;
    }
    cubes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(x: i32) -> Face {
        Face {
            cell: [x, 0, 0],
            side: 3,
        }
    }
    fn edge(x: i32, distance: f32) -> Edge {
        Edge {
            to: node(x),
            leg: Leg {
                direction: [1., 0., 0.],
                distance,
            },
        }
    }
    #[test]
    fn incremental_search_chooses_distance_over_fewest_edges() {
        let mut search = Search::new(node(0), node(3), 1.);
        let neighbors = |n: Face| match n.cell[0] {
            0 => alloc::vec![edge(3, 10.), edge(1, 1.)],
            1 => alloc::vec![edge(2, 1.)],
            2 => alloc::vec![edge(3, 1.)],
            _ => Vec::new(),
        };
        search.advance(1, neighbors);
        assert_eq!(search.status, Status::Searching);
        assert!(search.route().is_none());
        search.advance(10, neighbors);
        let route = search.route().unwrap();
        assert_eq!(route.len(), 3);
        assert_eq!(route.iter().map(|l| l.distance).sum::<f32>(), 3.);
    }
    #[test]
    fn disconnected_and_same_face_do_not_invent_travel() {
        let mut search = Search::new(node(0), node(1), 1.);
        search.advance(10, |_| Vec::new());
        assert_eq!(search.status, Status::Unreachable);
        assert!(search.route().is_none());
        let mut same = Search::new(node(0), node(0), 1.);
        same.advance(1, |_| panic!("same face needs no edges"));
        assert!(same.route().unwrap().is_empty());
    }
    #[test]
    fn dashed_cubes_cover_whole_long_route_and_keep_full_geometry_material() {
        let points = [0., 100., 200.].map(|x| Sample {
            point: [x, 0., 0.],
            normal: [0., 1., 0.],
        });
        let cubes = dashes(&points, 0.03, 0x7e05, 32);
        assert_eq!(cubes.len(), 32);
        assert!(cubes.last().unwrap().center[0] > 195.);
        assert!(
            cubes
                .iter()
                .all(|c| c.scale == 0.03 && c.flags == 0x7e05 && c.center[1] > c.scale)
        );
        assert!(
            cubes
                .windows(2)
                .all(|p| p[1].center[0] - p[0].center[0] > 2. * p[0].scale)
        );
    }
}
