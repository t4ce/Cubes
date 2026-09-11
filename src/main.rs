// trueos-blueprint: features=["tokio-net-probe"]
// Native background workers use the TRUEOS standard-library runtime.

extern crate alloc;
mod background;
mod camera_entry;
mod counters;
mod cube_format;
mod environment;
mod floor;
mod interaction_overlay;
mod carousel;
mod baked_materials { include!("../Cube/cube_driver_manifest.rs"); }
mod grid;
mod modes;
mod marker_lod;
mod platform_lod;
mod render_limits;
mod pointlist;
mod network;
mod orchard;
#[path = "SubCubes.rs"]
mod subcubes;
use modes::{ModeKeys, SceneMode};
include!(concat!(env!("OUT_DIR"), "/orchard_assets.rs"));
mod asset_brush;
mod picking;
mod reveal;
mod rubik;
mod transition;
#[path = "CubesWalkerCam.rs"]
mod walker_camera;
mod world_cube;
mod world_portals;
mod world_topology;
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
const _: () = assert!(render_limits::SEED_MAX == grid::MAX_SEED_COUNT);
const _: () = assert!(render_limits::SEED_MAX <= trueos::vgpu::MAX_RETAINED_SCENE_INSTANCES);
const _: () = assert!(render_limits::FULL_MAX == asset_brush::FULL_WORLD_SEEDS);

#[derive(Clone, Copy)]
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
            Self::MaterialShowcase => grid::MATERIAL_SHOWCASE_COUNT,
            Self::RenderLimits => 0,
        }
    }

    const fn number(self) -> u8 {
        match self {
            Self::InteractiveGrid => 1,
            Self::StaticCube => 2,
            Self::Sphere => 1,
            Self::Orchard => 4,
            Self::World => 5,
            Self::MaterialShowcase => 7,
            Self::RenderLimits => 9,
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
    carousel: carousel::Carousel,
    orchard_index: usize,
    worlds: orchard::Pages,
    world_index: usize,
    active_world: Option<world_portals::World>,
    world_cube: world_cube::Companion,
    visibility_scratch: orchard::VisibilityScratch,
    world_markers: marker_lod::Reducer,
    platform_view: platform_lod::View,
    limits: render_limits::Limits,
    limits_cursor: Option<GridCursor>,
    placed_reveal: reveal::Reveal,
    asset_brush: asset_brush::Brush,
    puzzle: rubik::Puzzle,
    mining: subcubes::Demo,
    mining_asset: orchard::Asset,
    orbit: [f32; 3], // yaw, elevation, radius
    look_target: [f32; 3],
    last_camera_activity_millis: u64,
    finished_at: Option<u64>,
    flight: Option<transition::Flight>,
    portal_trip: Option<transition::PortalTrip>,
    portal_ready_at: u64,
    selected_entry: Option<(usize, usize)>,
    selected_face_axis: Option<usize>,
    arrival_fade: Option<u64>,
    window_opacity: u8,
    number_keys: ModeKeys,
    network: network::Client,
    network_world: Option<NetworkWorld>,
    network_singleton: bool,
    picker_camera: Option<FlyCam>,
    demo_camera: Option<FlyCam>,
    walker_camera: Option<walker_camera::CubesWalkerCam>,
    pending_resize: Option<ResizeEvent>,
    previous_elapsed_millis: u64,
    first_frame: bool,
    previous_view_projection: [f32; 16],
}

struct NetworkWorld {
    bytes: Vec<u8>,
    asset: orchard::Asset,
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
        let frame = Frame::open_layered(x, y, WIDTH, HEIGHT, 60)
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
        let carousel = carousel::Carousel::new(ASSET_GRID_ASSETS, ASSET_GROUPS);
        let worlds = orchard::Pages::new(WORLD_ASSETS, false);
        let background = background::Background::start(
            frame
                .background()
                .map_err(|error| CubeError::Ui4("background-target", error))?,
        )
        .map_err(|error| CubeError::Ui4("background-start", error))?;
        Ok(Self {
            background,
            counters: counters::Sampler::new(clock::monotonic_millis()),
            carousel,
            orchard_index: 0,
            worlds,
            world_index: 0,
            visibility_scratch: orchard::VisibilityScratch::new(),
            world_markers: marker_lod::Reducer::new(),
            platform_view: platform_lod::View::new(),
            limits: render_limits::Limits::new(),
            limits_cursor: None,
            placed_reveal: reveal::Reveal::new(),
            asset_brush: asset_brush::Brush::new(),
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
            mining: subcubes::Demo::new(),
            mining_asset: orchard::Asset {
                name: "mining",
                cubes: Vec::new(),
                radius: 32.,
            },
            active_world: None,
            world_cube: world_cube::Companion::default(),
            orbit: [core::f32::consts::PI, 0.0, 7.5],
            look_target: [0.0; 3],
            last_camera_activity_millis: 0,
            finished_at: None,
            flight: None,
            portal_trip: None,
            portal_ready_at: 0,
            selected_entry: None,
            selected_face_axis: None,
            arrival_fade: None,
            window_opacity: 255,
            number_keys: ModeKeys::default(),
            network: network::Client::new(),
            network_world: None,
            network_singleton: false,
            picker_camera: None,
            demo_camera: None,
            walker_camera: None,
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
        self.service_network_world()?;
        if self.mode == SceneMode::RenderLimits {
            self.flycam.camera.position = [0., 0., -render_limits::camera_distance(self.frame.width(), self.frame.height(), PUZZLE_YFOV)];
            self.flycam.camera.rotation = look_at_camera_rotation(self.flycam.camera.position, [0.;3], [0.,-1.,0.]);
        }
        // A selected Key-2 action continues after entering a world. Only its
        // exact committed quarter-turns change the portal topology.
        self.puzzle.update(elapsed_millis);
        if self.mode == SceneMode::World {
            self.active_world
                .as_mut()
                .unwrap()
                .update(&self.puzzle, elapsed_millis);
        }
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
        if self.limits_cursor.as_ref().is_some_and(|cursor| !routed(cursor)) {
            self.limits.cancel_drag(); self.limits_cursor = None;
        }
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
            if self.mode == SceneMode::RenderLimits {
                let owner = self.limits_cursor.as_ref().is_none_or(|owner|
                    owner.source == cursor.source && owner.combo == cursor.combo && owner.virtual_cursor == cursor.virtual_cursor);
                if owner {
                    let camera = self.flycam.camera.retained(self.frame.width(), self.frame.height(), self.previous_view_projection);
                    if let Some((origin, direction)) = picking::ray(&camera.inverse_view_projection, event.local_x, event.local_y, self.frame.width(), self.frame.height()) {
                        if self.limits.pointer(origin, direction, event.buttons_pressed & 1 != 0, event.buttons_down & 1 != 0) {
                            logl::log(level::INFO, format_args!("Cubes: render limits full={} retained={}", self.limits.full(), self.limits.seeds()));
                        }
                        self.limits_cursor = self.limits.dragging().then_some(cursor);
                    }
                    if event.buttons_down & 1 == 0 { self.limits.cancel_drag(); self.limits_cursor = None; }
                }
            }
            if self.mode == SceneMode::Orchard {
                if event.buttons_pressed & 1 != 0 {
                    self.confirm_asset_picker()?;
                    // Confirming an asset must never also place it on this click.
                    continue;
                }
                if event.wheel != 0 {
                    self.carousel.wheel(-(event.wheel as i32));
                }
                self.carousel.orbit.look(event.dx as f32, event.dy as f32);
            }
            if self.mode == SceneMode::World && self.portal_trip.is_none() {
                if event.buttons_pressed & 4 != 0 {
                    self.toggle_asset_picker()?;
                    continue;
                }
                if event.wheel != 0 && self.asset_brush.tool_active {
                    self.carousel.cycle_selection(event.wheel).map_err(|_| CubeError::Contract)?;
                    self.asset_brush.confirm(self.carousel.selected_id());
                }
                if event.buttons_pressed & 1 != 0 && self.asset_brush.tool_active {
                    self.place_selected_asset()?;
                }
                if let Some(camera) = self.walker_camera.as_mut() {
                    camera.look(event.dx as f32, event.dy as f32);
                }
            }
            if self.mode == SceneMode::MaterialShowcase {
                if let Some(camera) = self.walker_camera.as_mut() {
                    camera.look(event.dx as f32, event.dy as f32);
                }
                if event.wheel != 0 {
                    self.mining.cycle(-(event.wheel as i32));
                    logl::log(
                        level::INFO,
                        format_args!(
                            "Cubes: Key7 mining tool={} side={} c1 grid=1 c1",
                            self.mining.tool_name(),
                            self.mining.tool_side().unwrap_or(0)
                        ),
                    );
                }
                if event.buttons_pressed & 4 != 0 {
                    self.mining = subcubes::Demo::new();
                    self.walker_camera = Some(walker_camera::CubesWalkerCam::mining_demo(
                        &self.mining.blocks,
                    ));
                    self.refresh_mining();
                } else if event.buttons_pressed & 1 != 0 {
                    if let Some(cut) = self.mining_target() {
                        self.mining.mine(cut);
                        self.refresh_mining();
                    }
                }
            }
            if self.mode == SceneMode::StaticCube
                && self.portal_trip.is_none()
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
                ) && let Some(hit) = picking::pick_face(
                    origin,
                    direction,
                    grid::CUBE_COMPACT_SPACING,
                    grid::CUBE_GRID_SCALE,
                    |id| self.puzzle.pose(id, 0.0, 1.0),
                ) && self.puzzle.select(hit.cubie, elapsed_millis)
                {
                    self.selected_entry = world_topology::entry(hit.cubie, hit.face_axis);
                    self.selected_face_axis = Some(hit.face_axis);
                    logl::log(
                        level::INFO,
                        format_args!(
                            "Cubes: selected cubie={} opening then 3 tracked turns",
                            hit.cubie
                        ),
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
        if self.portal_trip.is_none() {
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
            if matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase) {
                if let Some(camera) = self.walker_camera.as_mut() {
                    let before = camera.pose().0;
                    camera.update(
                        walker_camera::Input {
                            forward: ((held(0x1a) || held(0x52)) as i32
                                - (held(0x16) || held(0x51)) as i32)
                                as f32,
                            right: ((held(0x07) || held(0x4f)) as i32
                                - (held(0x04) || held(0x50)) as i32)
                                as f32,
                            roll: (held(0x14) as i32 - held(0x08) as i32) as f32,
                            boost: held(0xe1) || held(0xe5),
                            fast_walk: held(0xe1) || held(0xe5),
                            space: held(0x2c),
                            // R already opens the world cube. Home aligns the walk view.
                            align: held(0x4a),
                        },
                        delta_seconds,
                    );
                    let (position, rotation) = camera.pose();
                    self.flycam.camera.position = position;
                    self.flycam.camera.rotation = rotation;
                    if self.mode == SceneMode::World
                        && !self.network_singleton
                        && elapsed_millis >= self.portal_ready_at
                    {
                        if let Some(portal) = camera.crossed_portal(before) {
                            let route =
                                world_topology::routes(self.world_index, &self.puzzle)[portal];
                            if route != world_topology::Destination::None {
                                self.portal_trip = Some(transition::PortalTrip {
                                    source: self.world_index,
                                    portal,
                                    destination: match route {
                                        world_topology::Destination::World(w) => Some(w),
                                        _ => None,
                                    },
                                    stage: transition::PortalStage::FadeOut(elapsed_millis),
                                });
                            }
                        }
                    }
                }
            }
            if self.mode == SceneMode::StaticCube
                && self.puzzle.selected().is_some()
                && self.flight.is_none()
            {
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
            } else if !matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase | SceneMode::Orchard | SceneMode::RenderLimits)
                && self.flight.is_none()
            {
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
            self.flycam.camera.position = if self.flight.is_none()
                && self.mode == SceneMode::StaticCube
            {
                radial
            } else {
                self.flycam.camera.position
            };
            let up = orbit_up(yaw, pitch);
            if !matches!(
                self.mode,
                SceneMode::StaticCube
                    | SceneMode::Orchard
                    | SceneMode::World
                    | SceneMode::MaterialShowcase
                    | SceneMode::RenderLimits
            ) {
                self.look_target = radial.map(|v| -v);
            }
            if !matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase | SceneMode::Orchard | SceneMode::RenderLimits)
                && self.flight.is_none()
            {
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
                    let id = self.puzzle.selected().unwrap();
                    let (cell, basis) = self.puzzle.pose(id, 0.0, 1.0);
                    let axis = self.selected_face_axis.ok_or(CubeError::Contract)?;
                    let local = [
                        (id % 3) as f32 - 1.,
                        ((id / 3) % 3) as f32 - 1.,
                        (id / 9) as f32 - 1.,
                    ];
                    let normal = basis[axis].map(|v| v * local[axis]);
                    let center = cell.map(|x| x * grid::CUBE_GRID_SPACING);
                    let start = self.flycam.camera.position;
                    self.demo_camera = Some(self.flycam);
                    self.flight = Some(transition::Flight {
                        rotation: self.flycam.camera.rotation.0,
                        up,
                        started: elapsed_millis,
                        points: transition::face_approach(
                            start,
                            center,
                            normal,
                            up,
                            grid::CUBE_GRID_SCALE,
                        ),
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
                    let [y, p, _] = self.orbit;
                    let direction = [
                        -libm::cosf(p) * libm::sinf(y),
                        -libm::sinf(p),
                        -libm::cosf(p) * libm::cosf(y),
                    ];
                    self.flycam.camera.position = pos;
                    let target_rotation = look_at_camera_rotation(
                        pos,
                        core::array::from_fn(|i| pos[i] + direction[i]),
                        flight.up,
                    );
                    let blend = flight.orientation_blend(elapsed_millis);
                    let sign = if (0..4)
                        .map(|i| flight.rotation[i] * target_rotation.0[i])
                        .sum::<f32>()
                        < 0.
                    {
                        -1.
                    } else {
                        1.
                    };
                    self.flycam.camera.rotation = Quaternion(core::array::from_fn(|i| {
                        flight.rotation[i] * (1. - blend) + target_rotation.0[i] * sign * blend
                    }))
                    .normalized();
                    if flight.done(elapsed_millis) {
                        let (world, portal) = self.selected_entry.ok_or(CubeError::Contract)?;
                        self.select_mode(
                            modes::Selection {
                                mode: SceneMode::World,
                                page: Some(world),
                            },
                            Some(portal),
                        )?;
                        self.arrival_fade = Some(elapsed_millis);
                    }
                }
            }
        }
        self.update_portal_trip(elapsed_millis)?;
        let opacity = if let Some(trip) = &self.portal_trip {
            trip.opacity(elapsed_millis)
        } else if let Some(flight) = &self.flight {
            flight.opacity(elapsed_millis)
        } else if let Some(start) = self.arrival_fade {
            let elapsed = elapsed_millis.saturating_sub(start);
            if elapsed >= transition::REVEAL_MS {
                self.arrival_fade = None;
            }
            transition::reveal_opacity(elapsed)
        } else {
            255
        };
        if opacity != self.window_opacity {
            self.frame
                .set_opacity(opacity)
                .map_err(|e| CubeError::Ui4("journey-opacity", e))?;
            self.window_opacity = opacity;
        }
        if self.mode == SceneMode::Orchard {
            let position = self.carousel.orbit.position(width, height, PUZZLE_YFOV);
            let distance = libm::sqrtf(position.iter().map(|x| x * x).sum());
            if let Projection::Perspective {ref mut zfar, ..}=self.flycam.camera.projection {
                *zfar=Some((distance+16.0).max(100.0));
            }
            self.flycam.camera.position = position;
            self.flycam.camera.rotation=look_at_camera_rotation(self.flycam.camera.position,[0.;3],[0.,-1.,0.]);
        }
        let camera = self
            .flycam
            .camera
            .retained(width, height, self.previous_view_projection);
        let tan_half_fov = match self.flycam.camera.projection {
            Projection::Perspective { yfov, .. } => libm::tanf(yfov * 0.5),
            _ => libm::tanf(PUZZLE_YFOV * 0.5),
        };
        self.background
            .update(
                match self.mode {
                    SceneMode::World => background::Mode::World,
                    SceneMode::StaticCube => background::Mode::Cube,
                    _ => background::Mode::Neutral,
                },
                self.flycam.camera.rotation.0,
                delta_seconds,
                tan_half_fov,
                (width, height),
            )
            .map_err(|error| CubeError::Ui4("background-update", error))?;
        let puzzle_spacing = self.puzzle_spacing(elapsed_millis);
        if self.mode == SceneMode::Orchard {
            self.carousel.view_forward = self.flycam.camera.rotation.rotate([0., 0., -1.]);
            self.carousel.prepare(elapsed_millis).map_err(|_| CubeError::Contract)?;
            self.carousel.limit(self.limits.full().min(self.limits.seeds()-1));
        }
        let companion = self.world_cube.visible(self.mode == SceneMode::World);
        let preview_count = if self.mode == SceneMode::MaterialShowcase {
            usize::from(self.mining.tool_side().is_some())
        } else {
            0
        };
        let ghost_target = if self.mode == SceneMode::World && self.portal_trip.is_none() {
            self.walker_camera
                .as_ref()
                .and_then(|c| c.placement_target())
        } else {
            None
        };
        self.asset_brush.update_preview_with(ghost_target, |point, normal|
            self.carousel.placement(point, normal, rubik::MATERIAL_SHOWCASE_FLAG));
        let ghost_count = if self.mode == SceneMode::World { self.asset_brush.ghost.len().min(asset_brush::GHOST_SEEDS) } else { 0 };
        let (detail_budget, solid_capacity) = self.limits.scene_budget(
            preview_count + if companion { 27 } else { 0 },
            preview_count + ghost_count + if companion { world_cube::SEEDS } else { 0 },
        );
        if self.mode == SceneMode::World {
            let metadata = if platform_lod::ENABLED && !self.network_singleton {
                Some(&WORLD_PLATFORM_HULLS[self.world_index])
            } else { None };
            self.platform_view.prepare(&self.active_world.as_ref().unwrap().scene, metadata, self.flycam.camera.position);
        }
        let (visible, visibility_stats) = match self.mode {
            SceneMode::MaterialShowcase => {
                let (ids, stats) = orchard::visible_when_limited(
                    &mut self.visibility_scratch,
                    &self.mining_asset,
                    self.flycam.camera.position,
                    &camera.view_projection,
                    detail_budget,
                    |_| true,
                );
                (ids, Some(stats))
            }
            SceneMode::Orchard => (&[][..], None),
            SceneMode::World => {
                let asset = self.platform_view.asset(&self.active_world.as_ref().unwrap().scene);
                let base = if self.network_singleton {
                    self.network_world
                        .as_ref()
                        .map_or(0, |world| world.asset.cubes.len())
                } else {
                    self.worlds[self.world_index].cubes.len()
                };
                self.placed_reveal
                    .begin_frame(elapsed_millis, self.active_world.as_ref().unwrap().scene.cubes.len() - base);
                let eye = self.flycam.camera.position;
                let projection_y = camera.projection[5];
                let (ids, stats) = orchard::visible_with_admission(
                    &mut self.visibility_scratch,
                    asset,
                    eye,
                    &camera.view_projection,
                    solid_capacity,
                    |id, rank| {
                        let cube = asset.cubes[id];
                        let source_id = self.platform_view.source_id(id);
                        let placed = source_id.filter(|&source| source >= base).map(|source| source - base);
                        if cube.scale < 0.001 || placed.is_some_and(|id| !self.placed_reveal.admit(id)) {
                            return None;
                        }
                        let distance_squared = asset_brush::lod_distance_squared(cube.center, eye, &camera.view);
                        Some(rank < detail_budget && (self.platform_view.solid(id) || (placed.is_none_or(|id| self.placed_reveal.settled(id))
                            && asset_brush::detailed_with_budget(rank, cube, distance_squared, projection_y, height, detail_budget))))
                    },
                );
                self.placed_reveal.end_frame();
                (ids, Some(stats))
            }
            _ => (&[][..], None),
        };
        if self.mode == SceneMode::World {
            let source = &self.platform_view.asset(&self.active_world.as_ref().unwrap().scene).cubes;
            let base = self.active_world.as_ref().unwrap().scene.cubes.len() - self.asset_brush.worlds[self.world_index].len();
            self.world_markers.prepare_with_solids(
                source,
                visible,
                self.flycam.camera.position,
                &camera.view,
                camera.projection[5],
                height,
                detail_budget,
                |id| {
                    self.platform_view.source_id(id).filter(|&source| source >= base)
                        .map_or(1., |source| self.placed_reveal.growth_scale(source - base))
                },
                |id| self.platform_view.solid(id),
            );

        }
        match self.frame.begin_gpu_frame() {
            Ok(()) => {}
            Err(Ui4Error::Busy) => return Ok(()),
            Err(error) => return Err(CubeError::Ui4("frame-begin", error)),
        }
        let surface = self.device.acquire_ui4_surface(self.frame.window_id())
            .map_err(|code| CubeError::Vgpu("surface-acquire", code))?;
        let scene_opaque_count = if matches!(
            self.mode,
            SceneMode::Orchard | SceneMode::World | SceneMode::MaterialShowcase | SceneMode::RenderLimits
        ) {
            // The asset preview already keeps the retained group nonempty in
            // World mode. Do not add a black placeholder to an empty view.
            let count = if self.mode == SceneMode::World {
                self.world_markers.cubes.len()
            } else if self.mode == SceneMode::Orchard {
                self.carousel.drawn.len()
            } else if self.mode == SceneMode::RenderLimits {
                self.limits.cubes.len()
            } else {
                visible.len()
            };
            count.max(usize::from(preview_count == 0))
        } else {
            self.mode.seed_count().min(self.limits.seeds().saturating_sub(1))
        };
        // Visibility removes submissions, not authored scene instances. The
        // fallback seed is an ABI placeholder and is never a countable cube.
        let opaque_count =
            scene_opaque_count + preview_count + ghost_count + if companion { 27 } else { 0 };
        let countable_seed_count = visibility_stats
            .map_or(scene_opaque_count, |stats| stats.source)
            + preview_count
            + ghost_count
            + if companion { 27 } else { 0 };
        let seed_count = opaque_count
            + if self.mode == SceneMode::Orchard { 1 } else if self.mode == SceneMode::StaticCube {
                rubik::ALL_FACE_COUNT
            } else if companion {
                rubik::OUTER_FACE_COUNT
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
        // A tiny seed becomes a visible flat marker in the hull shader. If
        // the ABI needs one, place it in camera-local +Z (behind the eye),
        // never at twice the world position: that can appear as a distant dot.
        let behind = self.flycam.camera.rotation.rotate([0., 0., 2.]);
        let placeholder = core::array::from_fn(|a| self.flycam.camera.position[a] + behind[a]);
        for i in 0..scene_opaque_count {
            let (cell, basis) = self.puzzle.pose(i.min(26), turn_sin, turn_cos);
            let (translation, mut scale) = match self.mode {
                SceneMode::RenderLimits => {
                    let cube = self.limits.cubes[i]; (cube.center, cube.scale)
                }
                SceneMode::Orchard => self.carousel.drawn.get(i)
                    .map_or((placeholder, 0.0001), |draw| (draw.cube.center, draw.cube.scale)),
                SceneMode::World => {
                    self.world_markers.cubes.get(i)
                        .map_or((placeholder, 0.0001), |cube| (cube.center, cube.scale))
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
                SceneMode::MaterialShowcase => {
                    visible.get(i).map_or((placeholder, 0.0001), |&id| {
                        let c = self.mining_asset.cubes[id];
                        (c.center, c.scale)
                    })
                }
            };
            if matches!(self.mode, SceneMode::InteractiveGrid | SceneMode::Sphere)
                && scale >= 0.001 && expanded_count >= detail_budget {
                let depth = -(camera.view[2]*translation[0]+camera.view[6]*translation[1]+camera.view[10]*translation[2]+camera.view[14]);
                scale = grid::marker_scale(depth, camera.projection[5], height);
            }
            let seed = RetainedTransformSeed {
                translation,
                scale: [scale; 3],
                rotation: if self.mode == SceneMode::StaticCube {
                    quaternion_from_rotation_columns(basis[0], basis[1], basis[2]).0
                } else if self.mode == SceneMode::World && scale < 0.001 {
                    self.flycam.camera.rotation.0
                } else if matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase) {
                    orchard::WORLD_ROTATION
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                },
                local_radius: grid::CUBE_LOCAL_RADIUS,
                previous_translation: translation,
                draw_group: u32::from(self.mode == SceneMode::Orchard),
                flags: ((i as u32) << 16)
                    | if self.mode == SceneMode::RenderLimits {
                        self.limits.cubes[i].flags
                    } else if self.mode == SceneMode::StaticCube {
                        rubik::PALETTE_FLAG | rubik::ALL_FACES_FLAG | i as u32
                    } else if self.mode == SceneMode::InteractiveGrid {
                        rubik::ROOM_PALETTE_FLAG
                    } else if self.mode == SceneMode::Orchard {
                        self.carousel.drawn.get(i).map_or(carousel::FLAGS, |draw| {
                            if draw.cube.flags & orchard::CUSTOM_RGB555 == 0 {
                                return carousel::FLAGS | draw.cube.flags | (draw.opacity << 10);
                            }
                            let rgb=draw.cube.flags & 0x7fff;
                            let index=baked_materials::CAROUSEL_COLORS.binary_search(&rgb).unwrap();
                            carousel::FLAGS | index as u32 | (draw.opacity << 10)
                        })
                    } else if self.mode == SceneMode::World {
                        self.world_markers.cubes.get(i)
                            .map_or(orchard::CUSTOM_RGB555, |cube| cube.flags)
                    } else if self.mode == SceneMode::Sphere {
                        rubik::SPHERE_GRADIENT_FLAG
                    } else {
                        visible.get(i).map_or(rubik::MATERIAL_SHOWCASE_FLAG, |&id| {
                            self.mining_asset.cubes[id].flags
                        })
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
        if self.mode == SceneMode::MaterialShowcase && preview_count > 0 {
            // A fixed-scale tool swatch keeps wheel selection visible even off-target.
            let placement = world_cube::Placement::asset(width, height, tan_half_fov, 1.4);
            let (local, scale) = placement.asset_pose(
                [0.; 3],
                self.mining.tool_side().unwrap() as f32 * subcubes::C1 * 0.5,
            );
            let offset = self.flycam.camera.rotation.rotate(local);
            let translation = core::array::from_fn(|a| self.flycam.camera.position[a] + offset[a]);
            let basis = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
                .map(|v| self.flycam.camera.rotation.rotate(world_cube::orient(v)));
            let seed = RetainedTransformSeed {
                translation,
                scale: [scale; 3],
                rotation: quaternion_from_rotation_columns(basis[0], basis[1], basis[2]).0,
                local_radius: grid::CUBE_LOCAL_RADIUS,
                previous_translation: translation,
                draw_group: 0,
                flags: ((scene_opaque_count as u32) << 16) | orchard::CUSTOM_RGB555 | 0x7fff,
            };
            encode_seed(
                seed,
                &mut seed_bytes[scene_opaque_count * 64..(scene_opaque_count + 1) * 64],
            );
            expanded_count += 1;
        }
        for i in 0..ghost_count {
            let cube = self.asset_brush.ghost[i * self.asset_brush.ghost.len() / ghost_count];
            let depth = -(camera.view[2] * cube.center[0]
                + camera.view[6] * cube.center[1]
                + camera.view[10] * cube.center[2]
                + camera.view[14]);
            let scale = asset_brush::marker_scale(cube.scale, depth, camera.projection[5], height);
            let row = scene_opaque_count + preview_count + i;
            let seed = RetainedTransformSeed {
                translation: cube.center,
                previous_translation: cube.center,
                scale: [scale; 3],
                rotation: self.flycam.camera.rotation.0,
                local_radius: grid::CUBE_LOCAL_RADIUS,
                draw_group: 0,
                flags: ((row as u32) << 16) | cube.flags,
            };
            encode_seed(seed, &mut seed_bytes[row * 64..(row + 1) * 64]);
        }
        if companion {
            let placement = world_cube::Placement::new(
                width,
                height,
                tan_half_fov,
                self.world_cube.expansion(elapsed_millis),
            );
            for id in 0..27 {
                let (cell, basis) = self.puzzle.pose(id, turn_sin, turn_cos);
                let (position, basis, scale) = placement.pose(cell, basis);
                let offset = self.flycam.camera.rotation.rotate(position);
                let translation =
                    core::array::from_fn(|a| self.flycam.camera.position[a] + offset[a]);
                let basis = basis.map(|axis| self.flycam.camera.rotation.rotate(axis));
                let row = scene_opaque_count + preview_count + ghost_count + id;
                let seed = RetainedTransformSeed {
                    translation,
                    previous_translation: translation,
                    scale: [scale; 3],
                    rotation: quaternion_from_rotation_columns(basis[0], basis[1], basis[2]).0,
                    local_radius: grid::CUBE_LOCAL_RADIUS,
                    draw_group: 0,
                    flags: ((row as u32) << 16) | rubik::PALETTE_FLAG | id as u32,
                };
                opaque_seeds[id] = seed;
                encode_seed(seed, &mut seed_bytes[row * 64..(row + 1) * 64]);
                expanded_count += 1;
            }
        }
        if self.mode == SceneMode::StaticCube || companion {
            let all_faces = self.mode == SceneMode::StaticCube;
            let mut faces = Vec::with_capacity(seed_count - opaque_count);
            for id in 0..27 {
                let seed = opaque_seeds[id];
                let basis = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]
                    .map(|a| Quaternion(seed.rotation).rotate(a));
                for face in rubik::palette_faces(id, all_faces) {
                    let axis = face / 2;
                    let sign = if face % 2 == 0 { 1.0 } else { -1.0 };
                    let p: [f32; 3] = core::array::from_fn(|i| {
                        seed.translation[i] + basis[axis][i] * sign * seed.scale[0]
                    });
                    let depth = -(camera.view[2] * p[0]
                        + camera.view[6] * p[1]
                        + camera.view[10] * p[2]
                        + camera.view[14]);
                    faces.push((depth, id, face as u32));
                }
            }
            faces.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (slot, (_, id, face)) in faces.iter().enumerate() {
                let mut seed = opaque_seeds[*id];
                seed.draw_group = 1;
                seed.flags = ((slot as u32) << 16) | (seed.flags & 0xffff) | 512 | (*face << 10);
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
        if self.mode == SceneMode::Orchard {
            let seed = carousel_anchor_seed(placeholder);
            encode_seed(seed, &mut seed_bytes[opaque_count * 64..seed_count * 64]);
        }
        if seed_count > self.limits.seeds() || expanded_count > self.limits.full() {
            return Err(CubeError::Contract);
        }
        write_exact(
            self.device,
            self.seed_buffer,
            &seed_bytes[..seed_count * 64],
        )
        .map_err(|code| CubeError::Vgpu("grid-seed-upload", code))?;
        let outline = if matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase) {
            self.walker_camera.as_ref().and_then(|c| c.snap_outline())
        } else { None };
        let (floor_bytes, line_color) = if self.mode == SceneMode::MaterialShowcase {
            let target = self.mining.target_details(
                self.flycam.camera.position,
                self.flycam.camera.rotation.rotate([0., 0., -1.]),
            );
            let mut overlay = floor::Overlay::new();
            if let Some(snap) = outline.as_ref() {
                overlay.cube(&camera.view_projection, snap.lo, snap.hi);
            }
            if let Some(target) = target {
                overlay.mining_grid(&camera.view_projection, target);
            }
            (overlay.bytes, [255, 255, 255, 191]) // 75% straight alpha.
        } else if let Some(target) = outline.as_ref() {
            (
                floor::cube_outline(&camera.view_projection, target.lo, target.hi),
                if target.reachable { [255, 255, 255, 255] } else { [0, 0, 0, 255] },
            )
        } else {
            (
                floor::vertices(&camera.view_projection, self.mode == SceneMode::StaticCube),
                [100, 100, 100, 255],
            )
        };
        let interaction_mode = matches!(self.mode, SceneMode::World | SceneMode::MaterialShowcase);
        let guide_quads = if interaction_mode {
            interaction_overlay::strokes(&floor_bytes, width, height, line_color)
        } else { Vec::new() };
        // Keep the retained static-buffer contract stable across mode changes.
        // Interaction guides now use UI4's post-render overlay, not LINE_LIST.
        let floor_bytes = if interaction_mode {
            floor::vertices(&camera.view_projection, false)
        } else { floor_bytes };
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
                                    rgba8_srgb: u32::from_le_bytes(line_color),
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
                    draw_count: if !companion
                        && matches!(
                            self.mode,
                            SceneMode::Sphere
                                | SceneMode::World
                                | SceneMode::MaterialShowcase
                                | SceneMode::RenderLimits
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
                        if !companion
                            && matches!(
                                self.mode,
                                SceneMode::Sphere
                                    | SceneMode::World
                                    | SceneMode::MaterialShowcase
                                    | SceneMode::RenderLimits
                            )
                        {
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
        interaction_overlay::draw(&mut self.frame, &guide_quads)
            .map_err(|error| CubeError::Ui4("interaction-overlay", error))?;
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
            if interaction_mode {
                logl::log(level::INFO, format_args!(
                    "Cubes: guides mode={} tool={} landing={} quads={} path=ui4-after-retained",
                    self.mode.number(),
                    if self.mode == SceneMode::MaterialShowcase { self.mining.tool_name() } else { "space" },
                    outline.is_some(), guide_quads.len()
                ));
            }
            if self.mode == SceneMode::World {
                if let Some((platforms, detailed)) = self.platform_view.counts() {
                    let source = &self.active_world.as_ref().unwrap().scene;
                    logl::log(level::INFO, format_args!(
                        "Cubes: platform-lod detailed={}/{} hulls={} source={} candidates={}",
                        detailed, platforms, platforms - detailed, source.cubes.len(), self.platform_view.asset(source).cubes.len()
                    ));
                }
                logl::log(level::INFO, format_args!(
                    "Cubes: marker-lod dots_before={} dots_after={} seeds_submitted={} detail_budget={} retained_limit={}",
                    self.world_markers.dots_before, self.world_markers.dots_after, seed_count,
                    detail_budget, self.limits.seeds()
                ));
            }
        }
        self.previous_view_projection = camera.view_projection;
        Ok(())
    }

    fn service_mode_hotkeys(&mut self) -> Result<(), CubeError> {
        let state = self
            .frame
            .keyboard_state()
            .map_err(|error| CubeError::Ui4("mode-hotkeys", error))?;
        let r_held = state
            .as_ref()
            .is_some_and(|keyboard| keyboard.is_down(0x15));
        let network_held = state
            .as_ref()
            .is_some_and(|keyboard| keyboard.is_down(0x25));
        let navigation = state.as_ref().map_or(0, |keyboard| {
            (keyboard.is_down(0x04) as u8)
                | ((keyboard.is_down(0x07) as u8) << 1)
                | ((keyboard.is_down(0x1a) as u8) << 2)
                | ((keyboard.is_down(0x16) as u8) << 3)
        });
        let (asset_step, group_step) = self.carousel.key_input(
            navigation, self.mode == SceneMode::Orchard,
        );
        self.network.key(
            network_held,
            self.flycam.camera.position,
            self.flycam.camera.rotation.rotate([0.0, 0.0, -1.0]),
        );
        let current = state.map_or(0, |keyboard| {
            (keyboard.is_down(0x1e) as u16)
                | ((keyboard.is_down(0x1f) as u16) << 1)
                | (((keyboard.is_down(0x22) || keyboard.is_down(0x3e)) as u16) << 4)
                | ((keyboard.is_down(0x24) as u16) << 6)
                | ((keyboard.is_down(0x26) as u16) << 8)
        });
        if let Some(selection) = self.number_keys.update(
            current,
            self.mode,
            self.orchard_index,
            self.carousel.group_count(),
            self.worlds.len(),
        ) {
            self.network_singleton = false;
            self.network_world = None;
            if self.portal_trip.is_some() {
                self.puzzle.cancel_travel(self.previous_elapsed_millis);
            }
            self.select_mode(selection, None)?;
        } else if self.mode == SceneMode::Orchard {
            if group_step != 0 {
                let page = self.carousel.adjacent_group(group_step);
                self.carousel.select_group(page).map_err(|_| CubeError::Contract)?;
                self.orchard_index = page;
            }
            if asset_step != 0 {
                self.carousel.wheel(asset_step);
            }
        }
        self.world_cube.key(
            r_held,
            self.mode == SceneMode::World,
            self.previous_elapsed_millis,
        );
        Ok(())
    }

    fn service_network_world(&mut self) -> Result<(), CubeError> {
        let Some(result) = self.network.take_ready() else {
            return Ok(());
        };
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(error) => {
                logl::log(level::INFO, format_args!("Cubes: Key8 cubesrv {error}"));
                return Ok(());
            }
        };
        let mut asset =
            orchard::decode(WORLD_ASSETS[world_topology::VOID].0, &bytes).map_err(|error| {
                logl::log(
                    level::WARN,
                    format_args!("Cubes: Key8 cubesrv world rejected={error}"),
                );
                CubeError::Contract
            })?;
        for cube in &mut asset.cubes {
            cube.center = orchard::world_from_demo(cube.center);
        }
        logl::log(
            level::INFO,
            format_args!(
                "Cubes: Key8 cubesrv connected world=27 bytes={} cubes={}",
                bytes.len(),
                asset.cubes.len()
            ),
        );
        self.network_world = Some(NetworkWorld { bytes, asset });
        self.network_singleton = true;
        self.select_mode(
            modes::Selection {
                mode: SceneMode::World,
                page: Some(world_topology::VOID),
            },
            None,
        )
    }

    fn toggle_asset_picker(&mut self) -> Result<(), CubeError> {
        if self.asset_brush.tool_active { self.asset_brush.disable(); }
        else { self.open_asset_picker()?; }
        Ok(())
    }

    fn confirm_asset_picker(&mut self) -> Result<(), CubeError> {
        self.asset_brush.confirm(self.carousel.selected_id());
        self.restore_picker_world()
    }

    fn open_asset_picker(&mut self) -> Result<(), CubeError> {
        self.carousel.select_item(self.carousel.group, self.carousel.selected).map_err(|_| CubeError::Contract)?;
        self.picker_camera = Some(self.flycam);
        self.mode = SceneMode::Orchard;
        self.set_mode_projection(self.mode);
        self.cursors.clear();
        // Keep the live world, walker and placement reveal state in place.
        logl::log(level::INFO, format_args!("Cubes: asset picker group={} selected={} W/S=groups wheel/AD=assets LMB=confirm",
            self.carousel.name(), self.carousel.asset_name()));
        Ok(())
    }

    fn restore_picker_world(&mut self) -> Result<(), CubeError> {
        let Some(camera) = self.picker_camera.take() else { return Ok(()); };
        self.flycam = camera;
        self.mode = SceneMode::World;
        self.set_mode_projection(self.mode);
        self.previous_view_projection = self.flycam.camera.retained(self.frame.width(), self.frame.height(), [0.;16]).view_projection;
        self.cursors.clear();
        Ok(())
    }

    fn place_selected_asset(&mut self) -> Result<(), CubeError> {
        if !self.asset_brush.tool_active { return Ok(()); }
        let Some(camera) = self.walker_camera.as_mut() else {
            return Ok(());
        };
        let Some((point, normal)) = camera.placement_target() else {
            return Ok(());
        };
        let placed = self.carousel.placement(point, normal, rubik::MATERIAL_SHOWCASE_FLAG);
        let stored = &mut self.asset_brush.worlds[self.world_index];
        if stored.len() + placed.len() > asset_brush::MAX_PLACED_CUBES {
            return Ok(());
        }
        let bounds: Vec<_> = placed.iter().map(|c| (c.center, c.scale)).collect();
        if !camera.add_placed(&bounds) {
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: asset placement blocked by terrain, camera, portal clearance, or world bounds"
                ),
            );
            return Ok(());
        }
        self.active_world
            .as_mut()
            .unwrap()
            .scene
            .cubes
            .extend_from_slice(&placed);
        stored.extend(placed);
        self.placed_reveal.append(stored.len());
        self.world_markers.set_bounds(&self.active_world.as_ref().unwrap().scene.cubes);
        Ok(())
    }

    fn portal_flight(
        &self,
        world: usize,
        portal: usize,
        now: u64,
        reverse: bool,
    ) -> transition::Flight {
        let (cell, _) = self.puzzle.pose(world_topology::cubie(world), 0., 1.);
        let spacing = if reverse {
            grid::CUBE_COMPACT_SPACING
        } else {
            grid::CUBE_GRID_SPACING
        };
        let center = cell.map(|v| v * spacing);
        let normal = world_topology::portal_normal(world, portal, &self.puzzle);
        let up = if normal[1].abs() > 0.9 {
            [0., 0., 1.]
        } else {
            [0., 1., 0.]
        };
        let start = if reverse {
            let outward: [f32; 3] = core::array::from_fn(|i| cell[i] + normal[i] * 2.);
            let length = libm::sqrtf(outward.iter().map(|x| x * x).sum::<f32>()).max(0.1);
            outward.map(|v| v * 7.5 / length)
        } else {
            self.flycam.camera.position
        };
        let mut points =
            transition::face_approach(start, center, normal, up, grid::CUBE_GRID_SCALE);
        if reverse {
            points.reverse();
        }
        transition::Flight {
            started: now,
            points,
            up,
            rotation: if reverse {
                look_at_camera_rotation(points[0], center, up).0
            } else {
                self.flycam.camera.rotation.0
            },
        }
    }
    fn present_portal_flight(&mut self, flight: &transition::Flight, target: [f32; 3], now: u64) {
        let pos = flight.position(now);
        let target_rotation = look_at_camera_rotation(pos, target, flight.up);
        let blend = flight.orientation_blend(now);
        let sign = if (0..4)
            .map(|i| flight.rotation[i] * target_rotation.0[i])
            .sum::<f32>()
            < 0.
        {
            -1.
        } else {
            1.
        };
        self.flycam.camera.position = pos;
        self.flycam.camera.rotation = Quaternion(core::array::from_fn(|i| {
            flight.rotation[i] * (1. - blend) + target_rotation.0[i] * sign * blend
        }))
        .normalized();
    }
    fn update_portal_trip(&mut self, now: u64) -> Result<(), CubeError> {
        use transition::PortalStage;
        let Some(mut trip) = self.portal_trip.take() else {
            return Ok(());
        };
        match &trip.stage {
            PortalStage::FadeOut(start) if now.saturating_sub(*start) >= transition::REVEAL_MS => {
                self.select_mode(
                    modes::Selection {
                        mode: SceneMode::StaticCube,
                        page: None,
                    },
                    None,
                )?;
                let flight = self.portal_flight(trip.source, trip.portal, now, true);
                self.flycam.camera.position = flight.points[0];
                self.flycam.camera.rotation = Quaternion(flight.rotation);
                trip.stage = PortalStage::Exit(flight);
            }
            PortalStage::Exit(flight) => {
                let (cell, _) = self.puzzle.pose(world_topology::cubie(trip.source), 0., 1.);
                self.present_portal_flight(
                    flight,
                    cell.map(|v| v * grid::CUBE_COMPACT_SPACING),
                    now,
                );
                if flight.done(now) {
                    if let Some(destination) = trip.destination {
                        self.puzzle.travel_turn(
                            world_topology::cubie(trip.source),
                            world_topology::cubie(destination),
                            now,
                        );
                        trip.stage = PortalStage::Turn;
                    } else {
                        let p = self.flycam.camera.position;
                        self.orbit = [
                            libm::atan2f(p[0], p[2]),
                            libm::atan2f(p[1], libm::sqrtf(p[0] * p[0] + p[2] * p[2])),
                            7.5,
                        ];
                        self.look_target = [0.; 3];
                        return Ok(());
                    }
                }
            }
            PortalStage::Turn => {
                if !self.puzzle.locked() {
                    let destination = trip.destination.ok_or(CubeError::Contract)?;
                    let arrival = world_topology::routes(destination, &self.puzzle)
                        .iter()
                        .position(|&d| d == world_topology::Destination::World(trip.source))
                        .ok_or(CubeError::Contract)?;
                    let flight = self.portal_flight(destination, arrival, now, false);
                    trip.stage = PortalStage::Enter { flight, arrival };
                }
            }
            PortalStage::Enter { flight, arrival } => {
                // Look just through the face so the endpoint has a stable heading.
                let destination = trip.destination.ok_or(CubeError::Contract)?;
                let normal = world_topology::portal_normal(destination, *arrival, &self.puzzle);
                let target = core::array::from_fn(|i| flight.points[3][i] - normal[i] * 0.1);
                self.present_portal_flight(flight, target, now);
                if flight.done(now) {
                    let arrival = *arrival;
                    self.select_mode(
                        modes::Selection {
                            mode: SceneMode::World,
                            page: Some(destination),
                        },
                        Some(arrival),
                    )?;
                    self.arrival_fade = Some(now);
                    return Ok(());
                }
            }
            _ => {}
        }
        self.portal_trip = Some(trip);
        Ok(())
    }

    fn select_mode(
        &mut self,
        selection: modes::Selection,
        arrival: Option<usize>,
    ) -> Result<(), CubeError> {
        // Number-key navigation can abandon the picker. Restore the world
        // context first, so the usual mode-exit cleanup remains correct.
        if self.picker_camera.is_some() { self.restore_picker_world()?; }
        self.portal_trip = None;
        self.limits.cancel_drag(); self.limits_cursor = None;
        let mode = selection.mode;
        if mode == SceneMode::World {
            self.placed_reveal.reset();
            self.portal_ready_at = self.previous_elapsed_millis + transition::PORTAL_COOLDOWN_MS;
        }
        match mode {
            SceneMode::Orchard => self.orchard_index = selection.page.unwrap(),
            SceneMode::World => self.world_index = selection.page.unwrap(),
            _ => {}
        }
        if mode == SceneMode::Orchard {
            self.carousel.select_group(self.orchard_index).map_err(|_| CubeError::Contract)?;
        }
        if mode == SceneMode::World {
            self.worlds.load_world(self.world_index).map_err(|_| CubeError::Contract)?;
        }
        if mode != SceneMode::World && self.mode != SceneMode::World {
            self.demo_camera = None;
        }
        if mode == SceneMode::World && self.mode != SceneMode::World && self.demo_camera.is_none() {
            self.demo_camera = Some(self.flycam);
        } else if mode != SceneMode::World && self.mode == SceneMode::World {
            self.walker_camera = None;
            if let Some(camera) = self.demo_camera.take() {
                self.flycam = camera;
            }
        }
        if self.mode == SceneMode::MaterialShowcase && mode != SceneMode::MaterialShowcase {
            self.walker_camera = None;
        }
        if mode == SceneMode::Orchard && self.mode != mode {
            self.carousel.orbit = carousel::Orbit::default();
        }
        self.mode = mode;
        self.frame
            .set_center_snapped_mouse(matches!(
                mode,
                SceneMode::World | SceneMode::MaterialShowcase | SceneMode::Orchard
            ))
            .map_err(|error| CubeError::Ui4("center-snapped-mouse", error))?;
        if mode == SceneMode::StaticCube {
            self.puzzle.reenter(self.previous_elapsed_millis);
        }
        self.last_camera_activity_millis = self.previous_elapsed_millis;
        self.finished_at = None;
        self.flight = None;
        if self.puzzle.selected().is_none() {
            self.selected_entry = None;
            self.selected_face_axis = None;
        }
        self.arrival_fade = None;
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
            self.flycam.camera.rotation = look_at_camera_rotation(p, [0.0; 3], [0.0, -1.0, 0.0]);
        }
        if mode == SceneMode::MaterialShowcase {
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: Key7 rows={:?} c1={} renderer units",
                    subcubes::NAMES,
                    subcubes::C1
                ),
            );
            self.mining = subcubes::Demo::new();
            self.walker_camera = Some(walker_camera::CubesWalkerCam::mining_demo(
                &self.mining.blocks,
            ));
            self.refresh_mining();
            let (position, rotation) = self.walker_camera.as_ref().unwrap().pose();
            self.flycam.camera.position = position;
            self.flycam.camera.rotation = rotation;
        }
        if mode == SceneMode::Orchard {
            self.orbit=[core::f32::consts::PI,0.,12.];
            self.look_target=[0.;3];
            logl::log(level::INFO,format_args!("Cubes: asset picker group={} assets={} selected={} five-row-slots alpha=.25/.5/1/.5/.25 group-previews=above/below alpha=.5 wheel/AD=slide W/S=next/previous-group mouse=orbit",
                self.carousel.name(),self.carousel.group_len(),self.carousel.asset_name()));
        } else if mode == SceneMode::World {
            self.flycam = FlyCam::new(default_camera(), 3.0);
            let (world_name, world_bytes, asset) = if self.network_singleton {
                let network = self.network_world.as_ref().ok_or(CubeError::Contract)?;
                (network.asset.name, network.bytes.as_slice(), &network.asset)
            } else {
                (
                    WORLD_ASSETS[self.world_index].0,
                    WORLD_ASSETS[self.world_index].1,
                    &self.worlds[self.world_index],
                )
            };
            let mut camera = if self.network_singleton {
                walker_camera::CubesWalkerCam::from_world(
                    world_bytes,
                    self.world_index == world_topology::VOID,
                )
            } else {
                walker_camera::CubesWalkerCam::from_portal(
                    world_bytes,
                    self.world_index == world_topology::VOID,
                    arrival,
                )
            };
            let placed_bounds: Vec<_> = self.asset_brush.worlds[self.world_index]
                .iter()
                .map(|c| (c.center, c.scale))
                .collect();
            if !placed_bounds.is_empty() && !camera.add_placed(&placed_bounds) {
                return Err(CubeError::Contract);
            }
            let (position, rotation) = camera.pose();
            self.flycam.camera.position = position;
            self.flycam.camera.rotation = rotation;
            self.walker_camera = Some(camera);
            self.background
                .select_world(world_name, self.flycam.camera.rotation.0)
                .map_err(|error| CubeError::Ui4("background-world", error))?;
            let palette = environment::Palette::for_world(world_name).ok_or(CubeError::Contract)?;
            // One hardware-register update per Key-5 selection, not per
            // frame or mouse movement. Preserve the world if unavailable.
            if let Err(error) = self.frame.set_display_bottom_color(palette.average_rgb()) {
                logl::log(
                    level::WARN,
                    format_args!("Cubes: display bottom color unavailable: {error:?}"),
                );
            }
            self.active_world = Some(world_portals::World::new(
                self.world_index,
                asset,
                world_bytes,
                &self.puzzle,
            ));
            self.active_world
                .as_mut()
                .unwrap()
                .scene
                .cubes
                .extend_from_slice(&self.asset_brush.worlds[self.world_index]);
            self.world_markers.set_bounds(&self.active_world.as_ref().unwrap().scene.cubes);
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: {} world={}/{} asset={} authored={} first_person=surface_walk streamed_nearest={} renderer_seed_limit={} portals={}",
                    if self.network_singleton {
                        "Key8 cubesrv"
                    } else {
                        "Key5"
                    },
                    self.world_index + 1,
                    self.worlds.len(),
                    asset.name,
                    asset.cubes.len(),
                    self.limits.seeds(),
                    grid::MAX_SEED_COUNT,
                    if self.network_singleton {
                        "disabled"
                    } else {
                        "enabled"
                    }
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
                    SceneMode::InteractiveGrid => "1 interactive-grid Key1=sphere",
                    SceneMode::StaticCube =>
                        "2 compact-puzzle click=face/edge/corner turns=3x1s camera=WASD-orbit idle=3s-auto-orbit",
                    SceneMode::Sphere =>
                        "1 sphere=1024 camera=center WASD=look cursor-expand=10%-area Key1=interactive-grid",
                    SceneMode::Orchard => "asset-picker wheel/AD=slide/loop W/S=next/previous-group mouse=orbit LMB=confirm five-row-assets alpha=.25/.5/1/.5/.25 group-previews=above/below alpha=.5",
                    SceneMode::World =>
                        "5 lvl27-world first-person mouse-look WASD=surface-walk Shift=walk/flight-boost Space=edge-push/approach Home=align Key5=next-world R=display-cube MMB=picker/tool-off wheel=group-assets LMB=place",
                    SceneMode::RenderLimits => "9 render limits upper=full-geometry lower=retained-seeds click/drag=16-steps Key5=world",
                    SceneMode::MaterialShowcase =>
                        "7 mining 7 tiers x 6 materials mouse-look WASD=walk/fly Shift=boost Space=push/approach Home=align wheel=none/c1/c2/c3/c4 LMB=mine RMB=reset grid=c1",
                },
                if mode == SceneMode::Orchard {
                    self.carousel.drawn.len()
                } else if mode == SceneMode::World {
                    self.active_world
                        .as_ref()
                        .map_or(0, |world| world.scene.cubes.len())
                } else {
                    mode.seed_count()
                },
            ),
        );
        Ok(())
    }

    fn mining_target(&self) -> Option<subcubes::Block> {
        self.mining.target(
            self.flycam.camera.position,
            self.flycam.camera.rotation.rotate([0., 0., -1.]),
        )
    }
    fn refresh_mining(&mut self) {
        self.mining_asset.cubes = self
            .mining
            .blocks
            .iter()
            .map(|&b| {
                let (center, scale) = b.pose();
                orchard::Cube {
                    center,
                    scale,
                    flags: rubik::MATERIAL_SHOWCASE_FLAG | b.material,
                }
            })
            .collect();
        if let Some(camera) = self.walker_camera.as_mut() {
            camera.replace_mining_blocks(&self.mining.blocks);
        }
    }

    fn puzzle_spacing(&self, now: u64) -> f32 {
        let compact = grid::CUBE_COMPACT_SPACING;
        compact + (grid::CUBE_GRID_SPACING - compact) * self.puzzle.expansion(now)
    }

    fn set_mode_projection(&mut self, mode: SceneMode) {
        let zfar = match mode {
            SceneMode::World | SceneMode::MaterialShowcase => self
                .walker_camera
                .as_ref()
                .map_or(100., walker_camera::CubesWalkerCam::far_plane),
            SceneMode::Orchard => 100.,
            _ => 100.,
        };
        if mode == SceneMode::World {
            logl::log(
                level::INFO,
                format_args!(
                    "Cubes: world projection near={} far={} renderer_units c1={}",
                    walker_camera::NEAR, zfar, subcubes::C1
                ),
            );
        }
        self.flycam.camera.projection = Projection::Perspective {
            yfov: match mode {
                SceneMode::InteractiveGrid => ROOM_YFOV,
                SceneMode::StaticCube => PUZZLE_YFOV,
                SceneMode::Sphere => ROOM_YFOV,
                SceneMode::Orchard => PUZZLE_YFOV,
                SceneMode::World => walker_camera::FOV,
                SceneMode::MaterialShowcase => walker_camera::FOV,
                SceneMode::RenderLimits => PUZZLE_YFOV,
            },
            znear: if matches!(mode, SceneMode::World | SceneMode::MaterialShowcase) {
                walker_camera::NEAR
            } else {
                0.1
            },
            zfar: Some(zfar),
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
                self.background.resized();
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

fn carousel_anchor_seed(placeholder: [f32; 3]) -> RetainedTransformSeed {
    RetainedTransformSeed {
        translation: placeholder,
        previous_translation: placeholder,
        scale: [0.0001; 3],
        // Hidden seeds still pass transform validation; Default has a zero quaternion.
        rotation: [0., 0., 0., 1.],
        local_radius: grid::CUBE_LOCAL_RADIUS,
        draw_group: 0,
        flags: 0,
    }
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
