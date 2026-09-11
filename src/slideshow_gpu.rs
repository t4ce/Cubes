//! One ordinary retained PBR draw. No cube seeds, HS expansion or CPU readback.
use crate::{network::Slide, slideshow};
use trueos::vgpu::*;

pub struct Wall {
    device: Device,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    slide: Slide,
}
impl Wall {
    pub fn new(device: Device, slide: Slide) -> Result<Self, i32> {
        let vertex_bytes = slideshow::vertices();
        let index_bytes = slideshow::indices();
        let vertices = device.create_buffer(
            vertex_bytes.len(),
            BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX,
        )?;
        let indices = match device.create_buffer(
            index_bytes.len(),
            BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX,
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = device.destroy_buffer(vertices);
                return Err(error);
            }
        };
        let mesh = (|| {
            if device.write_buffer(vertices, 0, &vertex_bytes)? != vertex_bytes.len()
                || device.write_buffer(indices, 0, &index_bytes)? != index_bytes.len()
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
                slide,
            }),
            Err(error) => {
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                Err(error)
            }
        }
    }
    /// Called between completed frames. The incoming texture is already resident;
    /// the old texture remains owned until this atomic scene-thread replacement.
    pub fn replace(&mut self, slide: Slide) {
        self.slide = slide;
    }
    pub fn render(
        &self,
        queue: Queue,
        surface: Ui4Surface,
        camera: RetainedCamera,
        height: u32,
    ) -> Result<(), i32> {
        let point = self.device.submit_retained_frame_v2(
            queue,
            surface,
            self.mesh,
            self.vertices,
            self.indices,
            frame(
                camera,
                height,
                [
                    self.slide.texture.id().raw(),
                    0,
                    0,
                    self.slide.bevel.occlusion.id().raw(),
                    self.slide.bevel.normal.id().raw(),
                ],
            ),
        )?;
        self.device.wait(queue, point.value)
    }
}
impl Drop for Wall {
    fn drop(&mut self) {
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}

fn frame(camera: RetainedCamera, height: u32, textures: [u64; 5]) -> RetainedFrameSubmitV2 {
    let relief = slideshow::relief(
        [
            camera.position_near[0],
            camera.position_near[1],
            camera.position_near[2],
        ],
        camera.projection[5],
        height,
    );
    let mut frame = RetainedFrameSubmit {
        camera,
        seed_count: 1,
        material: RetainedMaterial {
            textures,
            ..RetainedMaterial::default()
        },
        clear_rgba8_srgb: 0,
        ..RetainedFrameSubmit::default()
    };
    frame.seeds[0] = RetainedTransformSeed {
        scale: [1.; 3],
        rotation: [0., 0., 0., 1.],
        local_radius: slideshow::DISTANCE + 2. * slideshow::HALF_EXTENT,
        ..RetainedTransformSeed::default()
    };
    RetainedFrameSubmitV2 {
        frame,
        material_parameters: RetainedMaterialParameters {
            normal_scale: relief,
            occlusion_strength: relief,
            metallic_factor: 0.15,
            roughness_factor: 0.34,
            flags: RETAINED_MATERIAL_FLAG_DOUBLE_SIDED,
            ..RetainedMaterialParameters::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_changes_only_material_and_retains_live_camera_lighting() {
        let mut camera = RetainedCamera::default();
        camera.position_near = [0., 0., -220., 0.01];
        camera.projection[5] = 2.63;
        let first = frame(camera, 441, [11, 0, 0, 22, 33]);
        let second = frame(camera, 441, [44, 0, 0, 22, 33]);
        assert_eq!(first.frame.seed_count, 1);
        assert_eq!(first.frame.static_draw_count, 0);
        assert_eq!(first.frame.seeds, second.frame.seeds);
        assert_eq!(second.frame.material.textures, [44, 0, 0, 22, 33]);
        assert_eq!(first.frame.camera, camera);
        assert_eq!(first.material_parameters.normal_scale, 1.);
        assert_eq!(first.material_parameters.occlusion_strength, 1.);
        camera.position_near[2] = 0.;
        assert_eq!(
            frame(camera, 441, [44, 0, 0, 22, 33])
                .material_parameters
                .normal_scale,
            0.
        );
    }
}
