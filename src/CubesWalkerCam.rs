//! Explore camera port from Cube/WorldShowcase.html.
//!
//! All contact math runs in authored voxel units; only the camera output is
//! scaled to renderer units. Packed cubes expand into an ideal solid union,
//! ignoring decorative gaps. Edge progress is signed walking distance, never
//! a timed animation. Keep the body/contact frame separate from the view.
extern crate alloc;
use alloc::vec::Vec;
use trueos_picasso::cam::Quaternion as Q;

type V = [f32; 3];
const UP: V = [0., 1., 0.];
const FORWARD: V = [0., 0., -1.];
const SKIN: f32 = 0.025;
// The reference uses doubles and 1e-5. Leave enough room for f32 at cell 112.
const EPS: f32 = 1e-4;
const EYE: f32 = 0.75;
const EDGE_TRAVEL: f32 = 1.8;
pub const FOV: f32 = core::f32::consts::FRAC_PI_4;
pub const NEAR: f32 = 0.01;

fn add(a: V, b: V) -> V {
    core::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: V, s: f32) -> V {
    a.map(|v| v * s)
}
fn sub(a: V, b: V) -> V {
    add(a, mul(b, -1.))
}
fn dot(a: V, b: V) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(v: V) -> V {
    let l = libm::sqrtf(dot(v, v));
    if l > 1e-10 { mul(v, 1. / l) } else { [0.; 3] }
}
fn cell(v: V) -> [i32; 3] {
    v.map(|x| libm::floorf(x) as i32)
}
fn tangent(v: V, n: V) -> V {
    let t = sub(v, mul(n, dot(v, n)));
    if dot(t, t) < 1e-10 {
        let fallback = if n[2].abs() < 0.9 { FORWARD } else { UP };
        norm(sub(fallback, mul(n, dot(fallback, n))))
    } else {
        norm(t)
    }
}
fn angle(p: f32) -> f32 {
    core::f32::consts::FRAC_PI_2 * p * p * (3. - 2. * p)
}
fn look(f: V, up: V) -> Q {
    let z = mul(norm(f), -1.);
    let x = norm(cross(up, z));
    let y = cross(z, x);
    let trace = x[0] + y[1] + z[2];
    let q = if trace > 0. {
        let s = libm::sqrtf(trace + 1.) * 2.;
        [
            (y[2] - z[1]) / s,
            (z[0] - x[2]) / s,
            (x[1] - y[0]) / s,
            s / 4.,
        ]
    } else if x[0] > y[1] && x[0] > z[2] {
        let s = libm::sqrtf(1. + x[0] - y[1] - z[2]) * 2.;
        [
            s / 4.,
            (x[1] + y[0]) / s,
            (z[0] + x[2]) / s,
            (y[2] - z[1]) / s,
        ]
    } else if y[1] > z[2] {
        let s = libm::sqrtf(1. + y[1] - x[0] - z[2]) * 2.;
        [
            (x[1] + y[0]) / s,
            s / 4.,
            (y[2] + z[1]) / s,
            (z[0] - x[2]) / s,
        ]
    } else {
        let s = libm::sqrtf(1. + z[2] - x[0] - y[1]) * 2.;
        [
            (z[0] + x[2]) / s,
            (y[2] + z[1]) / s,
            s / 4.,
            (x[1] - y[0]) / s,
        ]
    };
    Q(q).normalized()
}
fn slerp(a: Q, mut b: Q, t: f32) -> Q {
    let mut d: f32 = (0..4).map(|i| a.0[i] * b.0[i]).sum();
    if d < 0. {
        b.0 = b.0.map(|x| -x);
        d = -d;
    }
    let (u, v) = if d > 0.9995 {
        (1. - t, t)
    } else {
        let theta = libm::acosf(d.clamp(-1., 1.));
        let s = libm::sinf(theta);
        (libm::sinf((1. - t) * theta) / s, libm::sinf(t * theta) / s)
    };
    Q(core::array::from_fn(|i| a.0[i] * u + b.0[i] * v)).normalized()
}

/// Full authored occupancy, retained independently of visible seed streaming.
struct Solid {
    lo: [i32; 3],
    dims: [usize; 3],
    bits: Vec<u64>,
}
impl Solid {
    fn new(lo: [i32; 3], hi: [i32; 3]) -> Self {
        let dims = core::array::from_fn(|i| (hi[i] - lo[i]) as usize);
        Self {
            lo,
            dims,
            bits: alloc::vec![0; dims.iter().product::<usize>().div_ceil(64)],
        }
    }
    fn index(&self, p: [i32; 3]) -> Option<usize> {
        let p: [i32; 3] = core::array::from_fn(|i| p[i] - self.lo[i]);
        if (0..3).any(|i| p[i] < 0 || p[i] as usize >= self.dims[i]) {
            return None;
        }
        Some((p[0] as usize * self.dims[1] + p[1] as usize) * self.dims[2] + p[2] as usize)
    }
    fn insert(&mut self, p: [i32; 3]) {
        if let Some(i) = self.index(p) {
            self.bits[i / 64] |= 1 << (i % 64);
        }
    }
    fn has(&self, p: V) -> bool {
        self.index(cell(p))
            .is_some_and(|i| self.bits[i / 64] & (1 << (i % 64)) != 0)
    }
    fn ray(&self, origin: V, dir: V, max: f32) -> Option<Hit> {
        let dir = norm(dir);
        let mut p = cell(origin);
        let step = dir.map(|x| {
            if x > 0. {
                1
            } else if x < 0. {
                -1
            } else {
                0
            }
        });
        let delta = dir.map(|x| {
            if x == 0. {
                f32::INFINITY
            } else {
                (1. / x).abs()
            }
        });
        let mut next: V = core::array::from_fn(|i| {
            if step[i] == 0 {
                f32::INFINITY
            } else {
                (p[i] as f32 + if step[i] > 0 { 1. } else { 0. } - origin[i]) / dir[i]
            }
        });
        let mut distance = 0.;
        let mut normal = mul(dir, -1.);
        for _ in 0..2048 {
            if distance > max {
                break;
            }
            if self.has(p.map(|x| x as f32)) {
                return Some(Hit {
                    point: add(origin, mul(dir, distance)),
                    normal,
                    distance,
                });
            }
            let mut a = 0;
            if next[1] < next[a] {
                a = 1;
            }
            if next[2] < next[a] {
                a = 2;
            }
            if !next[a].is_finite() {
                break;
            }
            distance = next[a];
            next[a] += delta[a];
            p[a] += step[a];
            normal = [0.; 3];
            normal[a] = -step[a] as f32;
        }
        None
    }
    fn nearest(&self, target: V, max: f32) -> Option<Hit> {
        let mut best = None;
        let mut distance = max;
        let mut visit = |p: V| {
            if !self.has(p) {
                return;
            }
            for a in 0..3 {
                for sign in [-1., 1.] {
                    let mut n = [0.; 3];
                    n[a] = sign;
                    if self.has(add(p, n)) {
                        continue;
                    }
                    let q = core::array::from_fn(|i| {
                        if i == a {
                            p[i] + if sign > 0. { 1. } else { 0. }
                        } else {
                            target[i].clamp(p[i] + EPS, p[i] + 1. - EPS)
                        }
                    });
                    let d = libm::sqrtf(dot(sub(q, target), sub(q, target)));
                    if d < distance {
                        distance = d;
                        best = Some(Hit {
                            point: q,
                            normal: n,
                            distance: d,
                        });
                    }
                }
            }
        };
        if max.is_finite() {
            let lo = cell(sub(target, [max; 3]));
            let hi = cell(add(target, [max; 3]));
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        visit([x as f32, y as f32, z as f32]);
                    }
                }
            }
        } else {
            for (word, &bits) in self.bits.iter().enumerate() {
                let mut bits = bits;
                while bits != 0 {
                    let i = word * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let p = [
                        i / (self.dims[1] * self.dims[2]),
                        i / self.dims[2] % self.dims[1],
                        i % self.dims[2],
                    ];
                    visit(core::array::from_fn(|a| p[a] as f32 + self.lo[a] as f32));
                }
            }
        }
        best
    }
}
#[derive(Clone, Copy)]
struct Hit {
    point: V,
    normal: V,
    distance: f32,
}
#[derive(Clone, Copy)]
struct Turn {
    from: V,
    to: V,
    axis: V,
    edge_axis: usize,
    edge: V,
    convex: bool,
    progress: f32,
    cross_input: f32,
    run: [f32; 2],
}
/// A one-cell rise/drop is traversed at walking speed without turning the
/// contact frame. Signed input can pause or retrace the vertical segment.
#[derive(Clone, Copy)]
struct ElevationStep {
    from: V,
    to: V,
    across: V,
    height: f32,
    progress: f32,
}
#[derive(Default)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub roll: f32,
    pub boost: bool,
    pub fast_walk: bool,
    pub space: bool,
    pub align: bool,
}

#[derive(Clone, Copy)]
struct CubeBounds {
    lo: V,
    size: f32,
    gap: f32,
}

/// Renderer-space bounds of the packed cube selected by the Space approach probe.
pub struct SnapOutline {
    pub lo: V,
    pub hi: V,
    pub reachable: bool,
}

#[derive(Clone, Copy)]
struct PushOff {
    direction: V,
    look_at: V,
    remaining: f32,
    speed: f32,
}

#[derive(Clone, Copy)]
struct PortalOpening {
    front: V,
    inward: V,
}
fn portal_inward(face: usize) -> V {
    match face {
        0 => [0., 0., -1.],
        1 => [-1., 0., 0.],
        2 => [0., 0., 1.],
        3 => [1., 0., 0.],
        4 => [0., 1., 0.],
        5 => [0., -1., 0.],
        _ => FORWARD,
    }
}

pub struct CubesWalkerCam {
    solid: Solid,
    portals: [Option<PortalOpening>; 7],
    cubes: Vec<CubeBounds>,
    unit: f32,
    drift_half_extent: f32,
    foot: V,
    up: V,
    forward: V,
    pitch: f32,
    roll: f32,
    view: Q,
    position: V,
    rotation: Q,
    turn: Option<Turn>,
    elevation: Option<ElevationStep>,
    fly: bool,
    space_held: bool,
    push_off: Option<PushOff>,
    approach: Option<Hit>,
    align_held: bool,
    /// Reference defaults: 100% assistance and the soft 45-degree catch.
    pub camera_assist: f32,
    pub edge_perch: bool,
}
impl CubesWalkerCam {
    /// The page has already passed orchard::decode. Coordinates here match
    /// load_world: authored +Y stays up and authored Z is negated.
    /// Cycling worlds has no arrival edge, so use the north standing portal;
    /// Void uses its center portal. Look inward from inside the opening.
    pub fn from_world(bytes: &[u8], void: bool) -> Self {
        Self::from_portal(bytes, void, None)
    }
    /// Arrivals stand on the 2×2 connector one voxel in front of the opening.
    /// Without nearby support they stay in drift rather than snapping far away.
    pub fn from_portal(bytes: &[u8], void: bool, arrival: Option<usize>) -> Self {
        let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let records = &bytes[16 + 4 * bytes[10] as usize..];
        let origin = |r: &[u8]| {
            [
                r[0] as i8 as i32,
                r[1] as i8 as i32,
                -(r[2] as i8 as i32) - r[3] as i32,
            ]
        };
        let mut lo = [i32::MAX; 3];
        let mut hi = [i32::MIN; 3];
        let mut portal_lows = [[f32::INFINITY; 3]; 7];
        let mut portal_highs = [[f32::NEG_INFINITY; 3]; 7];
        for r in records.chunks_exact(8) {
            let p = origin(r);
            let size = r[3] as i32;
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a] + size);
            }
            let center = p.map(|x| x as f32 + size as f32 * 0.5);
            let face = if void {
                6
            } else if center[1].abs() > center[0].abs().max(center[2].abs()) {
                if center[1] < 0. { 4 } else { 5 }
            } else if center[0].abs() > center[2].abs() {
                if center[0] > 0. { 1 } else { 3 }
            } else if center[2] > 0. {
                0
            } else {
                2
            };
            if matches!(r[5], 9 | 10) {
                for a in 0..3 {
                    portal_lows[face][a] = portal_lows[face][a].min(p[a] as f32);
                    portal_highs[face][a] = portal_highs[face][a].max((p[a] + size) as f32);
                }
            }
        }
        let portals = core::array::from_fn(|face| {
            let lo = portal_lows[face];
            let hi = portal_highs[face];
            if !lo[0].is_finite() {
                return None;
            }
            let inward = portal_inward(face);
            let front = core::array::from_fn(|a| {
                if inward[a] > 0. {
                    hi[a]
                } else if inward[a] < 0. {
                    lo[a]
                } else {
                    (lo[a] + hi[a]) * 0.5
                }
            });
            Some(PortalOpening { front, inward })
        });
        let opening = portals[arrival.unwrap_or(if void { 6 } else { 0 })];
        let mut solid = Solid::new(lo, hi);
        let mut cubes = Vec::with_capacity(records.len() / 8);
        for r in records.chunks_exact(8) {
            let p = origin(r);
            cubes.push(CubeBounds {
                lo: p.map(|x| x as f32),
                size: r[3] as f32,
                gap: bytes[6] as f32 / 100.,
            });
            for x in 0..r[3] as i32 {
                for y in 0..r[3] as i32 {
                    for z in 0..r[3] as i32 {
                        solid.insert([p[0] + x, p[1] + y, p[2] + z]);
                    }
                }
            }
        }
        // One authored voxel in front of the opening, on its world-facing side.
        let inward = portal_inward(arrival.unwrap_or(if void { 6 } else { 0 }));
        let mut target = opening.map_or([0., hi[1] as f32 + 1., 0.], |p| add(p.front, p.inward));
        // The 2×2 connector occupies the center itself. Stand on its surface,
        // preserving distance from the portal rather than sliding far into the world.
        let support_up = tangent(UP, inward);
        for _ in 0..1024 {
            if !solid.has(target) {
                break;
            }
            target = add(target, mul(support_up, 0.25));
        }
        let surface = solid
            .ray(target, mul(UP, -1.), (hi[1] - lo[1] + 2) as f32)
            .or_else(|| solid.nearest(target, f32::INFINITY));
        // The drift envelope is a virtual world volume, not the occupied
        // bounds: clamping to a solid's bounds would push an outside eye in.
        let extent = (0..3)
            .map(|a| lo[a].abs().max(hi[a].abs()) as f32)
            .fold(0., f32::max);
        let drift_half_extent = (libm::ceilf((extent + 1.) / 32.) * 32.).max(32.);
        let mut cam = Self {
            solid,
            portals,
            cubes,
            unit,
            drift_half_extent,
            foot: target,
            up: UP,
            forward: FORWARD,
            pitch: 0.,
            roll: 0.,
            view: Q::IDENTITY,
            position: target,
            rotation: Q::IDENTITY,
            turn: None,
            elevation: None,
            fly: true,
            space_held: false,
            push_off: None,
            approach: None,
            align_held: false,
            camera_assist: 1.,
            edge_perch: true,
        };
        if opening.is_some() {
            cam.up = support_up;
            if let Some(hit) = cam.solid.ray(target, mul(support_up, -1.), 2.) {
                cam.attach(hit);
            }
            cam.position = cam.camera_target();
            let direction = if void {
                FORWARD
            } else {
                norm(mul(cam.position, -1.))
            };
            cam.forward = tangent(direction, cam.up);
            cam.pitch = libm::asinf(dot(direction, cam.up).clamp(-1., 1.));
            cam.view = look(direction, cam.up);
            cam.rotation = cam.view;
            return cam;
        }
        if let Some(hit) = surface {
            cam.attach(hit);
        }
        cam.forward = tangent(mul(cam.foot, -1.), cam.up);
        // The center portal has no inward horizontal direction.
        if void {
            cam.forward = tangent(FORWARD, cam.up);
        }
        cam.reset_view();
        cam.position = cam.camera_target();
        cam.rotation = cam.view;
        cam
    }
    /// Swept eye test for the 2×2 connector opening, not the decorative ring.
    pub fn crossed_portal(&self, previous: V) -> Option<usize> {
        let start = mul(previous, 1. / self.unit);
        let delta = sub(self.position, start);
        self.portals.iter().enumerate().find_map(|(face, portal)| {
            let p = portal.as_ref()?;
            // Only an inward-to-outward crossing enters the portal.
            let before = dot(sub(start, p.front), p.inward);
            let after = dot(sub(self.position, p.front), p.inward);
            if before <= 0.25 || after > 0.25 {
                return None;
            }
            let t = (before - 0.25) / (before - after);
            let at = sub(add(start, mul(delta, t)), p.front);
            let lateral = sub(at, mul(p.inward, dot(at, p.inward)));
            if lateral.iter().all(|x| x.abs() <= 1.75) {
                Some(face)
            } else {
                None
            }
        })
    }
    pub fn pose(&self) -> (V, Q) {
        (mul(self.position, self.unit), self.rotation)
    }
    pub fn look(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        if self.fly {
            // Mouse axes follow the banked view, rather than the old surface up.
            let bank = Q::from_axis_angle([0., 0., 1.], self.roll);
            let screen = (self.view * bank).normalized();
            let yaw = Q::from_axis_angle(screen.rotate(UP), -dx * 0.003);
            let right = (yaw * screen).rotate([1., 0., 0.]);
            let pitch = Q::from_axis_angle(right, -dy * 0.003);
            self.view = (pitch * yaw * self.view).normalized();
            self.up = self.view.rotate(UP);
            self.forward = self.view.rotate(FORWARD);
            self.pitch = 0.;
            return;
        }
        let yaw = Q::from_axis_angle(self.up, -dx * 0.003);
        self.forward = norm(yaw.rotate(self.forward));
        self.view = (yaw * self.view).normalized();
        let pitch = (self.pitch - dy * 0.003).clamp(-1.42, 1.42);
        let right = self.view.rotate([1., 0., 0.]);
        self.view = (Q::from_axis_angle(right, pitch - self.pitch) * self.view).normalized();
        self.pitch = pitch;
    }
    fn reset_view(&mut self) {
        self.roll = 0.;
        self.view = look(
            add(
                mul(self.forward, libm::cosf(self.pitch)),
                mul(self.up, libm::sinf(self.pitch)),
            ),
            self.up,
        );
    }
    fn attach(&mut self, hit: Hit) {
        self.approach = None;
        self.push_off = None;
        self.turn = None;
        self.elevation = None;
        self.up = hit.normal;
        self.foot = add(hit.point, mul(hit.normal, SKIN));
        self.forward = tangent(self.view.rotate(FORWARD), self.up);
        self.pitch = 0.;
        self.fly = false;
        self.reset_view();
    }
    fn camera_target(&self) -> V {
        if self.fly {
            return self.foot;
        }
        let hit = self
            .solid
            .ray(add(self.foot, mul(self.up, 0.03)), self.up, EYE);
        let height = hit.map_or(EYE, |h| (h.distance - 0.065).max(0.12));
        add(self.foot, mul(self.up, height))
    }
    // The rendered pose drives the probe, so the outline and Space agree with
    // the center of the screen even during camera smoothing.
    fn snap_target(&self) -> Option<Hit> {
        self.solid.ray(
            self.position,
            self.rotation.rotate(FORWARD),
            self.drift_half_extent * 4.,
        )
    }
    pub fn placement_target(&self) -> Option<(V, V)> {
        let hit = self.snap_target()?;
        let inside = sub(hit.point, mul(hit.normal, EPS));
        let c = self
            .cubes
            .iter()
            .find(|c| (0..3).all(|a| inside[a] >= c.lo[a] && inside[a] < c.lo[a] + c.size))?;
        let center = add(c.lo, [c.size * 0.5; 3]);
        Some((
            mul(add(center, mul(hit.normal, c.size * 0.5)), self.unit),
            hit.normal,
        ))
    }
    /// Add collision on the same ideal grid as authored cubes, ignoring visual gaps.
    pub fn add_placed(&mut self, pieces: &[(V, f32)]) -> bool {
        let bounds: Vec<_> = pieces
            .iter()
            .map(|&(center, scale)| {
                let size = libm::roundf(scale * 2. / self.unit).max(1.);
                let lo = sub(mul(center, 1. / self.unit), [size * 0.5; 3]).map(libm::roundf);
                CubeBounds {
                    lo,
                    size,
                    gap: (size - scale * 2. / self.unit).max(0.),
                }
            })
            .collect();
        let h = self.drift_half_extent as i32;
        for c in &bounds {
            // Keep every portal's connector and arrival area usable on revisits.
            if self.portals.iter().flatten().any(|p| {
                (0..3).all(|a| c.lo[a] < p.front[a] + 3. && c.lo[a] + c.size > p.front[a] - 3.)
            }) {
                return false;
            }
            if (0..3).any(|a| c.lo[a] < -h as f32 || c.lo[a] + c.size > h as f32) {
                return false;
            }
            if (0..3).all(|a| {
                self.position[a] >= c.lo[a] - 0.12 && self.position[a] <= c.lo[a] + c.size + 0.12
            }) {
                return false;
            }
            for x in 0..c.size as i32 {
                for y in 0..c.size as i32 {
                    for z in 0..c.size as i32 {
                        if self.solid.has(add(c.lo, [x as f32, y as f32, z as f32])) {
                            return false;
                        }
                    }
                }
            }
        }
        // Reserve the world flight envelope once when editing first needs it.
        if self.solid.lo != [-h; 3] || self.solid.dims != [(h * 2) as usize; 3] {
            self.solid = Solid::new([-h; 3], [h; 3]);
            for c in &self.cubes {
                for x in 0..c.size as i32 {
                    for y in 0..c.size as i32 {
                        for z in 0..c.size as i32 {
                            self.solid
                                .insert(cell(add(c.lo, [x as f32, y as f32, z as f32])));
                        }
                    }
                }
            }
        }
        for c in &bounds {
            for x in 0..c.size as i32 {
                for y in 0..c.size as i32 {
                    for z in 0..c.size as i32 {
                        self.solid
                            .insert(cell(add(c.lo, [x as f32, y as f32, z as f32])));
                    }
                }
            }
        }
        self.cubes.extend(bounds);
        true
    }
    pub fn snap_outline(&self) -> Option<SnapOutline> {
        let hit = self.snap_target()?;
        let inside = sub(hit.point, mul(hit.normal, EPS));
        let cube = self
            .cubes
            .iter()
            .find(|c| (0..3).all(|a| inside[a] >= c.lo[a] && inside[a] < c.lo[a] + c.size))?;
        // Follow the visible packed cube, including its tiny decorative gap.
        let inset = cube.gap * 0.5 - 0.005;
        Some(SnapOutline {
            lo: mul(add(cube.lo, [inset; 3]), self.unit),
            hi: mul(add(cube.lo, [cube.size - inset; 3]), self.unit),
            reachable: true,
        })
    }
    pub fn update(&mut self, input: Input, dt: f32) {
        let space_pressed = input.space && !self.space_held;
        if space_pressed && self.fly {
            if let Some(hit) = self.snap_target() {
                self.push_off = None;
                self.approach = Some(hit);
            }
        }
        if input.align && !self.align_held {
            self.pitch = 0.;
            self.reset_view();
        }
        self.space_held = input.space;
        self.align_held = input.align;
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.035);
        if let Some(t) = self.turn.as_mut() {
            t.cross_input = 0.;
        }
        if input.space && !self.fly {
            self.begin_push_off(input.fast_walk);
        }
        if self.push_off.is_some() || self.approach.is_some() {
            self.advance_space_flight(dt);
        } else if self.fly {
            let screen = (self.view * Q::from_axis_angle([0., 0., 1.], self.roll)).normalized();
            let movement = add(
                mul(screen.rotate(FORWARD), input.forward),
                mul(screen.rotate([1., 0., 0.]), input.right),
            );
            if dot(movement, movement) > 0. {
                let direction = norm(movement);
                let distance = 24. * if input.boost { 2.8 } else { 1. } * dt;
                let distance = self
                    .solid
                    .ray(self.foot, direction, distance + 0.12)
                    .map_or(distance, |h| (h.distance - 0.12).max(0.));
                self.foot = add(self.foot, mul(direction, distance));
                let h = self.drift_half_extent - 0.55;
                for a in 0..3 {
                    self.foot[a] = self.foot[a].clamp(-h, h);
                }
            }
        } else if input.forward != 0. || input.right != 0. {
            let total = 2.9 * if input.fast_walk { 10. } else { 5. } * dt;
            let steps = (libm::ceilf(total / 0.055) as usize).max(1);
            for _ in 0..steps {
                let direction = norm(add(
                    mul(self.forward, input.forward),
                    mul(norm(cross(self.forward, self.up)), input.right),
                ));
                self.spider_step(direction, total / steps as f32);
                if input.space && self.begin_push_off(input.fast_walk) {
                    break;
                }
            }
        }
        if let Some(t) = self.turn
            && self.edge_perch
            && t.cross_input.abs() <= 0.12
            && (t.progress - 0.5).abs() <= 0.15
        {
            let diff = 0.5 - t.progress;
            self.edge_pose(if diff.abs() < 1e-5 {
                0.5
            } else {
                t.progress + diff * (1. - libm::expf(-16. * dt))
            });
        }
        let target = self.camera_target();
        let ease = 1. - libm::expf(-20. * dt);
        self.position = add(self.position, mul(sub(target, self.position), ease));
        if self.solid.has(self.position) {
            self.position = target;
        }
        // Roll only the view: surface contact and walking direction stay stable.
        if self.fly {
            self.roll = (self.roll + input.roll * core::f32::consts::FRAC_PI_2 * dt)
                % core::f32::consts::TAU;
        } else {
            let limit = core::f32::consts::FRAC_PI_2 * 0.15;
            let target = input.roll.clamp(-1., 1.) * limit;
            self.roll += (target - self.roll) * (1. - libm::expf(-24. * dt));
            self.roll = self.roll.clamp(-limit, limit);
        }
        let rolled_view = (self.view * Q::from_axis_angle([0., 0., 1.], self.roll)).normalized();
        self.rotation = slerp(self.rotation, rolled_view, ease);
    }
    /// Only outside edges launch: ordinary steps and inside corners keep walking.
    fn begin_push_off(&mut self, fast_walk: bool) -> bool {
        let Some(t) = self.turn.filter(|t| t.convex) else {
            return false;
        };
        self.push_off = Some(PushOff {
            direction: norm(add(t.from, t.to)),
            look_at: sub(t.edge, mul(add(t.from, t.to), 0.1)),
            remaining: 18.,
            speed: 2. * 2.9 * if fast_walk { 10. } else { 5. },
        });
        self.turn = None;
        self.elevation = None;
        self.approach = None;
        self.foot = self.position;
        self.fly = true;
        true
    }
    fn aim_at(&mut self, point: V) {
        let direction = norm(sub(point, self.foot));
        if dot(direction, direction) < 0.5 {
            return;
        }
        if dot(direction, self.up).abs() > 0.99 {
            self.up = tangent(UP, direction);
        }
        self.forward = tangent(direction, self.up);
        self.pitch = libm::asinf(dot(direction, self.up).clamp(-1., 1.));
        self.view = look(direction, self.up);
    }
    fn advance_space_flight(&mut self, dt: f32) {
        if let Some(mut push) = self.push_off {
            let requested = (push.speed * dt).min(push.remaining);
            let distance = self
                .solid
                .ray(self.foot, push.direction, requested + 0.12)
                .map_or(requested, |h| (h.distance - 0.12).max(0.));
            self.foot = add(self.foot, mul(push.direction, distance));
            self.aim_at(push.look_at);
            push.remaining -= distance;
            self.push_off = if push.remaining <= EPS || distance < requested {
                None
            } else {
                Some(push)
            };
        } else if let Some(hit) = self.approach {
            let target = add(hit.point, mul(hit.normal, EYE + SKIN));
            let offset = sub(target, self.foot);
            let remaining = libm::sqrtf(dot(offset, offset));
            let direction = norm(offset);
            let requested = (48. * dt).min(remaining);
            let distance = self
                .solid
                .ray(self.foot, direction, requested + 0.12)
                .map_or(requested, |h| (h.distance - 0.12).max(0.));
            self.foot = add(self.foot, mul(direction, distance));
            self.aim_at(hit.point);
            if distance < requested {
                // Another surface blocks the approach; stop safely rather than teleport.
                self.approach = None;
            } else if remaining <= requested + EPS {
                self.attach(hit);
            }
        }
    }
    fn valid_segment(&self, t: Turn, segment: f32) -> bool {
        let mut p = t.edge;
        p[t.edge_axis] = segment + 0.5;
        p = cell(add(
            add(p, mul(t.from, -0.5)),
            mul(t.to, if t.convex { -0.5 } else { 0.5 }),
        ))
        .map(|x| x as f32);
        self.solid.has(p)
            && !self.solid.has(add(p, t.from))
            && if t.convex {
                !self.solid.has(add(p, t.to)) && !self.solid.has(add(add(p, t.from), t.to))
            } else {
                self.solid.has(sub(add(p, t.from), t.to))
            }
    }
    fn begin_turn(&mut self, normal: V, edge: V, convex: bool) -> bool {
        if self.turn.is_some() || dot(self.up, normal).abs() > 1e-6 {
            return false;
        }
        let axis = norm(cross(self.up, normal));
        let Some(edge_axis) = (0..3).find(|&a| axis[a].abs() > 0.9999) else {
            return false;
        };
        let mut t = Turn {
            from: self.up,
            to: normal,
            axis,
            edge_axis,
            edge,
            convex,
            progress: 0.,
            cross_input: 0.,
            run: [0.; 2],
        };
        let mut lo = libm::floorf(edge[edge_axis]);
        let mut hi = lo + 1.;
        let limit = self.solid.dims[edge_axis] + 2;
        for _ in 0..limit {
            if !self.valid_segment(t, lo - 1.) {
                break;
            }
            lo -= 1.;
        }
        for _ in 0..limit {
            if !self.valid_segment(t, hi) {
                break;
            }
            hi += 1.;
        }
        t.run = [lo + EPS, hi - EPS];
        self.turn = Some(t);
        self.edge_pose(0.);
        true
    }
    fn edge_pose(&mut self, progress: f32) {
        let Some(mut t) = self.turn else {
            return;
        };
        let progress = progress.clamp(0., 1.);
        let delta = angle(progress) - angle(t.progress);
        self.forward = norm(Q::from_axis_angle(t.axis, delta).rotate(self.forward));
        self.view = (Q::from_axis_angle(t.axis, delta * self.camera_assist.clamp(0.5, 1.))
            * self.view)
            .normalized();
        self.up = norm(Q::from_axis_angle(t.axis, angle(progress)).rotate(t.from));
        self.forward = tangent(self.forward, self.up);
        t.progress = progress;
        self.foot = if t.convex {
            add(
                add(
                    add(t.edge, mul(self.up, SKIN)),
                    mul(t.to, -EPS * (1. - progress)),
                ),
                mul(t.from, -EPS * progress),
            )
        } else {
            add(add(t.edge, mul(t.from, SKIN)), mul(t.to, SKIN))
        };
        self.turn = Some(t);
    }
    fn finish_turn(&mut self, next: bool) {
        let Some(t) = self.turn.take() else {
            return;
        };
        self.up = if next { t.to } else { t.from };
        self.forward = tangent(self.forward, self.up);
        self.foot = if t.convex {
            add(
                add(t.edge, mul(self.up, SKIN)),
                mul(if next { t.from } else { t.to }, -EPS),
            )
        } else {
            add(add(t.edge, mul(t.from, SKIN)), mul(t.to, SKIN))
        };
    }
    fn advance_turn(&mut self, direction: V, distance: f32) -> (f32, Q) {
        let mut t = self.turn.unwrap();
        let old = angle(t.progress);
        let across =
            Q::from_axis_angle(t.axis, old).rotate(mul(t.to, if t.convex { 1. } else { -1. }));
        let cross = dot(direction, across).clamp(-1., 1.);
        t.cross_input = cross;
        let mut used = distance;
        let mut endpoint = None;
        if cross > 1e-9 && t.progress + cross * distance / EDGE_TRAVEL >= 1. {
            used = (1. - t.progress) * EDGE_TRAVEL / cross;
            endpoint = Some(1.);
        } else if cross < -1e-9 && t.progress + cross * distance / EDGE_TRAVEL <= 0. {
            used = -t.progress * EDGE_TRAVEL / cross;
            endpoint = Some(0.);
        }
        t.edge[t.edge_axis] =
            (t.edge[t.edge_axis] + direction[t.edge_axis] * used).clamp(t.run[0], t.run[1]);
        let next = endpoint.unwrap_or(t.progress + cross * used / EDGE_TRAVEL);
        self.turn = Some(t);
        self.edge_pose(next);
        let rotation = Q::from_axis_angle(t.axis, angle(next) - old);
        if let Some(end) = endpoint {
            self.finish_turn(end == 1.);
            ((distance - used).max(0.), rotation)
        } else {
            (0., rotation)
        }
    }
    fn begin_elevation(&mut self, edge: V, side: V, height: f32) {
        let from = add(add(edge, mul(side, -EPS)), mul(self.up, SKIN));
        let to = add(add(edge, mul(side, EPS)), mul(self.up, height + SKIN));
        self.elevation = Some(ElevationStep {
            from,
            to,
            across: side,
            height,
            progress: 0.,
        });
        self.elevation_pose();
    }
    fn elevation_pose(&mut self) {
        let step = self.elevation.unwrap();
        // Rise on the near side of the riser; descend on its far side.
        // A straight diagonal chord between contacts would cut through solid.
        let base = if step.height > 0. {
            step.from
        } else {
            sub(step.to, mul(self.up, step.height))
        };
        self.foot = add(base, mul(self.up, step.height * step.progress));
    }
    fn advance_elevation(&mut self, direction: V, distance: f32) -> f32 {
        let mut step = self.elevation.unwrap();
        let cross = dot(direction, step.across).clamp(-1., 1.);
        if cross.abs() < 1e-9 {
            return 0.;
        }
        let available = if cross > 0. {
            1. - step.progress
        } else {
            step.progress
        };
        let used = distance.min(available / cross.abs());
        step.progress = (step.progress + cross * used).clamp(0., 1.);
        self.elevation = Some(step);
        self.elevation_pose();
        if used < distance
            || (cross > 0. && step.progress >= 1.)
            || (cross < 0. && step.progress <= 0.)
        {
            self.foot = if cross > 0. { step.to } else { step.from };
            self.elevation = None;
            (distance - used).max(0.)
        } else {
            0.
        }
    }
    fn spider_step(&mut self, direction: V, distance: f32) {
        if !distance.is_finite() || distance <= 0. || !direction.iter().all(|x| x.is_finite()) {
            return;
        }
        let projected = sub(direction, mul(self.up, dot(direction, self.up)));
        if dot(projected, projected) < 1e-12 {
            return;
        }
        let mut direction = norm(projected);
        let mut remaining = distance;
        for _ in 0..96 {
            if remaining <= EPS {
                return;
            }
            if self.elevation.is_some() {
                remaining = self.advance_elevation(direction, remaining);
                continue;
            }
            if self.turn.is_some() {
                let (rest, rotation) = self.advance_turn(direction, remaining);
                direction = norm(rotation.rotate(direction));
                remaining = rest;
                continue;
            }
            let Some(normal_axis) = (0..3).find(|&a| self.up[a].abs() > 0.999999) else {
                return;
            };
            let contact = sub(self.foot, mul(self.up, SKIN));
            let c = cell(sub(contact, mul(self.up, EPS))).map(|x| x as f32);
            if !self.solid.has(c) || self.solid.has(add(c, self.up)) {
                return;
            }
            let mut edge_axis = None;
            let mut edge_distance = f32::INFINITY;
            for a in 0..3 {
                if a == normal_axis || direction[a].abs() < 1e-9 {
                    continue;
                }
                let boundary = c[a] + if direction[a] > 0. { 1. } else { 0. };
                let d = ((boundary - contact[a]) / direction[a]).max(0.);
                if d < edge_distance - 1e-9 {
                    edge_distance = d;
                    edge_axis = Some(a);
                }
            }
            let Some(a) = edge_axis else {
                return;
            };
            if remaining < edge_distance {
                self.foot = add(self.foot, mul(direction, remaining));
                return;
            }
            let mut edge = add(contact, mul(direction, edge_distance));
            edge[normal_axis] = c[normal_axis] + if self.up[normal_axis] > 0. { 1. } else { 0. };
            let mut side = [0.; 3];
            side[a] = direction[a].signum();
            edge[a] = c[a] + if side[a] > 0. { 1. } else { 0. };
            for i in 0..3 {
                if i != normal_axis && i != a {
                    edge[i] = edge[i].clamp(c[i] + EPS, c[i] + 1. - EPS);
                }
            }
            remaining = (remaining - edge_distance).max(0.);
            let across = add(c, side);
            let above = add(across, self.up);
            let after = add(edge, mul(side, EPS));
            if self.solid.has(above) {
                let landing = add(after, self.up);
                let clear = self.solid.has(across)
                    && !self.solid.has(add(above, self.up))
                    && self
                        .solid
                        .ray(add(landing, mul(self.up, 0.07)), self.up, EYE)
                        .is_none();
                if clear {
                    self.begin_elevation(edge, side, 1.);
                    continue;
                }
                self.foot = add(add(edge, mul(side, -EPS)), mul(self.up, SKIN));
                if !self.begin_turn(mul(side, -1.), edge, false) {
                    return;
                }
                continue;
            }
            if self.solid.has(across) {
                self.foot = add(after, mul(self.up, SKIN));
                remaining = (remaining - EPS).max(0.);
                continue;
            }
            if self.solid.has(sub(c, self.up)) && self.solid.has(sub(across, self.up)) {
                self.begin_elevation(edge, side, -1.);
                continue;
            }
            self.foot = add(add(edge, mul(side, -EPS)), mul(self.up, SKIN));
            if !self.begin_turn(side, edge, true) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: V, b: V) {
        assert!(
            libm::sqrtf(dot(sub(a, b), sub(a, b))) < 0.002,
            "{a:?} != {b:?}"
        );
    }
    fn fixture(records: &[[i8; 4]], foot: V, forward: V) -> CubesWalkerCam {
        let mut bytes = alloc::vec![0u8;16];
        bytes[6] = 1;
        bytes[12..16].copy_from_slice(&1f32.to_le_bytes());
        for &[x, y, z, size] in records {
            // Input fixtures use renderer-oriented grid cells.
            bytes.extend_from_slice(&[x as u8, y as u8, (-z - size) as u8, size as u8, 0, 0, 0, 0]);
        }
        let mut c = CubesWalkerCam::from_world(&bytes, false);
        c.foot = foot;
        c.up = UP;
        c.forward = forward;
        c.pitch = 0.;
        c.fly = false;
        c.turn = None;
        c.reset_view();
        c.position = c.camera_target();
        c.rotation = c.view;
        c
    }
    #[test]
    fn html_reference_trace_matches_through_edges_and_reversals() {
        let mut c = fixture(&[[0, 0, 0, 4]], [3.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        for (distance, expected) in [
            0.2f32, 0.5, 0.6, -0.3, 0.8, 0.4, 0.9, 1.2, 2.1, 3.2, 0.6, -0.4,
        ]
        .into_iter()
        .zip(crate::REFERENCE_TRACES)
        {
            c.spider_step(mul(c.forward, distance.signum()), distance.abs());
            close(c.foot, expected[0..3].try_into().unwrap());
            close(c.up, expected[3..6].try_into().unwrap());
            close(c.forward, expected[6..9].try_into().unwrap());
            let q = Q(expected[9..13].try_into().unwrap());
            close(c.view.rotate(FORWARD), q.rotate(FORWARD));
            assert!((c.turn.map_or(-1., |t| t.progress) - expected[13]).abs() < 0.002);
        }
    }
    #[test]
    fn tiny_render_gaps_are_continuous_support() {
        let mut c = fixture(
            &[[0, 0, 0, 4], [4, 0, 0, 4], [8, 0, 0, 4]],
            [0.5, 4. + SKIN, 1.5],
            [1., 0., 0.],
        );
        for _ in 0..100 {
            c.spider_step(c.forward, 0.1);
            assert!(c.turn.is_none());
            close(c.up, UP);
        }
        close(c.foot, [10.5, 4. + SKIN, 1.5]);
    }
    #[test]
    fn edge_can_hold_reverse_and_perch_without_entering_solid() {
        let mut c = fixture(&[[0, 0, 0, 4]], [3.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        c.spider_step(c.forward, 0.5 + EDGE_TRAVEL * 0.45);
        let p = c.turn.unwrap().progress;
        assert!((p - 0.45).abs() < 0.001);
        c.edge_perch = false;
        c.update(Input::default(), 0.03);
        assert_eq!(c.turn.unwrap().progress, p);
        c.edge_perch = true;
        for _ in 0..50 {
            c.update(Input::default(), 0.03);
            assert!(!c.solid.has(c.position));
        }
        assert!((c.turn.unwrap().progress - 0.5).abs() < 0.0001);
        c.spider_step([0., 0., 1.], 10.);
        assert!(c.turn.unwrap().edge[2] < 4.); // Stop at the three-face vertex.
        c.spider_step(mul(c.forward, -1.), EDGE_TRAVEL * 0.5 + 0.2);
        assert!(c.turn.is_none());
        close(c.up, UP);
        assert!(c.foot[0] < 4.);
    }
    #[test]
    fn one_cell_steps_and_inner_corners_follow_the_surface() {
        let mut c = fixture(
            &[[0, 0, 0, 1], [1, 0, 0, 1], [1, 1, 0, 1]],
            [0.5, 1. + SKIN, 0.5],
            [1., 0., 0.],
        );
        c.spider_step(c.forward, 0.75);
        close(c.foot, [1. - EPS, 1.25 + SKIN, 0.5]);
        close(c.up, UP);
        c.spider_step(c.forward, 1.);
        close(c.foot, [1.25, 2. + SKIN, 0.5]);
        assert!(c.turn.is_none());
        c.spider_step(mul(c.forward, -1.), 1.5);
        close(c.foot, [0.75, 1. + SKIN, 0.5]);
        let mut c = fixture(
            &[[0, 0, 0, 1], [1, 0, 0, 1], [1, 1, 0, 1], [1, 2, 0, 1]],
            [0.5, 1. + SKIN, 0.5],
            [1., 0., 0.],
        );
        c.spider_step(c.forward, 0.5 + EDGE_TRAVEL * 0.5);
        assert!(!c.turn.unwrap().convex);
        close(c.up, [-0.70710677, 0.70710677, 0.]);
        assert!(!c.solid.has(c.camera_target()));
    }
    #[test]
    fn elevation_consumes_distance_and_can_pause_or_reverse_without_rotating() {
        let mut c = fixture(
            &[[0, 0, 0, 1], [1, 0, 0, 1], [1, 1, 0, 1]],
            [0.5, 1. + SKIN, 0.5],
            [1., 0., 0.],
        );
        let view = c.view;
        c.spider_step(c.forward, 0.5);
        for _ in 0..10 {
            let before = c.foot;
            c.spider_step(c.forward, 0.05);
            close(sub(c.foot, before), mul(UP, 0.05));
            assert!(!c.solid.has(c.foot));
            assert!(!c.solid.has(c.camera_target()));
            assert_eq!(c.view, view);
        }
        let before = c.foot;
        c.update(Input::default(), 0.035);
        assert_eq!(c.foot, before);
        c.spider_step(mul(c.forward, -1.), 0.75);
        close(c.foot, [0.75, 1. + SKIN, 0.5]);
        assert!(c.elevation.is_none());
        close(c.up, UP);
    }
    #[test]
    fn half_assist_rotates_view_half_as_far_as_contact() {
        let mut c = fixture(&[[0, 0, 0, 4]], [3.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        c.camera_assist = 0.5;
        c.spider_step(c.forward, 0.5 + EDGE_TRAVEL);
        close(c.up, [1., 0., 0.]);
        close(c.view.rotate(FORWARD), [0.70710677, -0.70710677, 0.]);
    }
    #[test]
    fn space_pushes_from_an_edge_and_returns_from_beyond_old_grip_range() {
        let mut c = fixture(&[[0, 0, 0, 4]], [3.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        c.update(
            Input {
                space: true,
                ..Input::default()
            },
            0.02,
        );
        assert!(!c.fly); // Space on a flat surface does not detach.
        for _ in 0..60 {
            c.update(
                Input {
                    space: true,
                    forward: 1.,
                    ..Input::default()
                },
                0.02,
            );
        }
        assert!(c.fly);
        assert!(c.turn.is_none());
        assert!(c.push_off.is_none());
        assert!(!c.solid.has(c.position));
        let toward = norm(sub([4., 4., 1.5], c.position));
        assert!(dot(c.rotation.rotate(FORWARD), toward) > 0.95);
        assert!(c.approach.is_none()); // Holding Space cannot immediately return.
        c.position = [1.5, 30., 1.5];
        c.foot = c.position;
        c.view = look([0., -1., 0.], [0., 0., 1.]);
        c.rotation = c.view;
        assert!(c.snap_outline().unwrap().reachable);
        c.update(Input::default(), 0.02);
        c.update(
            Input {
                space: true,
                ..Input::default()
            },
            0.02,
        );
        assert!(c.approach.is_some());
        assert!(c.fly); // Travel is animated, not an instant snap.
        for _ in 0..60 {
            c.update(Input::default(), 0.02);
            assert!(!c.solid.has(c.position));
        }
        assert!(!c.fly);
        close(c.up, UP);
        assert!(c.snap_outline().is_none());
    }
    #[test]
    fn space_can_interrupt_an_existing_outside_turn_without_cooldown() {
        for fast in [false, true] {
            let mut c = fixture(&[[0, 0, 0, 4]], [3.5, 4. + SKIN, 1.5], [1., 0., 0.]);
            c.spider_step(c.forward, 0.8);
            assert!(c.turn.is_some());
            let start = c.position;
            c.update(
                Input {
                    space: true,
                    fast_walk: fast,
                    ..Input::default()
                },
                0.02,
            );
            assert!(c.fly);
            assert!(c.push_off.is_some());
            assert!(c.turn.is_none());
            let moved = sub(c.foot, start);
            let expected = if fast { 58. } else { 29. } * 0.02;
            assert!((libm::sqrtf(dot(moved, moved)) - expected).abs() < 1e-5);
            // A fresh press can reverse immediately, even before push-off finishes.
            c.update(Input::default(), 0.02);
            c.rotation = look(sub([3.5, 4., 1.5], c.position), [0., 0., 1.]);
            c.update(
                Input {
                    space: true,
                    ..Input::default()
                },
                0.001,
            );
            assert!(c.push_off.is_none());
            assert!(c.approach.is_some() || !c.fly);
        }
    }
    #[test]
    fn view_roll_is_reversible_and_does_not_change_contact_or_heading() {
        let mut c = fixture(&[[0, 0, 0, 4]], [1.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        c.fly = true;
        let foot = c.foot;
        let heading = c.view.rotate(FORWARD);
        for _ in 0..20 {
            c.update(
                Input {
                    roll: 1.,
                    ..Input::default()
                },
                0.025,
            );
        }
        close(c.foot, foot);
        close(c.up, UP);
        close(c.rotation.rotate(FORWARD), heading);
        assert!(dot(c.rotation.rotate(UP), UP) < 0.9);
        assert!((c.roll - core::f32::consts::FRAC_PI_4).abs() < 1e-5);
        for _ in 0..20 {
            c.update(
                Input {
                    roll: -1.,
                    ..Input::default()
                },
                0.025,
            );
        }
        assert!(c.roll.abs() < 1e-5);
        c.update(
            Input {
                align: true,
                ..Input::default()
            },
            0.025,
        );
        assert_eq!(c.roll, 0.);
    }
    #[test]
    fn walking_lean_is_bounded_returns_quickly_and_does_not_steer() {
        for sign in [-1., 1.] {
            let mut c = fixture(&[[0, 0, 0, 16]], [4., 16. + SKIN, 4.], [1., 0., 0.]);
            let mut baseline = fixture(&[[0, 0, 0, 16]], [4., 16. + SKIN, 4.], [1., 0., 0.]);
            let limit = core::f32::consts::FRAC_PI_2 * 0.15;
            for _ in 0..20 {
                c.update(
                    Input {
                        roll: sign,
                        forward: 1.,
                        ..Input::default()
                    },
                    0.025,
                );
                baseline.update(
                    Input {
                        forward: 1.,
                        ..Input::default()
                    },
                    0.025,
                );
                assert!(c.roll.abs() <= limit);
                close(c.foot, baseline.foot);
                close(c.forward, baseline.forward);
                close(c.up, baseline.up);
            }
            assert!((c.roll - sign * limit).abs() < 1e-4);
            for _ in 0..12 {
                c.update(Input::default(), 0.025);
            }
            assert!(c.roll.abs() < 0.001);
            assert!(dot(c.rotation.rotate(UP), UP) > 0.9999);
        }
    }
    #[test]
    fn flight_strafe_follows_banked_screen_right_and_left() {
        for bank in [0., 0.4, -0.8, core::f32::consts::FRAC_PI_2, 3.] {
            for sign in [-1., 1.] {
                let mut c = fixture(&[[0, 0, 0, 4]], [1.5, 4. + SKIN, 1.5], [1., 0., 0.]);
                c.fly = true;
                c.foot = [1.5, 10., 1.5];
                c.position = c.foot;
                c.roll = bank;
                let screen = c.view * Q::from_axis_angle([0., 0., 1.], bank);
                let start = c.foot;
                c.update(
                    Input {
                        right: sign,
                        ..Input::default()
                    },
                    0.025,
                );
                close(
                    sub(c.foot, start),
                    mul(screen.rotate([1., 0., 0.]), sign * 24. * 0.025),
                );
            }
        }
    }
    #[test]
    fn placed_cubes_join_collision_and_cannot_overlap_existing_solids() {
        let mut c = fixture(&[[0, 0, 0, 4]], [1.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        let placed = [(mul([4.5, 0.5, 0.5], c.unit), c.unit * 0.495)];
        assert!(c.add_placed(&placed));
        assert!(c.solid.has([4.5, 0.5, 0.5]));
        assert!(!c.add_placed(&placed));
        let hit = c.solid.ray([6., 0.5, 0.5], [-1., 0., 0.], 10.).unwrap();
        close(hit.point, [5., 0.5, 0.5]);
    }
    #[test]
    fn flight_mouse_axes_follow_roll_at_every_bank_angle() {
        for bank in [0., 0.4, -0.8, core::f32::consts::FRAC_PI_2, 3.] {
            for (dx, dy) in [(10., 0.), (0., 10.)] {
                let mut c = fixture(&[[0, 0, 0, 4]], [1.5, 4. + SKIN, 1.5], [1., 0., 0.]);
                c.fly = true;
                c.roll = bank;
                let screen = c.view * Q::from_axis_angle([0., 0., 1.], bank);
                let right = screen.rotate([1., 0., 0.]);
                let up = screen.rotate(UP);
                let before = screen.rotate(FORWARD);
                c.look(dx, dy);
                let after = c.view.rotate(FORWARD);
                let change = sub(after, before);
                if dx != 0. {
                    assert!(dot(change, right) > 0.029);
                    assert!(dot(change, up).abs() < 1e-5);
                } else {
                    assert!(dot(change, up) < -0.029);
                    assert!(dot(change, right).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn drift_stops_at_solids_and_look_cannot_flip_pitch() {
        let mut c = fixture(&[[0, 0, 0, 4]], [1.5, 4. + SKIN, 1.5], [1., 0., 0.]);
        c.fly = true;
        c.position = [1.5, 6., 1.5];
        c.foot = c.position;
        c.view = look([0., -1., 0.], [0., 0., 1.]);
        c.rotation = c.view;
        for _ in 0..40 {
            c.update(
                Input {
                    forward: 1.,
                    boost: true,
                    ..Input::default()
                },
                0.035,
            );
            assert!(!c.solid.has(c.foot));
        }
        assert!(c.foot[1] >= 4.1);
        c.fly = false;
        c.look(0., -100000.);
        assert_eq!(c.pitch, 1.42);
    }
    #[test]
    fn explicit_portal_arrivals_are_centered_clear_and_face_the_origin() {
        // Authored Leave slots: pure=top, dual=bottom/top, trio=west/bottom/top.
        for (index, bytes) in crate::WORLD_PAGES.iter().take(26).enumerate() {
            let portals: &[usize] = if index < 6 {
                &[5]
            } else if index < 18 {
                &[4, 5]
            } else {
                &[3, 4, 5]
            };
            for &portal in portals {
                let c = CubesWalkerCam::from_portal(bytes, false, Some(portal));
                assert!(
                    !c.solid.has(c.position),
                    "world {} portal {}",
                    index + 1,
                    portal
                );
                close(c.position, c.camera_target());
                close(c.view.rotate(FORWARD), norm(mul(c.position, -1.)));
                if portal == 3 {
                    assert!(c.position[0] < -70.);
                } else if portal == 4 {
                    assert!(c.position[1] < -70.);
                } else {
                    assert!(c.position[1] > 70.);
                }
            }
        }
    }
    #[test]
    fn real_portal_openings_trigger_on_entry_not_on_arrival_or_ring() {
        for (world, bytes) in crate::WORLD_PAGES.iter().enumerate() {
            let mut c = CubesWalkerCam::from_world(bytes, world == 26);
            for face in 0..7 {
                let Some(p) = c.portals[face] else {
                    continue;
                };
                let start = add(p.front, p.inward);
                c.position = start;
                assert_eq!(c.crossed_portal(mul(start, c.unit)), None);
                c.position = add(p.front, mul(p.inward, 0.15));
                assert_eq!(c.crossed_portal(mul(start, c.unit)), Some(face));
                let tangent = tangent(UP, p.inward);
                let off = mul(tangent, 3.);
                c.position = add(c.position, off);
                assert_eq!(c.crossed_portal(mul(add(start, off), c.unit)), None);
                // Backward exit from a portal does not count as entering it.
                c.position = start;
                assert_eq!(c.crossed_portal(mul(p.front, c.unit)), None);
            }
        }
    }
    #[test]
    fn walking_back_into_each_real_connector_enters_its_portal() {
        for (world, bytes) in crate::WORLD_PAGES.iter().enumerate() {
            let faces: &[usize] = if world == 26 {
                &[6]
            } else {
                &[0, 1, 2, 3, 4, 5]
            };
            for &face in faces {
                let mut c = CubesWalkerCam::from_portal(bytes, world == 26, Some(face));
                let p = c.portals[face].unwrap();
                assert!((dot(sub(c.position, p.front), p.inward) - 1.).abs() < 1e-4);
                let mut entered = None;
                for _ in 0..100 {
                    let before = c.pose().0;
                    c.update(
                        Input {
                            forward: -1.,
                            ..Input::default()
                        },
                        0.016,
                    );
                    entered = c.crossed_portal(before);
                    if entered.is_some() {
                        break;
                    }
                }
                assert_eq!(entered, Some(face), "world {} face {}", world + 1, face);
            }
        }
    }
    #[test]
    fn every_real_world_starts_in_front_of_its_portal_and_can_move() {
        assert_eq!(crate::WORLD_PAGES.len(), 27);
        for (index, bytes) in crate::WORLD_PAGES.iter().enumerate() {
            let mut c = CubesWalkerCam::from_world(bytes, index == 26);
            let opening = c.portals[if index == 26 { 6 } else { 0 }].unwrap();
            assert!((dot(sub(c.position, opening.front), opening.inward) - 1.).abs() < 1e-4);
            assert!(
                !c.solid.has(c.position),
                "world {} entry inside solid {:?}",
                index + 1,
                c.position
            );
            if index < 26 {
                assert!(c.foot[2] > 0., "world {} foot {:?}", index + 1, c.foot);
                close(c.forward, tangent(mul(c.foot, -1.), c.up));
            }
            let start = c.foot;
            for _ in 0..100 {
                c.update(
                    Input {
                        forward: 1.,
                        ..Input::default()
                    },
                    0.016,
                );
                assert!(!c.solid.has(c.position));
            }
            assert!(
                dot(sub(c.foot, start), sub(c.foot, start)) > 1.,
                "world {} cannot leave portal {:?}",
                index + 1,
                c.foot
            );
        }
    }
}
