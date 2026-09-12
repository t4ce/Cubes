//! Textured gallery plus baked cube instances sharing one depth-tested frame.
use alloc::vec::Vec;
use crate::{network::Slide, slideshow};
use trueos::vgpu::*;

pub struct Wall {
    device: Device,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    slide: Slide,
    cubes: CubeInstances,
}
impl Wall {
    pub fn new(device: Device, slide: Slide) -> Result<Self, i32> {
        let cubes = CubeInstances::new(device)?;
        let geometry = slideshow::geometry(slide.layout);
        let (vertices, indices, mesh) = upload(device, &geometry)?;
        Ok(Self { device, vertices, indices, mesh, slide, cubes })
    }
    pub fn replace_holy(&mut self, frame: crate::network::HolyFrame) -> Result<(), i32> {
        if frame.session != self.slide.session || frame.gallery_revision != self.slide.revision {
            return Ok(());
        }
        self.cubes.replace(&frame.cubes, &self.slide.palette)
    }
    pub fn gallery_revision(&self) -> u32 { self.slide.revision }
    pub fn replace(&mut self, slide: Slide) -> Result<(), i32> {
        if self.slide.layout == slide.layout {
            if self.slide.revision != slide.revision || self.slide.session != slide.session {
                self.cubes.replace(&[], &[])?;
                self.cubes.asset.reveal.reset();
            }
            self.slide = slide;
        } else { *self = Self::new(self.device, slide)?; }
        Ok(())
    }
    pub fn layout(&self) -> slideshow::contract::Layout { self.slide.layout }
    pub fn render(
        &mut self, queue: Queue, surface: Ui4Surface, camera: RetainedCamera, height: u32, now: u64, terrain: &[RetainedTransformSeed],
    ) -> Result<(), i32> {
        self.cubes.animate(now, terrain)?;
        let point = self.device.submit_retained_frame_v4(
            queue, surface, self.mesh, self.cubes.mesh, self.vertices, self.indices,
            RetainedFrameSubmitV4 {
                frame: frame(camera, height, [self.slide.texture.id().raw(), 0, 0, 0, 0]),
                cubes: RetainedCubeDraw {
                    seed_buffer: self.cubes.seeds[self.cubes.active].raw(),
                    seed_count: self.cubes.count,
                    ..RetainedCubeDraw::default()
                },
            },
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

const CENTER_COUNT: usize = 27;
const MAX_CUBES: usize = MAX_RETAINED_SCENE_INSTANCES;
pub const TERRAIN_BUDGET: usize = MAX_CUBES-CENTER_COUNT-48*48;
const SEED_BYTES: usize = 64;
/// Immutable 44-patch topology. Updates only upload TRS/color seeds.
struct CubeInstances {
    device: Device,
    vertices: Buffer,
    indices: Buffer,
    mesh: RetainedMesh,
    seeds: [Buffer;2],
    active: usize,
    count: u32,
    asset: AnimatedAsset,
}
impl CubeInstances {
    fn new(device: Device) -> Result<Self, i32> {
        let (vertices, indices, mesh) = upload_mesh(device, &[0;12], &[0;44*4],
            RetainedMeshDescriptor {
                vertex_count: 1, index_count: 44,
                vertex_layout: RETAINED_VERTEX_LAYOUT_CUBE_PATCH_SEED,
                topology: RETAINED_TOPOLOGY_CUBE_PATCHLIST_1 | RETAINED_MESH_FLAG_DOUBLE_SIDED,
                ..RetainedMeshDescriptor::default()
            })?;
        let buffers = (|| {
            let first = device.create_buffer(MAX_CUBES*SEED_BYTES,
                BUFFER_USAGE_MAP_READ | BUFFER_USAGE_MAP_WRITE)?;
            match device.create_buffer(MAX_CUBES*SEED_BYTES,
                BUFFER_USAGE_MAP_READ | BUFFER_USAGE_MAP_WRITE) {
                Ok(second) => Ok([first,second]),
                Err(error) => { let _ = device.destroy_buffer(first); Err(error) }
            }
        })();
        let seeds = match buffers {
            Ok(seeds) => seeds,
            Err(error) => {
                let _ = device.destroy_retained_mesh(mesh);
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                return Err(error);
            }
        };
        let mut cubes = Self { device, vertices, indices, mesh, seeds, active:0, count:0,
            asset: AnimatedAsset::new() };
        cubes.replace(&[], &[])?;
        cubes.animate(0, &[])?;
        Ok(cubes)
    }
    fn replace(&mut self, pixels: &[cubes_protocol::holy::Pixel], palette: &[u16]) -> Result<(), i32> {
        self.asset.replace(pixels, palette)
    }
    fn animate(&mut self, now: u64, terrain: &[RetainedTransformSeed]) -> Result<(), i32> {
        if terrain.len() > TERRAIN_BUDGET { return Err(ERR_UNSUPPORTED); }
        let mut seeds = self.asset.frame(now);
        for seed in terrain {
            let mut seed = *seed;
            seed.flags = (seed.flags & 0xffff) | ((seeds.len() as u32)<<16);
            seeds.push(seed);
        }
        let bytes = seed_bytes(&seeds);
        // Publish only a complete upload; failure leaves the displayed frame intact.
        let next = 1-self.active;
        if self.device.write_buffer(self.seeds[next], 0, &bytes)? != bytes.len() {
            return Err(ERR_IO);
        }
        self.active = next;
        self.count = seeds.len() as u32;
        Ok(())
    }
}
/// One persistent grid asset. Sparse frame ordering is never an instance identity.
struct AnimatedAsset {
    authored: Vec<RetainedTransformSeed>,
    ids: Vec<usize>,
    reveal: crate::reveal::Reveal,
}
impl AnimatedAsset {
    fn new() -> Self {
        Self { authored: Vec::new(), ids: Vec::new(), reveal: crate::reveal::Reveal::new() }
    }
    fn replace(&mut self, pixels: &[cubes_protocol::holy::Pixel], palette: &[u16]) -> Result<(), i32> {
        let authored = cube_seeds(pixels, palette)?;
        let mut occupied = [false; 48*48];
        let mut ids = Vec::with_capacity(pixels.len());
        for pixel in pixels {
            let id = pixel.y as usize*48 + pixel.x as usize;
            if occupied[id] { return Err(ERR_UNSUPPORTED); }
            occupied[id] = true;
            ids.push(id);
        }
        self.authored = authored;
        self.ids = ids;
        Ok(())
    }
    fn frame(&mut self, now: u64) -> Vec<RetainedTransformSeed> {
        // Share placement's admission delay, rate cap, growth curve and rearm policy.
        // Tick at display cadence, independently of the server's 100 ms updates.
        self.reveal.begin_frame(now, 48*48);
        let mut visible = self.authored[..CENTER_COUNT].to_vec();
        for (&id, seed) in self.ids.iter().zip(&self.authored[CENTER_COUNT..]) {
            if self.reveal.admit(id) {
                let mut seed = *seed;
                let growth = self.reveal.growth_scale(id);
                seed.scale = seed.scale.map(|scale| scale*growth);
                if growth > 0. {
                    // flags encode compact GPU output slots, not authored IDs.
                    // Reveal keeps the persistent grid identity separately.
                    seed.flags = (seed.flags & 0xffff) | ((visible.len() as u32)<<16);
                    visible.push(seed);
                }
            }
        }
        self.reveal.end_frame();
        visible
    }
}
impl Drop for CubeInstances {
    fn drop(&mut self) {
        for buffer in self.seeds { let _ = self.device.destroy_buffer(buffer); }
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}
fn cube_seeds(pixels: &[cubes_protocol::holy::Pixel], palette: &[u16])
    -> Result<Vec<RetainedTransformSeed>, i32>
{
    if pixels.len() > 48*48 { return Err(ERR_UNSUPPORTED); }
    let mut seeds = Vec::with_capacity(CENTER_COUNT+pixels.len());
    let mut push = |translation: [f32;3], half: f32, color: u16| {
        seeds.push(RetainedTransformSeed {
            translation, previous_translation: translation, scale:[half;3],
            // Same orientation and bounding radius as placed cube assets.
            rotation:[1.,0.,0.,0.], local_radius:1.74,
            flags: color as u32 | ((seeds.len() as u32)<<16),
            ..RetainedTransformSeed::default()
        });
    };
    for x in -1..=1 { for y in -1..=1 { for z in -1..=1 {
        push([x,y,z].map(|v| v as f32*slideshow::CENTER_CUBE_SIDE),
            slideshow::CENTER_CUBE_SIDE*0.5, 0xffff);
    } } }
    for pixel in pixels {
        if pixel.x >= 48 || pixel.y >= 48 { return Err(ERR_UNSUPPORTED); }
        let color = *palette.get(pixel.palette as usize).ok_or(ERR_UNSUPPORTED)?;
        if color & 0x8000 == 0 { return Err(ERR_UNSUPPORTED); }
        push([
            (pixel.x as f32+0.5-24.)*slideshow::contract::C1,
            slideshow::CENTER_HALF_EXTENT+(48.-pixel.y as f32-0.5)*slideshow::contract::C1,
            0.,
        ], slideshow::contract::C1*0.5, color);
    }
    Ok(seeds)
}
fn seed_bytes(seeds: &[RetainedTransformSeed]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(seeds.len()*SEED_BYTES);
    for seed in seeds {
        for value in seed.translation.into_iter().chain(seed.scale).chain(seed.rotation)
            .chain([seed.local_radius]).chain(seed.previous_translation) {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&seed.draw_group.to_le_bytes());
        bytes.extend_from_slice(&seed.flags.to_le_bytes());
    }
    bytes
}

fn upload(device: Device, geometry: &slideshow::Geometry) -> Result<(Buffer, Buffer, RetainedMesh), i32> {
    upload_mesh(device, geometry.vertex_bytes(), geometry.index_bytes(),
        RetainedMeshDescriptor {
            vertex_count: geometry.vertices.len() as u32,
            index_count: geometry.indices.len() as u32,
            vertex_layout: RETAINED_VERTEX_LAYOUT_POS_NORMAL_UV_TANGENT,
            topology: PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            ..RetainedMeshDescriptor::default()
        })
}
fn upload_mesh(device: Device, vertex_bytes: &[u8], index_bytes: &[u8],
    descriptor: RetainedMeshDescriptor,
) -> Result<(Buffer, Buffer, RetainedMesh), i32> {
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
                descriptor,
            )
        })();
        match mesh {
            Ok(mesh) => Ok((vertices, indices, mesh)),
            Err(error) => {
                let _ = device.destroy_buffer(indices);
                let _ = device.destroy_buffer(vertices);
                Err(error)
            }
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
        local_radius: slideshow::RADIUS,
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
    fn pixels(points: &[(u8,u8,u8)]) -> Vec<cubes_protocol::holy::Pixel> {
        points.iter().map(|&(x,y,palette)| cubes_protocol::holy::Pixel {x,y,palette}).collect()
    }
    #[test]
    fn continuing_cells_keep_growth_across_sparse_reordering_and_color_changes() {
        let mut asset = AnimatedAsset::new();
        asset.replace(&pixels(&[(3,4,0)]), &[0xffff,0x801f]).unwrap();
        assert_eq!(asset.frame(0).len(), CENTER_COUNT);
        assert_eq!(asset.frame(333).len(), CENTER_COUNT);
        let before = asset.frame(433)[CENTER_COUNT];
        assert!(before.scale[0] > 0. && before.scale[0] < 0.1);
        asset.replace(&pixels(&[(1,2,0),(3,4,1)]), &[0xffff,0x801f]).unwrap();
        let after = asset.frame(433);
        assert_eq!(after.len(), CENTER_COUNT+1);
        assert_eq!(after[CENTER_COUNT].scale, before.scale);
        assert_eq!(after[CENTER_COUNT].flags >> 16, before.flags >> 16);
        assert_eq!(after[CENTER_COUNT].flags & 0xffff, 0x801f);
        assert!(asset.replace(&pixels(&[(3,4,0),(3,4,0)]), &[0xffff]).is_err());
        assert_eq!(asset.frame(433)[CENTER_COUNT].flags, after[CENTER_COUNT].flags);
    }
    #[test]
    fn transient_pixels_follow_placement_delay_and_absence_rearms() {
        let mut asset = AnimatedAsset::new();
        asset.replace(&pixels(&[(0,0,0)]), &[0xffff]).unwrap();
        assert_eq!(asset.frame(0).len(), CENTER_COUNT);
        asset.replace(&[], &[]).unwrap();
        assert_eq!(asset.frame(100).len(), CENTER_COUNT);
        asset.replace(&pixels(&[(0,0,0)]), &[0xffff]).unwrap();
        assert_eq!(asset.frame(200).len(), CENTER_COUNT);
        assert_eq!(asset.frame(533).len(), CENTER_COUNT);
        assert_eq!(asset.frame(1233)[CENTER_COUNT].scale, [0.1;3]);
        asset.replace(&[], &[]).unwrap();
        assert_eq!(asset.frame(1300).len(), CENTER_COUNT);
        asset.replace(&pixels(&[(0,0,0)]), &[0xffff]).unwrap();
        assert_eq!(asset.frame(1600).len(), CENTER_COUNT);
    }
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

#[cfg(test)]
#[path = "../tools/slideshow_gpu_regression.rs"]
mod regression;
