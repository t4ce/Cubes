//! Six beveled image slabs, one atlas, one immutable retained PBR draw.
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
        let geometry = slideshow::geometry(slide.layout);
        let vertex_bytes = geometry.vertex_bytes();
        let index_bytes = geometry.index_bytes();
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
                    vertex_count: geometry.vertices.len() as u32,
                    index_count: geometry.indices.len() as u32,
                    vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL_UV_TANGENT,
                    topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
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
    pub fn replace(&mut self, slide: Slide) -> Result<(), i32> {
        if self.slide.layout == slide.layout { self.slide = slide; }
        else { *self = Self::new(self.device, slide)?; }
        Ok(())
    }
    pub fn layout(&self) -> slideshow::contract::Layout { self.slide.layout }
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
                    0,
                    0,
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

fn frame(camera: RetainedCamera, _height: u32, textures: [u64; 5]) -> RetainedFrameSubmitV2 {
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
            normal_scale: 0.,
            occlusion_strength: 0.,
            metallic_factor: 0.15,
            roughness_factor: 0.34,
            flags: RETAINED_MATERIAL_FLAG_NEAREST,
            ..RetainedMaterialParameters::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gallery_uses_one_draw_one_atlas_and_nearest_filtering() {
        let camera = RetainedCamera::default();
        let frame = frame(camera, 441, [11, 0, 0, 0, 0]);
        assert_eq!(frame.frame.seed_count, 1);
        assert_eq!(frame.frame.static_draw_count, 0);
        assert_eq!(frame.frame.material.textures, [11,0,0,0,0]);
        assert_eq!(frame.frame.camera, camera);
        assert_eq!(frame.material_parameters.flags, RETAINED_MATERIAL_FLAG_NEAREST);
        assert_eq!(frame.material_parameters.normal_scale, 0.);
        assert_eq!(frame.material_parameters.occlusion_strength, 0.);
    }
}
