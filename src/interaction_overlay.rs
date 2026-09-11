//! Screen-space strokes for the mining and landing guides. Submit through
//! UI4 after the retained render retires, before publishing the same frame.
use alloc::vec::Vec;
use trueos::ui4_scene::{Error, Frame, SpriteCorner, SpriteQuad};

const WIDTH: f32 = 3.;
// Bound each arbitrary quad's dispatch rectangle, especially on diagonals.
const SEGMENT_PIXELS: f32 = 128.;
// One hardware sprite worklist, including both core and border strokes.
const MAX_QUADS: usize = 256;

pub fn strokes(bytes: &[u8], width: u32, height: u32, color: [u8; 4]) -> Vec<SpriteQuad> {
    let mut quads = Vec::new();
    if width == 0 || height == 0 || color[3] == 0 {
        return quads;
    }
    for line in bytes.chunks_exact(24) {
        let p: [f32; 6] = core::array::from_fn(|i| {
            f32::from_le_bytes(line[i * 4..i * 4 + 4].try_into().unwrap())
        });
        // floor clips in homogeneous space before its perspective divide.
        // Unused/rejected lines have z=2 and must never become screen strokes.
        if p.iter().any(|v| !v.is_finite())
            || !(0. ..=1.).contains(&p[2])
            || !(0. ..=1.).contains(&p[5])
        {
            continue;
        }
        let a = [
            (p[0] + 1.) * width as f32 * 0.5,
            (1. - p[1]) * height as f32 * 0.5,
        ];
        let b = [
            (p[3] + 1.) * width as f32 * 0.5,
            (1. - p[4]) * height as f32 * 0.5,
        ];
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = libm::sqrtf(d[0] * d[0] + d[1] * d[1]);
        if length < 0.01 {
            continue;
        }
        let normal = [-d[1] / length, d[0] / length];
        // Retain every line even on very large outputs without growing the
        // number of hardware worklists. Longer segments trade some bounding
        // rectangle coverage for a bounded number of GPU walkers.
        let per_line = (MAX_QUADS / (bytes.len() / 24).max(1) / 2).max(1);
        let segments = (libm::ceilf(length / SEGMENT_PIXELS) as usize).min(per_line);
        for segment in 0..segments {
            let start = segment as f32 / segments as f32;
            let end = (segment + 1) as f32 / segments as f32;
            let a = [a[0] + d[0] * start, a[1] + d[1] * start];
            let b = [a[0] + d[0] * (end - start), a[1] + d[1] * (end - start)];
            // A one-pixel dark border also makes the white grid readable on
            // yellow/light materials. The inner stroke keeps the requested alpha.
            for (stroke_width, rgba) in [(WIDTH + 2., [0, 0, 0, 128]), (WIDTH, color)] {
                let n = normal.map(|v| v * stroke_width * 0.5);
                let corner = |p: [f32; 2], sign: f32, u: f32, v: f32| SpriteCorner {
                    x: p[0] + sign * n[0],
                    y: p[1] + sign * n[1],
                    u,
                    v,
                };
                quads.push(SpriteQuad {
                    sprite_id: 0,
                    c0: corner(a, -1., 0., 0.),
                    c1: corner(b, -1., 1., 0.),
                    c2: corner(b, 1., 1., 1.),
                    c3: corner(a, 1., 0., 1.),
                    color_rgba: u32::from_le_bytes(rgba),
                    source_over: true,
                });
            }
        }
    }
    quads
}

pub fn draw(frame: &mut Frame, quads: &[SpriteQuad]) -> Result<(), Error> {
    if !quads.is_empty() {
        frame.draw_sprite_quads(quads)?;
    }
    Ok(())
}
