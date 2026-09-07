#![no_std]

extern crate alloc;

use trueos::ui4_scene::{Damage, Error as Ui4Error, Frame, output_dimensions};
use trueos::vgpu::{
    BUFFER_USAGE_INDEX, BUFFER_USAGE_MAP_WRITE, BUFFER_USAGE_VERTEX, Buffer, Capabilities, Device,
    Queue, QueueClass, RETAINED_VERTEX_LAYOUT_POS_NORMAL, RetainedFrameSubmit, RetainedMesh,
    RetainedMeshDescriptor, RetainedTransformSeed,
};
use trueos::{
    clock,
    logl::{self, level},
    vsys,
};
use trueos_picasso::Picasso;
use trueos_picasso::cam::{Camera, FlyCam, Projection, Quaternion};

include!(concat!(env!("OUT_DIR"), "/cube_asset.rs"));

const CUBE_SOURCE: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cube/cube.glb"));
const WIDTH: u32 = 784;
const HEIGHT: u32 = 441;
const FLYCAM_SPEED: f32 = 1.5;
const FLYCAM_LOOK_SENSITIVITY: f32 = 0.002;

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
    flycam: FlyCam,
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
    // The GLB and its host-prepared buffers deliberately cross Picasso's
    // public database boundary before becoming vGPU resources.
    let picasso = Picasso::new().map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/cube.glb", CUBE_SOURCE)
        .map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/prepared/vertices", CUBE_VERTICES)
        .map_err(|_| CubeError::Contract)?;
    picasso
        .put_embedded_asset("Cube/prepared/indices", CUBE_INDICES)
        .map_err(|_| CubeError::Contract)?;
    let vertices = picasso
        .embedded_asset("Cube/prepared/vertices")
        .map_err(|_| CubeError::Contract)?
        .ok_or(CubeError::Contract)?;
    let indices = picasso
        .embedded_asset("Cube/prepared/indices")
        .map_err(|_| CubeError::Contract)?
        .ok_or(CubeError::Contract)?;
    if vertices.as_slice() != CUBE_VERTICES || indices.as_slice() != CUBE_INDICES {
        return Err(CubeError::Contract);
    }
    logl::log(
        level::INFO,
        format_args!(
            "Cubes: Picasso database ready source_bytes={} vertices={} indices={}",
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
        if vertex_bytes.len() != CUBE_VERTEX_COUNT as usize * 24
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
                    vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL,
                    topology: trueos::vgpu::PRIMITIVE_TOPOLOGY_TRIANGLE_LIST
                        | trueos::vgpu::RETAINED_MESH_FLAG_DOUBLE_SIDED,
                    ..RetainedMeshDescriptor::default()
                },
            )
            .map_err(|code| CubeError::Vgpu("mesh-create", code))?;
        let camera = default_camera();
        let mut flycam = FlyCam::new(camera, FLYCAM_SPEED);
        flycam.set_look_sensitivity(FLYCAM_LOOK_SENSITIVITY);
        Ok(Self {
            frame,
            device,
            queue,
            vertices,
            indices,
            mesh,
            flycam,
            previous_elapsed_millis: 0,
            previous_view_projection: camera.retained(WIDTH, HEIGHT, [0.0; 16]).view_projection,
        })
    }

    fn render(&mut self, elapsed_millis: u64) -> Result<(), CubeError> {
        let delta_seconds =
            elapsed_millis.saturating_sub(self.previous_elapsed_millis) as f32 * 0.001;
        self.previous_elapsed_millis = elapsed_millis;
        self.flycam
            .step_ui4(&self.frame, delta_seconds)
            .map_err(|error| CubeError::Ui4("flycam-ui4", error))?;
        while let Some(event) = self
            .frame
            .take_pointer_event()
            .map_err(|error| CubeError::Ui4("pointer-event", error))?
        {
            self.flycam.handle_ui4_pointer_event(&event, true);
        }
        let width = self.frame.width();
        let height = self.frame.height();
        let camera = self
            .flycam
            .camera
            .retained(width, height, self.previous_view_projection);
        match self.frame.begin_gpu_frame() {
            Ok(()) => {}
            Err(Ui4Error::Busy) => return Ok(()),
            Err(error) => return Err(CubeError::Ui4("frame-begin", error)),
        }
        let surface = self
            .device
            .acquire_ui4_surface(self.frame.window_id())
            .map_err(|code| CubeError::Vgpu("surface-acquire", code))?;
        let mut seeds =
            [RetainedTransformSeed::default(); trueos::vgpu::MAX_RETAINED_TRANSFORM_SEEDS];
        seeds[0] = RetainedTransformSeed {
            translation: [0.0; 3],
            scale: [1.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            local_radius: 1.0,
            previous_translation: [0.0; 3],
            draw_group: 0,
            flags: 0,
        };
        let point = self
            .device
            .submit_retained_frame(
                self.queue,
                surface,
                self.mesh,
                self.vertices,
                self.indices,
                RetainedFrameSubmit {
                    camera,
                    clear_rgba8_srgb: u32::from_le_bytes([0, 128, 0, 0]),
                    seed_count: 1,
                    seeds,
                    ..RetainedFrameSubmit::default()
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
}

fn write_exact(device: Device, buffer: Buffer, bytes: &[u8]) -> Result<(), i32> {
    (device.write_buffer(buffer, 0, bytes)? == bytes.len())
        .then_some(())
        .ok_or(trueos::vgpu::ERR_IO)
}

fn default_camera() -> Camera {
    let position = [0.0, 0.0, -5.0];
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
