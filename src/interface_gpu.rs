//! Small tilted material panel. Texture work runs off the scene/input thread.
use crate::cube_interface::{self, Demo};
use alloc::{sync::Arc, vec::Vec};
use std::sync::Mutex;
use trueos::{vgpu::*, vmedia};
use trueos_picasso::cam::{Camera, Projection, Quaternion};

pub fn rotation() -> Quaternion {
    Quaternion::from_axis_angle([0., 0., 1.], -0.025)
        * Quaternion::from_axis_angle([0., 1., 0.], -0.12)
        * Quaternion::from_axis_angle([1., 0., 0.], 0.065)
}
pub fn camera(width: u32, height: u32, columns: usize, rows: usize, tier: u8) -> Camera {
    let half_x = columns as f32 * 0.02 * tier as f32;
    let half_y = rows as f32 * 0.02 * tier as f32;
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let distance = (half_y.max(half_x / aspect) + half_x * 0.14 + half_y * 0.09)
        / libm::tanf(core::f32::consts::FRAC_PI_6)
        * 1.18;
    Camera {
        position: [0., 0., distance],
        rotation: Quaternion::IDENTITY,
        projection: Projection::Perspective {
            yfov: core::f32::consts::FRAC_PI_3,
            aspect_ratio: None,
            znear: 0.01,
            zfar: Some(distance + 100.),
        },
    }
}
pub fn hit(
    origin: [f32; 3],
    direction: [f32; 3],
    columns: usize,
    rows: usize,
    tier: u8,
) -> Option<[f32; 2]> {
    let q = rotation().0;
    let inverse = Quaternion([-q[0], -q[1], -q[2], q[3]]);
    let o = inverse.rotate(origin);
    let d = inverse.rotate(direction);
    if d[2].abs() < 1e-6 {
        return None;
    }
    let t = -o[2] / d[2];
    if t < 0. {
        return None;
    }
    let cell = 0.04 * tier as f32;
    Some([
        (o[0] + t * d[0]) / cell + columns as f32 * 0.5,
        rows as f32 * 0.5 - (o[1] + t * d[1]) / cell,
    ])
}

/// Adapt the logical picking camera to retained PBR's native position contract.
/// Its Naga-built VS negates clip Y before the SF viewport's negative Y scale
/// (TRUEOS src/intel/render/picasso_vue_compare.rs and pipeline.rs). Compensate
/// here so uploaded UVs, bevel normals and CPU picking all keep their orientation.
fn render_camera(mut camera: RetainedCamera) -> RetainedCamera {
    // Left-multiply by the clip-space Y reflection in column-major storage.
    for matrix in [
        &mut camera.projection,
        &mut camera.view_projection,
        &mut camera.previous_view_projection,
    ] {
        for column in 0..4 {
            matrix[column * 4 + 1] = -matrix[column * 4 + 1];
        }
    }
    // Inverse(VP') = inverse(VP) * reflection: negate its Y column.
    for value in &mut camera.inverse_view_projection[4..8] {
        *value = -*value;
    }
    camera
}
struct Textures {
    page: usize,
    color: vmedia::RetainedTexture,
    normal: vmedia::RetainedTexture,
}
struct Work {
    busy: bool,
    ready: Option<Result<Textures, i32>>,
}
pub struct Renderer {
    device: Device,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    textures: Option<Textures>,
    requested: u64,
    work: Arc<Mutex<Work>>,
}
impl Renderer {
    pub fn new(device: Device) -> Result<Self, i32> {
        let data: [[f32; 12]; 4] = [
            [-1., 1., 0., 0., 0., 1., 0., 0., 1., 0., 0., -1.],
            [-1., -1., 0., 0., 0., 1., 0., 1., 1., 0., 0., -1.],
            [1., -1., 0., 0., 0., 1., 1., 1., 1., 0., 0., -1.],
            [1., 1., 0., 0., 0., 1., 1., 0., 1., 0., 0., -1.],
        ];
        let vb: Vec<_> = data
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        // The render-camera reflection reverses winding. Preserve the native
        // clockwise front face so double-sided PBR keeps front-facing normals.
        let ib: Vec<_> = [0u32, 2, 1, 0, 3, 2]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let vertices =
            device.create_buffer(vb.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX)?;
        let indices =
            match device.create_buffer(ib.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX) {
                Ok(v) => v,
                Err(e) => {
                    let _ = device.destroy_buffer(vertices);
                    return Err(e);
                }
            };
        let mesh = (|| {
            if device.write_buffer(vertices, 0, &vb)? != vb.len()
                || device.write_buffer(indices, 0, &ib)? != ib.len()
            {
                return Err(ERR_IO);
            }
            device.create_retained_mesh(
                vertices,
                indices,
                RetainedMeshDescriptor {
                    vertex_count: 4,
                    index_count: 6,
                    vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL_UV_TANGENT,
                    topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST | RETAINED_MESH_FLAG_DOUBLE_SIDED,
                    ..RetainedMeshDescriptor::default()
                },
            )
        })();
        match mesh {
            Ok(mesh) => Ok(Self {
                device,
                vertices,
                indices,
                mesh,
                textures: None,
                requested: 0,
                work: Arc::new(Mutex::new(Work {
                    busy: false,
                    ready: None,
                })),
            }),
            Err(e) => {
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                Err(e)
            }
        }
    }
    pub fn update(&mut self, demo: &Demo) -> Result<(), i32> {
        let ready = self.work.lock().unwrap().ready.take();
        if let Some(result) = ready {
            let textures = result.map_err(|error| {
                self.requested = 0;
                error
            })?;
            if textures.page == demo.page {
                self.textures = Some(textures);
            }
        }
        let mut work = self.work.lock().unwrap();
        if work.busy || self.requested == demo.revision {
            return Ok(());
        }
        let canvas = demo.raster();
        let page = demo.page;
        let revision = demo.revision;
        work.busy = true;
        drop(work);
        self.requested = revision;
        let shared = self.work.clone();
        let device = self.device;
        if trueos::worker::spawn(move || {
            let (color, normal) = cube_interface::textures(&canvas);
            let result = trueos::async_fs::block_on(async {
                let color =
                    vmedia::decode_retained(device, vmedia::ImageFormat::Bmp, &color).await?;
                let normal =
                    vmedia::decode_retained(device, vmedia::ImageFormat::Bmp, &normal).await?;
                Ok(Textures {
                    page,
                    color,
                    normal,
                })
            });
            let mut work = shared.lock().unwrap();
            work.ready = Some(result);
            work.busy = false;
        })
        .is_err()
        {
            self.work.lock().unwrap().busy = false;
            self.requested = 0;
            return Err(ERR_IO);
        }
        Ok(())
    }
    pub fn ready(&self, page: usize) -> bool {
        self.textures.as_ref().is_some_and(|t| t.page == page)
    }
    /// Retained-image publication and draws share the device's Picasso setup
    /// lease. Keep the published UI4 front buffer while the worker owns it.
    pub fn uploading(&self) -> bool {
        self.work.lock().unwrap().busy
    }
    pub fn can_render(&self, page: usize) -> bool {
        self.ready(page) && !self.uploading()
    }
    /// `false` means no frame was submitted: the surface guard discarded its
    /// write lease, so the caller must skip publish and retry on a later tick.
    pub fn render(
        &self,
        queue: Queue,
        surface: Ui4Surface,
        demo: &Demo,
        camera: RetainedCamera,
    ) -> Result<bool, i32> {
        if !self.can_render(demo.page) {
            return Ok(false);
        }
        let Some(textures) = self.textures.as_ref().filter(|t| t.page == demo.page) else {
            return Ok(false);
        };
        let tier = cube_interface::definition(demo.page).tier as f32;
        let scale = [
            demo.layout.width as f32 * 0.02 * tier,
            demo.layout.height as f32 * 0.02 * tier,
            1.,
        ];
        let mut frame = RetainedFrameSubmit {
            camera: render_camera(camera),
            seed_count: 1,
            clear_rgba8_srgb: 0,
            material: RetainedMaterial {
                textures: [
                    textures.color.id().raw(),
                    0,
                    0,
                    0,
                    textures.normal.id().raw(),
                ],
                ..RetainedMaterial::default()
            },
            ..RetainedFrameSubmit::default()
        };
        frame.seeds[0] = RetainedTransformSeed {
            scale,
            rotation: rotation().0,
            local_radius: 1.5,
            ..RetainedTransformSeed::default()
        };
        let point = match self.device.submit_retained_frame_v2(
            queue,
            surface,
            self.mesh,
            self.vertices,
            self.indices,
            RetainedFrameSubmitV2 {
                frame,
                material_parameters: RetainedMaterialParameters {
                    metallic_factor: 0.,
                    roughness_factor: 0.7,
                    normal_scale: 0.7,
                    flags: RETAINED_MATERIAL_FLAG_DOUBLE_SIDED,
                    ..RetainedMaterialParameters::default()
                },
            },
        ) {
            Ok(point) => point,
            Err(ERR_BUSY) => return Ok(false),
            Err(error) => return Err(error),
        };
        // Submission retires synchronously today. A wait failure after an
        // accepted submission is not an unsubmitted frame and stays an error.
        self.device.wait(queue, point.value)?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../tools/interface_gpu_regression.rs"]
mod regression_tests;
impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tilted_panel_rays_return_widget_coordinates() {
        for (width, height) in [(784, 441), (441, 784), (1920, 1080)] {
            let camera = camera(width, height, 150, 170, 1);
            for p in [[0., 0.], [75., 85.], [140., 160.]] {
                let world = rotation().rotate([(p[0] - 75.) * 0.04, (85. - p[1]) * 0.04, 0.]);
                let direction = core::array::from_fn(|a| world[a] - camera.position[a]);
                let actual = hit(camera.position, direction, 150, 170, 1).unwrap();
                assert!((actual[0] - p[0]).abs() < 0.0001 && (actual[1] - p[1]).abs() < 0.0001);
            }
        }
        assert!(hit([0., 0., 3.], [0., 0., 1.], 150, 170, 1).is_none());
    }
}
