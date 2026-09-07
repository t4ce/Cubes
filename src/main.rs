#![no_std]

extern crate alloc;
mod grid;
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
    vsys,
};
use trueos_picasso::Picasso;
use trueos_picasso::cam::{Camera, Projection, Quaternion};

// Runtime input contains no imported mesh: HS generates all 44 triangles.
const CUBE_VERTEX_COUNT: u32 = 1;
const CUBE_INDEX_COUNT: u32 = 44;
const CUBE_VERTICES: &[u8] = &[0; 12];
const CUBE_INDICES: &[u8] = &[0; 44 * 4];

const CUBE_SOURCE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cube/cube.glb"));
const WIDTH: u32 = 784;
const HEIGHT: u32 = 441;

struct GridCursor {
    source: CursorSource,
    combo: u32,
    virtual_cursor: bool,
    local: [i32; 2],
}

#[derive(Clone, Copy, Debug)]
enum CubeError {
    Contract,
    Ui4(&'static str, Ui4Error),
    Vgpu(&'static str, i32),
}

struct CubeScene {
    frame: Frame,
    device: Device,
    queue: Queue,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    camera: Camera,
    seed_buffer: Buffer,
    cursors: Vec<GridCursor>,
    pending_resize: Option<ResizeEvent>,
    previous_elapsed_millis: u64,
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

    let mut scene = CubeScene::open(&vertices, &indices)?;
    let started = clock::monotonic_millis();
    loop {
        scene.render(clock::monotonic_millis().saturating_sub(started))?;
        vsys::sleep_ms(16);
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
        let frame = Frame::open_streaming(x, y, WIDTH, HEIGHT)
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
        let seed_buffer = device
            .create_buffer(
                grid::COUNT * 64,
                BUFFER_USAGE_MAP_READ | BUFFER_USAGE_MAP_WRITE,
            )
            .map_err(|code| CubeError::Vgpu("grid-seed-buffer", code))?;
        logl::log(
            level::INFO,
            format_args!(
                "Cubes: grid=16x9 retained_seeds=144 cursor_radius=cube-scale-linked mouse=per-source-xy camera=fixed-grid-facing keys=W/S inactive=3px-flat-seed-markers",
            ),
        );
        Ok(Self {
            frame,
            device,
            queue,
            vertices,
            indices,
            mesh,
            camera,
            seed_buffer,
            cursors: Vec::new(),
            pending_resize: None,
            previous_elapsed_millis: 0,
            previous_view_projection: camera.retained(WIDTH, HEIGHT, [0.0; 16]).view_projection,
        })
    }

    fn render(&mut self, elapsed_millis: u64) -> Result<(), CubeError> {
        self.service_resize_events()?;
        let delta_seconds =
            elapsed_millis.saturating_sub(self.previous_elapsed_millis) as f32 * 0.001;
        self.previous_elapsed_millis = elapsed_millis;
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
        let held = |usage| {
            routes
                .iter()
                .filter(|r| r.selected_for_window && r.application_focus)
                .any(|r| r.keyboard.as_ref().is_some_and(|k| k.is_down(usage)))
        };
        self.camera.position[2] = grid::move_camera(
            self.camera.position[2],
            held(0x1a),
            held(0x16),
            delta_seconds,
        );
        let width = self.frame.width();
        let height = self.frame.height();
        let camera = self
            .camera
            .retained(width, height, self.previous_view_projection);
        let cursor_radius_px = grid::cursor_radius_px(
            grid::CUBE_SCALE,
            self.camera.position[2],
            camera.projection[5],
            height,
        );
        match self.frame.begin_gpu_frame() {
            Ok(()) => {}
            Err(Ui4Error::Busy) => return Ok(()),
            Err(error) => return Err(CubeError::Ui4("frame-begin", error)),
        }
        let surface = self
            .device
            .acquire_ui4_surface(self.frame.window_id())
            .map_err(|code| CubeError::Vgpu("surface-acquire", code))?;
        let mut seed_bytes = [0u8; grid::COUNT * 64];
        for i in 0..grid::COUNT {
            let translation = grid::position(i);
            let active = grid::project(&camera.view_projection, translation, width, height)
                .is_some_and(|point| {
                    self.cursors
                        .iter()
                        .any(|c| grid::near(point, c.local, width, height, cursor_radius_px))
                });
            let seed = RetainedTransformSeed {
                translation,
                scale: [if active {
                    grid::CUBE_SCALE
                } else {
                    grid::marker_scale(self.camera.position[2], camera.projection[5], height)
                }; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                local_radius: grid::CUBE_LOCAL_RADIUS,
                previous_translation: translation,
                draw_group: 0,
                flags: (i as u32) << 16,
            };
            encode_seed(seed, &mut seed_bytes[i * 64..(i + 1) * 64]);
        }
        write_exact(self.device, self.seed_buffer, &seed_bytes)
            .map_err(|code| CubeError::Vgpu("grid-seed-upload", code))?;
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
                            clear_rgba8_srgb: u32::from_le_bytes([0, 128, 0, 0]),
                            ..RetainedFrameSubmit::default()
                        },
                        ..RetainedFrameSubmitV2::default()
                    },
                    seed_buffer: self.seed_buffer.raw(),
                    seed_count: grid::COUNT as u32,
                    draw_count: 1,
                    draws: [
                        RetainedDrawRange {
                            first_index: 0,
                            index_count: 44,
                        },
                        RetainedDrawRange::default(),
                        RetainedDrawRange::default(),
                        RetainedDrawRange::default(),
                    ],
                    ..RetainedFrameSubmitV3::default()
                },
            )
            .map_err(|code| CubeError::Vgpu("frame-submit", code))?;
        self.device
            .wait(self.queue, point.value)
            .map_err(|code| CubeError::Vgpu("timeline-wait", code))?;
        self.frame
            .publish(Damage::full(width, height))
            .map_err(|error| CubeError::Ui4("frame-publish", error))?;
        self.previous_view_projection = camera.view_projection;
        Ok(())
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

fn default_camera() -> Camera {
    let position = [0.0, 0.0, -7.5];
    Camera {
        position,
        rotation: look_at_camera_rotation(position, [0.0; 3], [0.0, -1.0, 0.0]),
        projection: Projection::Perspective {
            yfov: core::f32::consts::FRAC_PI_3,
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
