//! Destination-colored portal pieces, driven by the shared Rubik permutation.
use alloc::vec::Vec;
use crate::{environment, orchard::{Asset, Cube, CUSTOM_RGB555}, rubik::Puzzle, world_topology::{self, Destination, Routes}};

pub const PHASE_MS: u64 = 1_000;

struct Piece {
    index: usize,
    face: usize,
    base: Cube,
    accent: bool,
    uv: [f32; 2],
    delay: f32,
    scatter: [f32; 3],
}
#[derive(Clone, Copy)]
struct Change { from: Routes, to: Routes, started: u64 }

pub struct World {
    index: usize,
    revision: u64,
    pub scene: Asset,
    pieces: Vec<Piece>,
    current: Routes,
    latest: Routes,
    change: Option<Change>,
}

impl World {
    pub fn new(index: usize, asset: &Asset, bytes: &[u8], puzzle: &Puzzle) -> Self {
        // The page decoder already validated this record stream. Keep part IDs
        // alongside an active copy; cached floors and authored pages stay intact.
        let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let start = 16 + 4 * bytes[10] as usize;
        let pieces = bytes[start..].chunks_exact(8).enumerate().filter_map(|(id, r)| {
            if !matches!(r[5], 9 | 10) { return None; }
            let p: [f32; 3] = core::array::from_fn(|a| (r[a] as i8 as f32 + r[3] as f32 * 0.5) * unit);
            let face = if index == world_topology::VOID { 6 }
                else if p[1].abs() > p[0].abs().max(p[2].abs()) { if p[1] < 0. { 4 } else { 5 } }
                else if p[0].abs() > p[2].abs() { if p[0] > 0. { 1 } else { 3 } }
                else if p[2] < 0. { 0 } else { 2 };
            let uv = match face { 1 | 3 => [p[2] / unit, (p[1] - 2.3) / unit],
                4 | 5 => [p[0] / unit, p[2] / unit], _ => [p[0] / unit, (p[1] - 2.3) / unit] };
            let seed = id as u32;
            Some(Piece { index: id, face, base: asset.cubes[id], accent: r[5] == 10, uv,
                delay: noise(seed) * 0.55,
                scatter: core::array::from_fn(|a| (noise(seed.wrapping_add(71 * a as u32 + 1)) - 0.5) * 1.2),
            })
        }).collect();
        let current = world_topology::routes(index, puzzle);
        let mut world = Self { index, revision: puzzle.revision(),
            scene: Asset { name: asset.name, cubes: asset.cubes.clone(), radius: asset.radius },
            pieces, current, latest: current, change: None };
        world.paint(0);
        world
    }

    pub fn update(&mut self, puzzle: &Puzzle, now: u64) {
        if self.revision != puzzle.revision() {
            self.revision = puzzle.revision();
            self.latest = world_topology::routes(self.index, puzzle);
        }
        let was_changing = self.change.is_some();
        if let Some(change) = self.change && now.saturating_sub(change.started) >= PHASE_MS * 2 {
            self.current = change.to;
            self.change = None;
        }
        if self.change.is_none() && self.current != self.latest {
            self.change = Some(Change { from: self.current, to: self.latest, started: now });
        }
        // Finish the current two-second cycle before accepting the latest
        // queued destination. Repeated commits cannot restart pieces mid-pop.
        if was_changing || self.change.is_some() { self.paint(now); }
    }

    fn paint(&mut self, now: u64) {
        let authored = world_topology::authored_routes(self.index);
        for piece in &self.pieces {
            let mut cube = piece.base;
            let mut destination = self.current[piece.face];
            if let Some(change) = self.change && change.from[piece.face] != change.to[piece.face] {
                let elapsed = now.saturating_sub(change.started);
                let rebuilding = elapsed >= PHASE_MS;
                let progress = (elapsed % PHASE_MS) as f32 / PHASE_MS as f32;
                let pop = smooth((progress - piece.delay) / (1.0 - 0.55));
                let visible = if rebuilding { pop } else { 1.0 - pop };
                destination = if rebuilding { change.to[piece.face] } else { change.from[piece.face] };
                cube.scale *= visible;
                for a in 0..3 { cube.center[a] += piece.scatter[a] * (1.0 - visible); }
            }
            if destination != authored[piece.face] {
                cube.flags = tint(destination, piece.face, piece.uv, piece.accent);
            }
            self.scene.cubes[piece.index] = cube;
        }
    }
}

fn smooth(x: f32) -> f32 { let x = x.clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) }
fn noise(mut n: u32) -> f32 {
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    (n ^ (n >> 16)) as f32 / u32::MAX as f32
}
fn tint(destination: Destination, face: usize, [u, v]: [f32; 2], accent: bool) -> u32 {
    let mut colors = [0xcbd5e1; 3];
    let mut count = 1;
    let mut seed = [b'n', b'e', b's', b'w', b'b', b't', b'c'][face] as u32 * 11;
    if let Destination::World(world) = destination {
        seed += (world as u32 + 1) * 37;
        if world == world_topology::VOID {
            colors[0] = environment::VOID_COLOR;
            seed += 97;
        } else {
            count = 0;
            for (i, (_, color)) in environment::THEMES.into_iter().enumerate() {
                if world_topology::THEME_MASKS[world] & (1 << i) != 0 {
                    colors[count] = color;
                    count += 1;
                    seed += (i as u32 + 1) * 17;
                }
            }
        }
    } else { seed += 131; }
    // Same territory mixture as the authored floor/portal builder.
    let hash = ((libm::floorf(u) as i32 + 97) as u32).wrapping_mul(374761393)
        ^ ((libm::floorf(v) as i32 - 53) as u32).wrapping_mul(668265263)
        ^ seed.wrapping_mul(1442695041);
    let n = noise(hash);
    let mix = if count == 1 { 0 } else if count == 2 {
        usize::from(u + v * 0.18 + 2.8 * libm::sinf(v * 0.26 + seed as f32 * 0.7) + (n - 0.5) * 2.6 >= 0.0)
    } else {
        let angle = libm::atan2f(v, u) + core::f32::consts::PI
            + (seed as f32 * 0.73) % core::f32::consts::TAU + (n - 0.5) * 0.34;
        ((angle.rem_euclid(core::f32::consts::TAU) / (core::f32::consts::TAU / 3.0)) as usize).min(2)
    };
    let rgb = colors[mix];
    let channels = [16, 8, 0].map(|shift| {
        let c = ((rgb >> shift) & 255) as f32;
        let c = if accent { c + (255.0 - c) * if destination == Destination::Leave { 0.5 } else { 0.3 } } else { c };
        (c * 31.0 / 255.0 + 0.5) as u32
    });
    CUSTOM_RGB555 | channels[0] | (channels[1] << 5) | (channels[2] << 10)
}
