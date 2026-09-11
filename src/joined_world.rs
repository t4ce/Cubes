//! First Key-5 world rendered as one retained, indexed exposed-face mesh.
//! Walking, portals and targeting continue to use the authored `.cubes` data.
use alloc::vec::Vec;
use trueos::{async_fs, vgpu::*, vmedia};

const VERTICES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/joined_world_01_vertices.bin"));
const INDICES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/joined_world_01_indices.bin"));
const WORLD: &[u8] = include_bytes!("../Cube/lvl27/world_01_sky.cubes");
const VERTEX_STRIDE: usize = 48;
const TILE: usize = 96;
const EDGE: usize = 7; // 7/96 = 7.29%, one third wider than the 5.5% demo seam.

pub struct Renderer {
    device: Device,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    atlas: vmedia::RetainedTexture,
}

impl Renderer {
    pub fn new(device: Device) -> Result<Self, i32> {
        if VERTICES.is_empty()
            || INDICES.is_empty()
            || !VERTICES.len().is_multiple_of(VERTEX_STRIDE)
            || !INDICES.len().is_multiple_of(4)
        {
            return Err(ERR_UNSUPPORTED);
        }
        let vertices =
            device.create_buffer(VERTICES.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX)?;
        let indices = match device
            .create_buffer(INDICES.len(), BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX)
        {
            Ok(value) => value,
            Err(error) => {
                let _ = device.destroy_buffer(vertices);
                return Err(error);
            }
        };
        let mesh = (|| {
            if device.write_buffer(vertices, 0, VERTICES)? != VERTICES.len()
                || device.write_buffer(indices, 0, INDICES)? != INDICES.len()
            {
                return Err(ERR_IO);
            }
            device.create_retained_mesh(
                vertices,
                indices,
                RetainedMeshDescriptor {
                    vertex_count: (VERTICES.len() / VERTEX_STRIDE) as u32,
                    index_count: (INDICES.len() / 4) as u32,
                    vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL_UV_TANGENT,
                    topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST | RETAINED_MESH_FLAG_DOUBLE_SIDED,
                    ..RetainedMeshDescriptor::default()
                },
            )
        })();
        let mesh = match mesh {
            Ok(value) => value,
            Err(error) => {
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                return Err(error);
            }
        };
        let atlas = match async_fs::block_on(vmedia::decode_retained(
            device,
            vmedia::ImageFormat::Bmp,
            &palette_atlas(),
        )) {
            Ok(value) => value,
            Err(error) => {
                let _ = device.destroy_retained_mesh(mesh);
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                return Err(error);
            }
        };
        Ok(Self {
            device,
            vertices,
            indices,
            mesh,
            atlas,
        })
    }

    pub fn quads(&self) -> usize {
        INDICES.len() / 4 / 6
    }

    pub fn bytes(&self) -> usize {
        VERTICES.len() + INDICES.len()
    }

    pub fn render(
        &self,
        queue: Queue,
        surface: Ui4Surface,
        camera: RetainedCamera,
    ) -> Result<(), i32> {
        let mut frame = RetainedFrameSubmit {
            camera: render_camera(camera),
            seed_count: 1,
            clear_rgba8_srgb: 0,
            material: RetainedMaterial {
                textures: [self.atlas.id().raw(), 0, 0, 0, 0],
                ..RetainedMaterial::default()
            },
            ..RetainedFrameSubmit::default()
        };
        frame.seeds[0] = RetainedTransformSeed {
            scale: [1.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            local_radius: 512.0,
            ..RetainedTransformSeed::default()
        };
        let point = self.device.submit_retained_frame_v2(
            queue,
            surface,
            self.mesh,
            self.vertices,
            self.indices,
            RetainedFrameSubmitV2 {
                frame,
                material_parameters: RetainedMaterialParameters {
                    metallic_factor: 0.0,
                    roughness_factor: 0.82,
                    flags: RETAINED_MATERIAL_FLAG_DOUBLE_SIDED,
                    ..RetainedMaterialParameters::default()
                },
            },
        )?;
        self.device.wait(queue, point.value)
    }
}

/// PBR and the procedural cube shader use opposite clip-Y conventions. Keep
/// the joined mesh aligned with the walker/picker camera used by Key 5.
fn render_camera(mut camera: RetainedCamera) -> RetainedCamera {
    for matrix in [
        &mut camera.projection,
        &mut camera.view_projection,
        &mut camera.previous_view_projection,
    ] {
        for column in 0..4 {
            matrix[column * 4 + 1] = -matrix[column * 4 + 1];
        }
    }
    for value in &mut camera.inverse_view_projection[4..8] {
        *value = -*value;
    }
    camera
}

/// Exact opaque palette texels with a hard, unfiltered-looking border. The
/// seventh source entry is the neutral portal frame; terrain remains the six
/// shared world colours.
fn palette_atlas() -> Vec<u8> {
    let count = WORLD[10] as usize;
    let palette = &WORLD[16..16 + count * 4];
    let mut rgb = vec![0; count * TILE * TILE * 3];
    for material in 0..count {
        let color = &palette[material * 4..material * 4 + 4];
        assert_eq!(color[3], 255);
        for y in 0..TILE {
            for x in 0..TILE {
                let edge = x < EDGE || y < EDGE || x >= TILE - EDGE || y >= TILE - EDGE;
                let pixel = (y * count * TILE + material * TILE + x) * 3;
                for channel in 0..3 {
                    rgb[pixel + channel] = if edge {
                        ((color[channel] as u16 * 66 + 50) / 100) as u8
                    } else {
                        color[channel]
                    };
                }
            }
        }
    }
    crate::cube_interface::bmp(count * TILE, TILE, &rgb)
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}

#[cfg(test)]
#[path = "../tools/build_joined_world.rs"]
mod build_mesh_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_is_opaque_exact_palette_with_hard_seven_pixel_edges() {
        let bmp = palette_atlas();
        let count = WORLD[10] as usize;
        assert_eq!(
            u32::from_le_bytes(bmp[18..22].try_into().unwrap()) as usize,
            count * TILE
        );
        assert_eq!(
            u32::from_le_bytes(bmp[22..26].try_into().unwrap()) as usize,
            TILE
        );
        let rgba = &bmp[54..];
        let pixel = |x: usize, y: usize| &rgba[(y * count * TILE + x) * 4..][..4];
        for material in 0..count {
            let source = &WORLD[16 + material * 4..20 + material * 4];
            let center = pixel(material * TILE + TILE / 2, TILE / 2);
            assert_eq!(center, [source[2], source[1], source[0], 255]);
            let edge = pixel(material * TILE + EDGE - 1, TILE / 2);
            let inside = pixel(material * TILE + EDGE, TILE / 2);
            assert_ne!(edge, inside);
            assert_eq!(inside, center);
        }
    }
}
