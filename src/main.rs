#![no_std]

extern crate alloc;
mod background;
mod camera_entry;
mod counters;
mod floor;
mod grid;
mod modes;
mod orchard;
use modes::{ModeKeys, SceneMode};
include!(concat!(env!("OUT_DIR"), "/orchard_assets.rs"));
mod picking;
mod reveal;
mod rubik;
mod transition;
mod world_bounds;
mod world_look;
use alloc::vec::Vec;

use trueos::ui4_scene::{
    CursorSource, Damage, Error as Ui4Error, Frame, ResizeEvent, output_dimensions,
};
use trueos::vgpu::{
    BUFFER_USAGE_INDEX, BUFFER_USAGE_MAP_READ, BUFFER_USAGE_MAP_WRITE, BUFFER_USAGE_VERTEX, Buffer,
    Capabilities, Device, Queue, QueueClass, RETAINED_VERTEX_LAYOUT_CUBE_PATCH_SEED,
    RetainedDrawRange, RetainedFrameSubmit, RetainedFrameSubmitV2, RetainedFrameSubmitV3,
    RetainedMesh, RetainedMeshDescriptor, RetainedTransformSeed,
};
use trueos::{
    clock,
    logl::{self, level},
};
use trueos_picasso::Picasso;
use trueos_picasso::cam::{Camera, FlyCam, Projection, Quaternion};

// Runtime input contains no imported mesh: HS generates all 44 triangles.
const CUBE_VERTEX_COUNT: u32 = 1;
const CUBE_INDEX_COUNT: u32 = 44;
const CUBE_VERTICES: &[u8] = &[0; 12];
const CUBE_INDICES: &[u8] = &[0; 44 * 4];

const CUBE_SOURCE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cube/cube.glb"));
const WIDTH: u32 = 784;
const HEIGHT: u32 = 441;
const PUZZLE_YFOV: f32 = core::f32::consts::FRAC_PI_3;
const ROOM_YFOV: f32 = 5.0 * core::f32::consts::PI / 12.0;
const IDLE_ORBIT_DELAY_MS: u64 = 3_000;
const IDLE_ORBIT_RADIANS_PER_SECOND: f32 = 0.18;
const WORLD_SEED_BUDGET: usize = grid::MAX_SEED_COUNT;
const WORLD_EYE_HEIGHT: f32 = 1.8;
const WORLD_INITIAL_LOOK_AHEAD: f32 = 3.0;

struct GridCursor {
    source: CursorSource,
    combo: u32,
    virtual_cursor: bool,
    local: [i32; 2],
}

impl SceneMode {
    const fn seed_count(self) -> usize {
        match self {
            Self::InteractiveGrid => grid::COUNT,
            Self::StaticCube => grid::CUBE_GRID_COUNT,
            Self::Sphere => grid::SPHERE_COUNT,
            Self::Orchard => 0, // Asset-specific count is selected at runtime.
            Self::World => 0,   // Nearest visible world seeds are selected at runtime.
        }
    }

    const fn number(self) -> u8 {
        match self {
            Self::InteractiveGrid => 1,
            Self::StaticCube => 2,
            Self::Sphere => 3,
            Self::Orchard => 4,
            Self::World => 5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CubeError {
    Contract,
    Ui4(&'static str, Ui4Error),
    Vgpu(&'static str, i32),
}

struct CubeScene {
    background: background::Background,
    frame: Frame,
    counters: counters::Sampler,
    device: Device,
    queue: Queue,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    flycam: FlyCam,
    seed_buffer: Buffer,
    floor_vertices: Buffer,
    floor_indices: Buffer,
    floor_revision: u32,
    cursors: Vec<GridCursor>,
    mode: SceneMode,
    orchards: orchard::Pages,
    orchard_index: usize,
    worlds: orchard::Pages,
    world_index: usize,
    visibility_scratch: orchard::VisibilityScratch,
    orchard_reveal: reveal::Reveal,
    puzzle: rubik::Puzzle,
    orbit: [f32; 3], // yaw, elevation, radius
    look_target: [f32; 3],
    last_camera_activity_millis: u64,
    finished_at: Option<u64>,
    flight: Option<transition::Flight>,
    number_keys: ModeKeys,
    demo_camera: Option<FlyCam>,
    world_yaw: f32,
    world_pitch: f32,
    pending_resize: Option<ResizeEvent>,
    previous_elapsed_millis: u64,
    first_frame: bool,
    previous_view_projection: [f32; 16],
}

fn main() {
    if let Err(error) = run() {
        logl::log(
            level::ERROR,
            format_args!("Cubes: startup/render failure={error:?}"),
        );
    }
    let _ = trueos::vshell::shutdown_current_blueprint("Cubes exited");
}

fn run() -> Result<(), CubeError> {
    // Keep the source as a reference asset; only seed/patch indices become
    // vGPU geometry. The driver owns the precompiled matching HS/DS bundle.
    let picasso = Picasso::new().map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/cube.glb", CUBE_SOURCE)
        .map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/patch/seed", CUBE_VERTICES)
        .map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/patch/indices", CUBE_INDICES)
        .map_err(|_| CubeError::Contract)?;
    let vertices = picasso
        .embedded_asset("Cube/patch/seed")
        .map_err(|_| CubeError::Contract)?
        .ok_or(CubeError::Contract)?;
    let indices = picasso
        .embedded_asset("Cube/patch/indices")
        .map_err(|_| CubeError::Contract)?
        .ok_or(CubeError::Contract)?;
    if vertices.as_slice() != CUBE_VERTICES || indices.as_slice() != CUBE_INDICES {
        return Err(CubeError::Contract);
    }
    logl::log(
        level::INFO,
        format_args!(
            "Cubes: HS/TE/DS cube source_bytes={} seed_vertices={} patches={} topology=patchlist1 control_points=3 triangles=44 imported_mesh_draw=0",
            CUBE_SOURCE.len(),
            CUBE_VERTEX_COUNT,
            CUBE_INDEX_COUNT
        ),
    );

    let opening = clock::monotonic_millis();
    logl::log(
        level::INFO,
        format_args!("Cubes: startup stage=scene-open-begin uptime_ms={opening}"),
    );
    let mut scene = CubeScene::open(&vertices, &indices)?;
    logl::log(
        level::INFO,
        format_args!(
            "Cubes: startup stage=scene-open-end uptime_ms={} elapsed_ms={} asset_loading=on-selection",
            clock::monotonic_millis(),
            clock::monotonic_millis().saturating_sub(opening)
        ),
    );
    let started = clock::monotonic_millis();
    loop {
        scene.render(clock::monotonic_millis().saturating_sub(started))?;
        trueos::vsys::sleep_ms(16);
    }
}

impl CubeScene {
    fn open(vertex_bytes: &[u8], index_bytes: &[u8]) -> Result<Self, CubeError> {
        if vertex_bytes.len() != CUBE_VERTEX_COUNT as usize * 12
            || index_bytes.len() != CUBE_INDEX_COUNT as usize * 4
        {
            return Err(CubeError::Contract);
        }
        let (x, y) = output_dimensions()
            .map(|(width, height)| {
                (
                    i32::try_from(width.saturating_sub(WIDTH) / 2).unwrap_or(0),
                    i32::try_from(height.saturating_sub(HEIGHT) / 2).unwrap_or(0),
                )
            })
            .unwrap_or((120, 96));
        let frame = Frame::open_layered(x, y, WIDTH, HEIGHT, 10)
            .map_err(|error| CubeError::Ui4("frame-open", error))?;
        let device = Device::open(Capabilities::DEFAULT.union(Capabilities::PRESENT))
            .map_err(|code| CubeError::Vgpu("device-open", code))?;
        let queue = device
            .create_queue(QueueClass::Render)
            .map_err(|code| CubeError::Vgpu("queue-create", code))?;
        let vertices = device
            .create_buffer(
                vertex_bytes.len(),
                BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX,
            )
            .map_err(|code| CubeError::Vgpu("vertex-buffer-create", code))?;
        let indices = device
            .create_buffer(
                index_bytes.len(),
                BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX,
            )
            .map_err(|code| CubeError::Vgpu("index-buffer-create", code))?;
        write_exact(device, vertices, vertex_bytes)
            .map_err(|code| CubeError::Vgpu("vertex-upload", code))?;
        write_exact(device, indices, index_bytes)
            .map_err(|code| CubeError::Vgpu("index-upload", code))?;
        let mesh = device
            .create_retained_mesh(
                vertices,
                indices,
                RetainedMeshDescriptor {
                    vertex_count: CUBE_VERTEX_COUNT,
                    index_count: CUBE_INDEX_COUNT,
                    vertex_layout: RETAINED_VERTEX_LAYOUT_CUBE_PATCH_SEED,
                    topology: trueos::vgpu::RETAINED_TOPOLOGY_CUBE_PATCHLIST_1
                        | trueos::vgpu::RETAINED_MESH_FLAG_DOUBLE_SIDED,
                    ..RetainedMeshDescriptor::default()
                },
            )
            .map_err(|code| CubeError::Vgpu("mesh-create", code))?;
        let camera = default_camera();
        let floor_vertices = device
            .create_buffer(
                floor::VERTICES * 12,
                BUFFER_USAGE_VERTEX | BUFFER_USAGE_MAP_WRITE,
            )
            .map_err(|code| CubeError::Vgpu("floor-vertices", code))?;
        let floor_indices = device
            .create_buffer(
                floor::VERTICES * 4,
                BUFFER_USAGE_INDEX | BUFFER_USAGE_MAP_WRITE,
            )
            .map_err(|code| CubeError::Vgpu("floor-indices", code))?;
        let mut floor_index_bytes = [0u8; floor::VERTICES * 4];
        for i in 0..floor::VERTICES {
            floor_index_bytes[i * 4..i * 4 + 4].copy_from_slice(&(i as u32).to_le_bytes());
        }
        write_exact(device, floor_indices, &floor_index_bytes)
            .map_err(|code| CubeError::Vgpu("floor-index-upload", code))?;
        let flycam = FlyCam::new(camera, 3.0);
        let seed_buffer = device
            .create_buffer(
                grid::MAX_SEED_COUNT * 64,
                BUFFER_USAGE_MAP_READ | BUFFER_USAGE_MAP_WRITE,
            )
            .map_err(|code| CubeError::Vgpu("grid-seed-buffer", code))?;
        logl::log(
            level::INFO,
            format_args!(
                "Cubes: mode-1-room=6x{}x{} retained_seeds={} default=2 compact-select-expand-3turns wait=0s flight=3.5s camera=WASD-orbit idle=3s-auto-orbit",
                grid::COLS,
                grid::ROWS,
                grid::COUNT,
            ),
        );
        let orchards = orchard::Pages::new(ORCHARD_ASSETS, true);
        let worlds = orchard::Pages::new(WORLD_ASSETS, false);
        let background = background::Background::start(
            frame.background().map_err(|error| CubeError::Ui4("background-target", error))?
        ).map_err(|error| CubeError::Ui4("background-start", error))?;
        Ok(Self {
            background,
            counters: counters::Sampler::new(clock::monotonic_millis()),
            orchards,
            orchard_index: 0,
            worlds,
            world_index: 0,
            visibility_scratch: orchard::VisibilityScratch::new(),
            orchard_reveal: reveal::Reveal::new(),
            frame,
            device,
            queue,
            vertices,
            indices,
            mesh,
            flycam,
            seed_buffer,
            floor_vertices,
            floor_indices,
            floor_revision: 0,
            cursors: Vec::new(),
            mode: SceneMode::StaticCube,
            puzzle: rubik::Puzzle::new(0),
            orbit: [core::f32::consts::PI, 0.0, 7.5],
            look_target: [0.0; 3],
            last_camera_activity_millis: 0,
            finished_at: None,
            flight: None,
            number_keys: ModeKeys::default(),
            demo_camera: None,
            world_yaw: 0.0,
            world_pitch: 0.0,
            pending_resize: None,
            previous_elapsed_millis: 0,
            first_frame: true,
            previous_view_projection: camera.retained(WIDTH, HEIGHT, [0.0; 16]).view_projection,
        })
    }

    fn render(&mut self, elapsed_millis: u64) -> Result<(), CubeError> {
        self.service_resize_events()?;
        let delta_seconds =
            elapsed_millis.saturating_sub(self.previous_elapsed_millis) as f32 * 0.001;
        self.previous_elapsed_millis = elapsed_millis;
        self.service_mode_hotkeys()?;
        let routes = self
            .frame
            .input_routes()
            .map_err(|error| CubeError::Ui4("grid-input-routes", error))?;
        let routed = |cursor: &GridCursor| {
            routes.iter().any(|route| {
                route.cursor == cursor.source
                    && route.combo_id == cursor.combo
                    && route.vcursor == cursor.virtual_cursor
                    && route.selected_for_window
                    && route.application_focus
            })
        };
        self.cursors.retain(&routed);
        while let Some(event) = self
            .frame
            .take_pointer_event()
            .map_err(|error| CubeError::Ui4("pointer-event", error))?
        {
            let cursor = GridCursor {
                source: event.source,
                combo: event.combo_id,
                virtual_cursor: event.vcursor,
                local: [event.local_x, event.local_y],
            };
            if !routed(&cursor) {
                continue;
            }
            if event.dx != 0 || event.dy != 0 {
                self.last_camera_activity_millis = elapsed_millis;
            }
            if self.mode == SceneMode::World {
                // Worlds are flat XZ planes. Rebuild from world yaw/pitch with
                // +Y fixed as up: FlyCam::look is intentionally camera-local
                // and can otherwise accumulate roll after a pitch.
                world_look::update(
                    &mut self.world_yaw,
                    &mut self.world_pitch,
                    event.dx as f32,
                    event.dy as f32,
                    self.flycam.look_sensitivity(),
                );
                let direction = world_look::direction(self.world_yaw, self.world_pitch);
                self.flycam.camera.rotation = look_at_camera_rotation(
                    self.flycam.camera.position,
                    core::array::from_fn(|i| self.flycam.camera.position[i] + direction[i]),
                    [0.0, 1.0, 0.0],
                );
            }
            if self.mode == SceneMode::StaticCube
                && self.puzzle.selected().is_none()
                && event.buttons_pressed & 1 != 0
            {
                let camera = self.flycam.camera.retained(
                    self.frame.width(),
                    self.frame.height(),
                    self.previous_view_projection,
                );
                if let Some((origin, direction)) = picking::ray(
                    &camera.inverse_view_projection,
                    event.local_x,
                    event.local_y,
                    self.frame.width(),
                    self.frame.height(),
                ) && let Some(id) = picking::pick(
                    origin,
                    direction,
                    grid::CUBE_COMPACT_SPACING,
                    grid::CUBE_GRID_SCALE,
                ) && self.puzzle.select(id, elapsed_millis)
                {
                    logl::log(
                        level::INFO,
                        format_args!("Cubes: selected cubie={} opening then 3 tracked turns", id),
                    );
                }
            }
            if let Some(existing) = self.cursors.iter_mut().find(|c| {
                c.source == cursor.source
                    && c.combo == cursor.combo
                    && c.virtual_cursor == cursor.virtual_cursor
            }) {
                *existing = cursor;
            } else {
                self.cursors.push(cursor);
            }
        }
        let width = self.frame.width();
        let height = self.frame.height();
        {
            if self.mode == SceneMode::StaticCube {
                self.puzzle.update(elapsed_millis);
            }
            let held = |key| {
                routes
                    .iter()
                    .filter(|r| r.selected_for_window && r.application_focus)
                    .any(|r| r.keyboard.as_ref().is_some_and(|k| k.is_down(key)))
            };
            let dt = delta_seconds.clamp(0.0, 0.1);
            let ease = 1.0 - libm::expf(-10.0 * dt);
            let wasd_held = held(0x04) || held(0x07) || held(0x16) || held(0x1a);
            if wasd_held && self.mode != SceneMode::World {
                self.last_camera_activity_millis = elapsed_millis;
            }
            let mut target = [0.0; 3];
            if self.mode == SceneMode::World {
                let local = [
                    (held(0x07) as i32 - held(0x04) as i32) as f32,
                    0.0,
                    (held(0x16) as i32 - held(0x1a) as i32) as f32,
                ];
                let mut walk = self.flycam.camera.rotation.rotate(local);
                // Keep movement on the common 2D world plane even while the
                // user looks up or down.
                walk[1] = 0.0;
                let length = libm::sqrtf(walk[0] * walk[0] + walk[2] * walk[2]);
                if length > f32::EPSILON {
                    let distance = self.flycam.speed() * dt / length;
                    let bounded = world_bounds::advance(
                        [
                            self.flycam.camera.position[0],
                            self.flycam.camera.position[2],
                        ],
                        [walk[0] * distance, walk[2] * distance],
                        world_bounds::has_horizontal_ramps(self.world_index),
                    );
                    self.flycam.camera.position[0] = bounded[0];
                    self.flycam.camera.position[2] = bounded[1];
                }
            }
            if self.mode == SceneMode::StaticCube && self.puzzle.locked() {
                let angle = self.puzzle.angle(elapsed_millis);
                let (cell, _) = self.puzzle.pose(
                    self.puzzle.selected().unwrap(),
                    libm::sinf(angle),
                    libm::cosf(angle),
                );
                let yaw = libm::atan2f(cell[0], cell[2]);
                let elevation =
                    libm::atan2f(cell[1], libm::sqrtf(cell[0] * cell[0] + cell[2] * cell[2]));
                let diff = libm::atan2f(
                    libm::sinf(yaw - self.orbit[0]),
                    libm::cosf(yaw - self.orbit[0]),
                );
                self.orbit[0] += diff * ease;
                self.orbit[1] += libm::atan2f(
                    libm::sinf(elevation - self.orbit[1]),
                    libm::cosf(elevation - self.orbit[1]),
                ) * ease;
                target = cell.map(|x| x * self.puzzle_spacing(elapsed_millis));
            } else if self.mode != SceneMode::World && self.flight.is_none() {
                // The room uses a screen-down world Y convention, so reverse
                // both orbit axes to retain conventional visual controls.
                self.orbit[0] += (held(0x07) as i32 - held(0x04) as i32) as f32 * dt;
                self.orbit[1] += (held(0x1a) as i32 - held(0x16) as i32) as f32 * dt;
                if (self.mode == SceneMode::Orchard
                    || (self.mode == SceneMode::StaticCube && self.puzzle.selected().is_none()))
                    && elapsed_millis.saturating_sub(self.last_camera_activity_millis)
                        >= IDLE_ORBIT_DELAY_MS
                {
                    self.orbit[0] += IDLE_ORBIT_RADIANS_PER_SECOND * dt;
                }
            }
            self.orbit[0] %= core::f32::consts::TAU;
            self.orbit[1] %= core::f32::consts::TAU;
            for i in 0..3 {
                self.look_target[i] += (target[i] - self.look_target[i]) * ease;
            }
            let [yaw, pitch, radius] = self.orbit;
            let radial = [
                radius * libm::cosf(pitch) * libm::sinf(yaw),
                radius * libm::sinf(pitch),
                radius * libm::cosf(pitch) * libm::cosf(yaw),
            ];
            self.flycam.camera.position =
                if matches!(self.mode, SceneMode::StaticCube | SceneMode::Orchard) {
                    radial
                } else {
                    self.flycam.camera.position
                };
            let up = orbit_up(yaw, pitch);
            if !matches!(
                self.mode,
                SceneMode::StaticCube | SceneMode::Orchard | SceneMode::World
            ) {
                self.look_target = radial.map(|v| -v);
            }
            if self.mode != SceneMode::World {
                self.flycam.camera.rotation =
                    look_at_camera_rotation(self.flycam.camera.position, self.look_target, up);
            }
            if self.mode == SceneMode::StaticCube
                && self.puzzle.selected().is_some()
                && !self.puzzle.locked()
            {
                let finished = *self.finished_at.get_or_insert(elapsed_millis);
                if elapsed_millis.saturating_sub(finished) >= transition::WAIT_MS
                    && self.flight.is_none()
                {
                    let (cell, _) = self.puzzle.pose(self.puzzle.selected().unwrap(), 0.0, 1.0);
                    let end = cell.map(|x| x * grid::CUBE_GRID_SPACING);
                    let start = self.flycam.camera.position;
                    let right = [-libm::cosf(yaw), 0.0, libm::sinf(yaw)];
                    self.flight = Some(transition::Flight {
                        started: elapsed_millis,
                        points: [
                            start,
                            core::array::from_fn(|i| start[i] + right[i] * 1.5 + up[i] * 0.5),
                            core::array::from_fn(|i| end[i] + cell[i] * 0.6),
                            end,
                        ],
                    });
                }
                if let Some(flight) = &self.flight {
                    let pos = flight.position(elapsed_millis);
                    let end = flight.points[3];
                    let dir: [f32; 3] = core::array::from_fn(|i| end[i] - pos[i]);
                    if dir.iter().map(|x| x * x).sum::<f32>() > 1e-8 {
                        self.orbit[0] = libm::atan2f(-dir[0], -dir[2]);
                        self.orbit[1] =
                            libm::atan2f(-dir[1], libm::sqrtf(dir[0] * dir[0] + dir[2] * dir[2]));
                    }
                    let [y, p, r] = self.orbit;
                    let direction = [
                        -libm::cosf(p) * libm::sinf(y),
                        -libm::sinf(p),
                        -libm::cosf(p) * libm::cosf(y),
                    ];
                    self.flycam.camera.position = pos;
                    self.flycam.camera.rotation = look_at_camera_rotation(
                        pos,
                        core::array::from_fn(|i| pos[i] + direction[i]),
                        orbit_up(y, p),
                    );
                    if flight.done(elapsed_millis) {
                        self.mode = SceneMode::InteractiveGrid;
                        self.set_mode_projection(SceneMode::InteractiveGrid);
                        self.flycam.camera.position = [0.0; 3];
                        self.look_target = direction.map(|x| x * r);
                        self.flight = None;
                        logl::log(
                            level::INFO,
                            format_args!(
                                "Cubes: flight complete -> room=6x100 seeds camera=center WASD=look"
                            ),
                        );
                    }
                }
            }
        }
        let camera = self
            .flycam
            .camera
            .retained(width, height, self.previous_view_projection);
        let tan_half_fov = match self.flycam.camera.projection {
            Projection::Perspective { yfov, .. } => libm::tanf(yfov * 0.5),
            _ => libm::tanf(PUZZLE_YFOV * 0.5),
        };
        self.background.update(
            self.mode == SceneMode::World, WORLD_ASSETS[self.world_index].0,
            self.world_yaw, self.world_pitch, tan_half_fov, (width, height),
        ).map_err(|error| CubeError::Ui4("background-update", error))?;
        match self.frame.begin_gpu_frame() {
            Ok(()) => {}
            Err(Ui4Error::Busy) => return Ok(()),
            Err(error) => return Err(CubeError::Ui4("frame-begin", error)),
        }
        let surface = self
            .device
            .acquire_ui4_surface(self.frame.window_id())
            .map_err(|code| CubeError::Vgpu("surface-acquire", code))?;
        let puzzle_spacing = self.puzzle_spacing(elapsed_millis);
        let (visible, visibility_stats) = match self.mode {
            SceneMode::Orchard => {
                let asset = &self.orchards[self.orchard_index];
                self.orchard_reveal
                    .begin_frame(elapsed_millis, asset.cubes.len());
                let (ids, stats) = orchard::visible_when(
                    &mut self.visibility_scratch,
                    asset,
                    self.flycam.camera.position,
                    &camera.view_projection,
                    |id| self.orchard_reveal.admit(id),
                );
                self.orchard_reveal.end_frame();
                (ids, Some(stats))
            }
            SceneMode::World => {
                let asset = &self.worlds[self.world_index];
                let (ids, stats) = orchard::visible_when_limited(
                    &mut self.visibility_scratch,
                    asset,
                    self.flycam.camera.position,
                    &camera.view_projection,
                    WORLD_SEED_BUDGET,
                    |_| true,
                );
                (ids, Some(stats))
            }
            _ => (&[][..], None),
        };
        let opaque_count = if matches!(self.mode, SceneMode::Orchard | SceneMode::World) {
            visible.len().max(1)
        } else {
            self.mode.seed_count()
        };
        // Visibility removes submissions, not authored scene instances. The
        // fallback seed is an ABI placeholder and is never a countable cube.
        let countable_seed_count = visibility_stats.map_or(opaque_count, |stats| stats.source);
        let seed_count = opaque_count
            + if self.mode == SceneMode::StaticCube {
                54
            } else if self.mode == SceneMode::InteractiveGrid {
                1
            } else {
                0
            };
        let turn_angle = self.puzzle.angle(elapsed_millis);
        let (turn_sin, turn_cos) = (libm::sinf(turn_angle), libm::cosf(turn_angle));
        let mut seed_bytes = [0u8; grid::MAX_SEED_COUNT * 64];
        let mut opaque_seeds = [RetainedTransformSeed::default(); 27];
        let mut expanded_count = 0usize;
        for i in 0..opaque_count {
            let (cell, basis) = self.puzzle.pose(i.min(26), turn_sin, turn_cos);
            let (translation, scale) = match self.mode {
                SceneMode::Orchard => {
                    if let Some(&id) = visible.get(i) {
                        let cube = self.orchards[self.orchard_index].cubes[id];
                        (cube.center, cube.scale)
                    } else {
                        // Empty view: a required nonempty retained group, behind the eye.
                        (self.flycam.camera.position.map(|v| v * 2.0), 0.0001)
                    }
                }
                SceneMode::World => {
                    if let Some(&id) = visible.get(i) {
                        let cube = self.worlds[self.world_index].cubes[id];
                        (cube.center, cube.scale)
                    } else {
                        (self.flycam.camera.position.map(|v| v * 2.0), 0.0001)
                    }
                }
                SceneMode::InteractiveGrid => {
                    let translation = grid::position(i);
                    let depth = -(camera.view[2] * translation[0]
                        + camera.view[6] * translation[1]
                        + camera.view[10] * translation[2]
                        + camera.view[14]);
                    let cursor_radius_px = grid::cursor_radius_px(
                        grid::CUBE_SCALE,
                        depth,
                        camera.projection[5],
                        height,
                    );
                    let active = grid::project(&camera.view_projection, translation, width, height)
                        .is_some_and(|point| {
                            self.cursors.iter().any(|c| {
                                grid::near(point, c.local, width, height, cursor_radius_px)
                            })
                        });
                    (
                        translation,
                        if active {
                            grid::CUBE_SCALE
                        } else {
                            grid::marker_scale(depth, camera.projection[5], height)
                        },
                    )
                }
                SceneMode::StaticCube => (cell.map(|x| x * puzzle_spacing), grid::CUBE_GRID_SCALE),
                SceneMode::Sphere => {
                    let translation = grid::sphere_position(i);
                    let depth = -(camera.view[2] * translation[0]
                        + camera.view[6] * translation[1]
                        + camera.view[10] * translation[2]
                        + camera.view[14]);
                    let active = grid::project(&camera.view_projection, translation, width, height)
                        .is_some_and(|point| {
                            self.cursors.iter().any(|c| {
                                grid::near(
                                    point,
                                    c.local,
                                    width,
                                    height,
                                    grid::sphere_cursor_radius_px(width, height),
                                )
                            })
                        });
                    (
                        translation,
                        if active {
                            grid::CUBE_SCALE
                        } else {
                            grid::marker_scale(depth, camera.projection[5], height)
                        },
                    )
                }
            };
            let seed = RetainedTransformSeed {
                translation,
                scale: [scale; 3],
                rotation: if self.mode == SceneMode::StaticCube {
                    quaternion_from_rotation_columns(basis[0], basis[1], basis[2]).0
                } else if self.mode == SceneMode::World {
                    orchard::WORLD_ROTATION
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                },
                local_radius: grid::CUBE_LOCAL_RADIUS,
                previous_translation: translation,
                draw_group: 0,
                flags: ((i as u32) << 16)
                    | if self.mode == SceneMode::StaticCube {
                        rubik::PALETTE_FLAG | i as u32
                    } else if self.mode == SceneMode::InteractiveGrid {
                        rubik::ROOM_PALETTE_FLAG
                    } else if self.mode == SceneMode::Orchard {
                        visible.get(i).map_or(orchard::CUSTOM_RGB555, |&id| {
                            self.orchards[self.orchard_index].cubes[id].flags
                        })
                    } else if self.mode == SceneMode::World {
                        visible.get(i).map_or(orchard::CUSTOM_RGB555, |&id| {
                            self.worlds[self.world_index].cubes[id].flags
                        })
                    } else {
                        rubik::SPHERE_GRADIENT_FLAG
                    },
            };
            if scale >= 0.001 {
                expanded_count += 1;
            }
            if i < 27 {
                opaque_seeds[i] = seed;
            }
            encode_seed(seed, &mut seed_bytes[i * 64..(i + 1) * 64]);
        }
        if self.mode == SceneMode::StaticCube {
            let mut faces = Vec::with_capacity(54);
            for id in 0..27 {
                let (_, basis) = self.puzzle.pose(id, turn_sin, turn_cos);
                let cell = [id % 3, (id / 3) % 3, id / 9];
                for axis in 0..3 {
                    if cell[axis] != 1 {
                        let sign = if cell[axis] == 2 { 1.0 } else { -1.0 };
                        let face = (axis * 2 + usize::from(sign < 0.0)) as u32;
                        let p: [f32; 3] = core::array::from_fn(|i| {
                            opaque_seeds[id].translation[i]
                                + basis[axis][i] * sign * grid::CUBE_GRID_SCALE
                        });
                        let depth = -(camera.view[2] * p[0]
                            + camera.view[6] * p[1]
                            + camera.view[10] * p[2]
                            + camera.view[14]);
                        faces.push((depth, id, face));
                    }
                }
            }
            faces.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (slot, (_, id, face)) in faces.iter().enumerate() {
                let mut seed = opaque_seeds[*id];
                seed.draw_group = 1;
                seed.flags =
                    ((slot as u32) << 16) | rubik::PALETTE_FLAG | 512 | (*face << 10) | *id as u32;
                let row = opaque_count + slot;
                encode_seed(seed, &mut seed_bytes[row * 64..(row + 1) * 64]);
            }
        } else if self.mode == SceneMode::InteractiveGrid {
            // Keep both draw groups stable across the scene transition. This
            // origin seed has clip.w=0 and HS emits no primitives.
            let dummy = RetainedTransformSeed {
                scale: [0.0001; 3],
                rotation: [0., 0., 0., 1.],
                draw_group: 1,
                flags: 512,
                ..RetainedTransformSeed::default()
            };
            encode_seed(dummy, &mut seed_bytes[opaque_count * 64..seed_count * 64]);
        }
        write_exact(
            self.device,
            self.seed_buffer,
            &seed_bytes[..seed_count * 64],
        )
        .map_err(|code| CubeError::Vgpu("grid-seed-upload", code))?;
        let floor_bytes =
            floor::vertices(&camera.view_projection, self.mode == SceneMode::StaticCube);
        write_exact(self.device, self.floor_vertices, &floor_bytes)
            .map_err(|code| CubeError::Vgpu("floor-upload", code))?;
        self.floor_revision = self.floor_revision.wrapping_add(1);
        let submit_started = if self.first_frame {
            let now = clock::monotonic_millis();
            logl::log(
                level::INFO,
                format_args!("Cubes: startup stage=first-submit-begin uptime_ms={now}"),
            );
            now
        } else {
            0
        };
        let point = self
            .device
            .submit_retained_frame_v3(
                self.queue,
                surface,
                self.mesh,
                self.vertices,
                self.indices,
                RetainedFrameSubmitV3 {
                    frame: RetainedFrameSubmitV2 {
                        frame: RetainedFrameSubmit {
                            camera,
                            static_vertex_buffer: self.floor_vertices.raw(),
                            static_index_buffer: self.floor_indices.raw(),
                            static_vertex_revision: self.floor_revision,
                            static_draw_count: 1,
                            static_draws: [
                                trueos::vgpu::IndexedBatchDrawV2 {
                                    index_count: floor::VERTICES as u32,
                                    topology: trueos::vgpu::PRIMITIVE_TOPOLOGY_LINE_LIST,
                                    rgba8_srgb: u32::from_le_bytes([100, 100, 100, 255]),
                                    ..trueos::vgpu::IndexedBatchDrawV2::default()
                                },
                                trueos::vgpu::IndexedBatchDrawV2::default(),
                                trueos::vgpu::IndexedBatchDrawV2::default(),
                            ],
                            clear_rgba8_srgb: 0, // Transparent premultiplied foreground reveals the independent background.
                            ..RetainedFrameSubmit::default()
                        },
                        ..RetainedFrameSubmitV2::default()
                    },
                    seed_buffer: self.seed_buffer.raw(),
                    seed_count: seed_count as u32,
                    draw_count: if matches!(
                        self.mode,
                        SceneMode::Sphere | SceneMode::Orchard | SceneMode::World
                    ) {
                        1
                    } else {
                        2
                    },
                    draws: [
                        RetainedDrawRange {
                            first_index: 0,
                            index_count: 44,
                        },
                        if matches!(
                            self.mode,
                            SceneMode::Sphere | SceneMode::Orchard | SceneMode::World
                        ) {
                            RetainedDrawRange::default()
                        } else {
                            RetainedDrawRange {
                                first_index: 0,
                                index_count: 44,
                            }
                        },
                        RetainedDrawRange::default(),
                        RetainedDrawRange::default(),
                    ],
                    ..RetainedFrameSubmitV3::default()
                },
            )
            .map_err(|code| CubeError::Vgpu("frame-submit", code))?;
        let wait_started = if self.first_frame {
            let now = clock::monotonic_millis();
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: startup stage=first-submit-end uptime_ms={now} elapsed_ms={}",
                    now.saturating_sub(submit_started)
                ),
            );
            now
        } else {
            0
        };
        self.device
            .wait(self.queue, point.value)
            .map_err(|code| CubeError::Vgpu("timeline-wait", code))?;
        if self.first_frame {
            let now = clock::monotonic_millis();
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: startup stage=first-wait-end uptime_ms={now} elapsed_ms={}",
                    now.saturating_sub(wait_started)
                ),
            );
        }
        self.frame
            .publish(Damage::full(width, height))
            .map_err(|error| CubeError::Ui4("frame-publish", error))?;
        if self.first_frame {
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: startup stage=first-publish-end uptime_ms={}",
                    clock::monotonic_millis()
                ),
            );
            self.first_frame = false;
        }
        if let Some(report) = self.counters.record(
            clock::monotonic_millis(),
            self.mode.number(),
            expanded_count,
            countable_seed_count.saturating_sub(expanded_count),
            visibility_stats.map(|stats| counters::Visibility {
                frustum: stats.frustum,
                occluded: stats.occluded,
                pending: stats.pending,
                patches_per_cube: CUBE_INDEX_COUNT as usize,
            }),
        ) {
            logl::log(level::INFO, format_args!("Cubes: {report}"));
        }
        self.previous_view_projection = camera.view_projection;
        Ok(())
    }

    fn service_mode_hotkeys(&mut self) -> Result<(), CubeError> {
        let state = self
            .frame
            .keyboard_state()
            .map_err(|error| CubeError::Ui4("mode-hotkeys", error))?;
        let current = state.map_or(0, |keyboard| {
            (keyboard.is_down(0x1e) as u8)
                | ((keyboard.is_down(0x1f) as u8) << 1)
                | ((keyboard.is_down(0x20) as u8) << 2)
                | ((keyboard.is_down(0x21) as u8) << 3)
                | ((keyboard.is_down(0x22) as u8) << 4)
        });
        if let Some(selection) = self.number_keys.update(
            current,
            self.mode,
            self.orchard_index,
            self.orchards.len(),
            self.worlds.len(),
        ) {
            let mode = selection.mode;
            match mode {
                SceneMode::Orchard => self.orchard_index = selection.page.unwrap(),
                SceneMode::World => self.world_index = selection.page.unwrap(),
                _ => {}
            }
            if matches!(mode, SceneMode::Orchard | SceneMode::World) {
                let started = clock::monotonic_millis();
                let (pages, index) = if mode == SceneMode::Orchard {
                    (&mut self.orchards, self.orchard_index)
                } else {
                    (&mut self.worlds, self.world_index)
                };
                let loaded = if mode == SceneMode::World {
                    pages.load_world(index)
                } else {
                    pages.load(index)
                }
                .map_err(|error| {
                    logl::log(
                        level::ERROR,
                        format_args!(
                            "Cubes: asset mode={} page={} rejected={error}",
                            mode.number(),
                            index + 1
                        ),
                    );
                    CubeError::Contract
                })?;
                if loaded {
                    logl::log(
                        level::INFO,
                        format_args!(
                            "Cubes: asset mode={} page={} cubes={} decode_ms={}",
                            mode.number(),
                            index + 1,
                            pages[index].cubes.len(),
                            clock::monotonic_millis().saturating_sub(started)
                        ),
                    );
                }
            }
            if mode == SceneMode::World && self.mode != SceneMode::World {
                self.demo_camera = Some(self.flycam);
            } else if mode != SceneMode::World && self.mode == SceneMode::World {
                if let Some(camera) = self.demo_camera.take() {
                    self.flycam = camera;
                }
            }
            self.mode = mode;
            self.frame
                .set_center_snapped_mouse(mode == SceneMode::World)
                .map_err(|error| CubeError::Ui4("center-snapped-mouse", error))?;
            if mode != SceneMode::World {
                self.puzzle = rubik::Puzzle::new(self.previous_elapsed_millis);
            }
            self.last_camera_activity_millis = self.previous_elapsed_millis;
            self.finished_at = None;
            self.flight = None;
            if mode == SceneMode::StaticCube {
                let p = camera_entry::puzzle_position(self.flycam.camera.position);
                self.flycam.camera.position = p;
                let radius = libm::sqrtf(p.iter().map(|x| x * x).sum::<f32>()).max(7.5);
                self.orbit = [
                    libm::atan2f(p[0], p[2]),
                    libm::atan2f(p[1], libm::sqrtf(p[0] * p[0] + p[2] * p[2])),
                    radius,
                ];
                self.look_target = [0.0; 3];
                self.flycam.camera.rotation =
                    look_at_camera_rotation(p, [0.0; 3], [0.0, -1.0, 0.0]);
            }
            if mode == SceneMode::Orchard {
                self.orchard_reveal.reset();
                let asset = &self.orchards[self.orchard_index];
                self.orbit = [core::f32::consts::PI, -0.15, (asset.radius * 2.5).max(1.0)];
                self.look_target = [0.; 3];
                logl::log(
                    level::INFO,
                    format_args!(
                        "Cubes: Key4 asset={} cubes={} assets={} visibility=collective-cpu-before-HS reveal=pop delay={}ms rate={}/s burst={} rearm={}ms",
                        asset.name,
                        asset.cubes.len(),
                        self.orchards.len(),
                        reveal::DELAY_MS,
                        reveal::STARTS_PER_SECOND,
                        reveal::MAX_STARTS_PER_FRAME,
                        reveal::REARM_MS
                    ),
                );
            } else if mode == SceneMode::World {
                self.flycam = FlyCam::new(default_camera(), 3.0);
                self.world_yaw = 0.0;
                self.world_pitch = 0.0;
                self.flycam.camera.position =
                    orchard::world_from_demo([0.0, -WORLD_EYE_HEIGHT, 0.0]);
                self.flycam.camera.rotation = look_at_camera_rotation(
                    self.flycam.camera.position,
                    // Aim straight along the XZ world plane.  Targeting the
                    // ground at Y=0 here gave the fresh first-person camera
                    // an unintended atan(1.8 / 3.0) ~= 31 degree down-pitch.
                    orchard::world_from_demo([0.0, -WORLD_EYE_HEIGHT, -WORLD_INITIAL_LOOK_AHEAD]),
                    orchard::world_from_demo([0.0, -1.0, 0.0]),
                );
                let asset = &self.worlds[self.world_index];
                logl::log(
                    level::INFO,
                    format_args!(
                        "Cubes: Key5 world={}/{} asset={} authored={} first_person=plane_walk streamed_nearest={} renderer_seed_limit={}",
                        self.world_index + 1,
                        self.worlds.len(),
                        asset.name,
                        asset.cubes.len(),
                        WORLD_SEED_BUDGET,
                        grid::MAX_SEED_COUNT
                    ),
                );
            } else if mode != SceneMode::StaticCube {
                self.flycam.camera.position = [0.0; 3];
            }
            self.set_mode_projection(mode);
            self.previous_view_projection = self
                .flycam
                .camera
                .retained(self.frame.width(), self.frame.height(), [0.0; 16])
                .view_projection;
            self.cursors.clear();
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: mode={} seed_count={}",
                    match mode {
                        SceneMode::InteractiveGrid => "1 interactive-grid",
                        SceneMode::StaticCube =>
                            "2 compact-puzzle click=edge/corner turns=3x1s camera=WASD-orbit idle=3s-auto-orbit",
                        SceneMode::Sphere =>
                            "3 sphere=1024 camera=center WASD=look cursor-expand=10%-area",
                        SceneMode::Orchard =>
                            "4 cubes-pair WASD=orbit idle=auto-orbit Key4=next-pair",
                        SceneMode::World =>
                            "5 lvl27-world first-person mouse-look WASD=plane-walk nearest-first-stream Key5=next-world",
                    },
                    if mode == SceneMode::Orchard {
                        self.orchards[self.orchard_index].cubes.len()
                    } else if mode == SceneMode::World {
                        self.worlds[self.world_index].cubes.len()
                    } else {
                        mode.seed_count()
                    },
                ),
            );
        }
        Ok(())
    }

    fn puzzle_spacing(&self, now: u64) -> f32 {
        let compact = grid::CUBE_COMPACT_SPACING;
        compact + (grid::CUBE_GRID_SPACING - compact) * self.puzzle.expansion(now)
    }

    fn set_mode_projection(&mut self, mode: SceneMode) {
        self.flycam.camera.projection = Projection::Perspective {
            yfov: match mode {
                SceneMode::InteractiveGrid => ROOM_YFOV,
                SceneMode::StaticCube => PUZZLE_YFOV,
                SceneMode::Sphere => ROOM_YFOV,
                SceneMode::Orchard => PUZZLE_YFOV,
                SceneMode::World => PUZZLE_YFOV,
            },
            znear: 0.1,
            zfar: Some(if mode == SceneMode::Orchard {
                (self.orchards[self.orchard_index].radius * 10.0).max(100.0)
            } else {
                100.0
            }),
            aspect_ratio: None,
        };
    }

    /// UI4 delivers maximization and restoration as resize requests. Keep a
    /// request until the current GPU lease has retired, since replacing a
    /// streaming frame may temporarily be busy immediately after publish.
    fn service_resize_events(&mut self) -> Result<(), CubeError> {
        while let Some(event) = self
            .frame
            .take_resize_event()
            .map_err(|error| CubeError::Ui4("resize-event-take", error))?
        {
            self.pending_resize = Some(event);
        }

        let Some(event) = self.pending_resize else {
            return Ok(());
        };
        if (event.width, event.height) == (self.frame.width(), self.frame.height()) {
            self.pending_resize = None;
            return Ok(());
        }

        match self.frame.resize(event.width, event.height) {
            Ok(()) => {
                self.pending_resize = None;
                logl::log(
                    level::INFO,
                    format_args!(
                        "Cubes: UI4 frame resized {}x{} -> {}x{}",
                        event.old_width, event.old_height, event.width, event.height,
                    ),
                );
            }
            Err(Ui4Error::Busy) => {}
            Err(error) => return Err(CubeError::Ui4("frame-resize", error)),
        }
        Ok(())
    }
}

fn write_exact(device: Device, buffer: Buffer, bytes: &[u8]) -> Result<(), i32> {
    (device.write_buffer(buffer, 0, bytes)? == bytes.len())
        .then_some(())
        .ok_or(trueos::vgpu::ERR_IO)
}

fn encode_seed(seed: RetainedTransformSeed, bytes: &mut [u8]) {
    let values = seed
        .translation
        .into_iter()
        .chain(seed.scale)
        .chain(seed.rotation)
        .chain([seed.local_radius])
        .chain(seed.previous_translation);
    for (i, value) in values.enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[56..60].copy_from_slice(&seed.draw_group.to_le_bytes());
    bytes[60..64].copy_from_slice(&seed.flags.to_le_bytes());
}

fn orbit_up(yaw: f32, pitch: f32) -> [f32; 3] {
    [
        libm::sinf(yaw) * libm::sinf(pitch),
        -libm::cosf(pitch),
        libm::cosf(yaw) * libm::sinf(pitch),
    ]
}

fn default_camera() -> Camera {
    let position = [0.0, 0.0, -7.5];
    Camera {
        position,
        rotation: look_at_camera_rotation(position, [0.0; 3], [0.0, -1.0, 0.0]),
        projection: Projection::Perspective {
            yfov: PUZZLE_YFOV,
            znear: 0.1,
            zfar: Some(100.0),
            aspect_ratio: None,
        },
    }
}

fn look_at_camera_rotation(position: [f32; 3], target: [f32; 3], world_up: [f32; 3]) -> Quaternion {
    let forward = [
        target[0] - position[0],
        target[1] - position[1],
        target[2] - position[2],
    ];
    let length =
        libm::sqrtf(forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]);
    let forward = [
        forward[0] / length,
        forward[1] / length,
        forward[2] / length,
    ];
    let right = [
        forward[1] * world_up[2] - forward[2] * world_up[1],
        forward[2] * world_up[0] - forward[0] * world_up[2],
        forward[0] * world_up[1] - forward[1] * world_up[0],
    ];
    let length = libm::sqrtf(right[0] * right[0] + right[1] * right[1] + right[2] * right[2]);
    let right = [right[0] / length, right[1] / length, right[2] / length];
    let up = [
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    ];
    quaternion_from_rotation_columns(right, up, [-forward[0], -forward[1], -forward[2]])
}

fn quaternion_from_rotation_columns(x: [f32; 3], y: [f32; 3], z: [f32; 3]) -> Quaternion {
    let trace = x[0] + y[1] + z[2];
    let rotation = if trace > 0.0 {
        let s = libm::sqrtf(trace + 1.0) * 2.0;
        [
            (y[2] - z[1]) / s,
            (z[0] - x[2]) / s,
            (x[1] - y[0]) / s,
            0.25 * s,
        ]
    } else if x[0] > y[1] && x[0] > z[2] {
        let s = libm::sqrtf(1.0 + x[0] - y[1] - z[2]) * 2.0;
        [
            0.25 * s,
            (x[1] + y[0]) / s,
            (z[0] + x[2]) / s,
            (y[2] - z[1]) / s,
        ]
    } else if y[1] > z[2] {
        let s = libm::sqrtf(1.0 + y[1] - x[0] - z[2]) * 2.0;
        [
            (x[1] + y[0]) / s,
            0.25 * s,
            (y[2] + z[1]) / s,
            (z[0] - x[2]) / s,
        ]
    } else {
        let s = libm::sqrtf(1.0 + z[2] - x[0] - y[1]) * 2.0;
        [
            (z[0] + x[2]) / s,
            (y[2] + z[1]) / s,
            0.25 * s,
            (x[1] - y[0]) / s,
        ]
    };
    Quaternion(rotation).normalized()
}
