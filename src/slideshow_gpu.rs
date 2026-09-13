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
    scene: Option<crate::network::VfxScene>,
}
impl Wall {
    pub fn new(device: Device, slide: Slide) -> Result<Self, i32> {
        let cubes = CubeInstances::new(device)?;
        let geometry = slideshow::geometry(slide.layout);
        let (vertices, indices, mesh) = upload(device, &geometry)?;
        Ok(Self { device, vertices, indices, mesh, slide, cubes, scene: None })
    }
    pub fn replace_vfx(&mut self, scene: crate::network::VfxScene) -> Result<(), i32> {
        if scene.session!=self.slide.session || scene.info.gallery_revision!=self.slide.revision { return Ok(()); }
        for bytes in scene.assets.iter().flatten() {
            if cubes_protocol::vfx::Sequence::parse(bytes).is_none() { return Err(ERR_UNSUPPORTED); }
        }
        self.scene=Some(scene);
        Ok(())
    }
    pub fn spawned(&self) -> [Option<[i16;3]>;cubes_protocol::vfx::TERRAIN_CUBES] {
        self.scene.as_ref().map_or([None;cubes_protocol::vfx::TERRAIN_CUBES], |s| s.terrain())
    }
    pub fn gallery_revision(&self) -> u32 { self.slide.revision }
    pub fn replace(&mut self, slide: Slide) -> Result<(), i32> {
        if self.slide.layout == slide.layout {
            if self.slide.revision != slide.revision || self.slide.session != slide.session {
                self.scene = None;
            }
            self.slide = slide;
        } else { *self = Self::new(self.device, slide)?; }
        Ok(())
    }
    pub fn layout(&self) -> slideshow::contract::Layout { self.slide.layout }
    pub fn render(
        &mut self, queue: Queue, surface: Ui4Surface, camera: RetainedCamera, height: u32, _now: u64, terrain: &[RetainedTransformSeed], overlays: &[RetainedTransformSeed],
    ) -> Result<(), i32> {
        let seeds = scene_seeds(self.scene.as_ref(), &self.slide.palette, &self.slide.world, &camera)?;
        self.cubes.animate(seeds, terrain, overlays)?;
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
pub const OVERLAY_BUDGET: usize = 129;
pub const TERRAIN_BUDGET: usize = MAX_CUBES - CENTER_COUNT
    - cubes_protocol::vfx::INSTANCES * cubes_protocol::vfx::CELLS - cubes_protocol::vfx::TERRAIN_CUBES - OVERLAY_BUDGET;
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
        let mut cubes = Self { device, vertices, indices, mesh, seeds, active:0, count:0 };
        cubes.animate(landmark_seeds(), &[], &[])?;
        Ok(cubes)
    }
    fn animate(&mut self, mut seeds: Vec<RetainedTransformSeed>, terrain: &[RetainedTransformSeed], overlays: &[RetainedTransformSeed]) -> Result<(), i32> {
        if terrain.len()>TERRAIN_BUDGET || overlays.len()>OVERLAY_BUDGET
            || seeds.len()+terrain.len()+overlays.len()>MAX_CUBES { return Err(ERR_UNSUPPORTED); }
        for seed in terrain {
            let mut seed = *seed;
            seed.flags = (seed.flags & 0xffff) | ((seeds.len() as u32)<<16);
            seeds.push(seed);
        }
        for (slot, seed) in overlays.iter().enumerate() {
            let mut seed = *seed;
            seed.draw_group = 1;
            seed.flags = (seed.flags & 0xffff) | ((slot as u32)<<16);
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
impl Drop for CubeInstances {
    fn drop(&mut self) {
        for buffer in self.seeds { let _ = self.device.destroy_buffer(buffer); }
        let _ = self.device.destroy_retained_mesh(self.mesh);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_buffer(self.vertices);
    }
}
fn terrain_seed(anchor: [i16; 3]) -> RetainedTransformSeed {
    let translation = anchor.map(|coordinate| coordinate as f32 * slideshow::contract::C1);
    RetainedTransformSeed {
        translation, previous_translation: translation,
        scale: [slideshow::CENTER_CUBE_SIDE * 0.5; 3], rotation: [1., 0., 0., 0.],
        local_radius: 1.74, flags: 0xffff, ..RetainedTransformSeed::default()
    }
}
fn landmark_seeds() -> Vec<RetainedTransformSeed> {
    let mut seeds=Vec::with_capacity(CENTER_COUNT);
    for x in -1..=1 { for y in -1..=1 { for z in -1..=1 {
        seeds.push(terrain_seed([x*8,y*8,z*8]));
    } } }
    for (i,s) in seeds.iter_mut().enumerate() { s.flags |= (i as u32)<<16; }
    seeds
}

/// View matrix rows are world-space camera right/up/back. A common basis keeps
/// all six planes parallel to the viewport and their original pixel order.
fn billboard(camera: &RetainedCamera) -> ([f32;3],[f32;3],[f32;4]) {
    let m=&camera.view;
    let mut right=[m[0],m[4],m[8]];
    let mut up=[m[1],m[5],m[9]];
    if right.iter().map(|v|v*v).sum::<f32>()<0.5 {
        right=[1.,0.,0.]; up=[0.,1.,0.];
    }
    let back=[right[1]*up[2]-right[2]*up[1],right[2]*up[0]-right[0]*up[2],right[0]*up[1]-right[1]*up[0]];
    let trace=right[0]+up[1]+back[2];
    let q=if trace>0. {
        let s=libm::sqrtf(trace+1.)*2.;
        [(up[2]-back[1])/s,(back[0]-right[2])/s,(right[1]-up[0])/s,0.25*s]
    } else if right[0]>up[1] && right[0]>back[2] {
        let s=libm::sqrtf(1.+right[0]-up[1]-back[2])*2.;
        [0.25*s,(up[0]+right[1])/s,(back[0]+right[2])/s,(up[2]-back[1])/s]
    } else if up[1]>back[2] {
        let s=libm::sqrtf(1.+up[1]-right[0]-back[2])*2.;
        [(up[0]+right[1])/s,0.25*s,(back[1]+up[2])/s,(back[0]-right[2])/s]
    } else {
        let s=libm::sqrtf(1.+back[2]-right[0]-up[1])*2.;
        [(back[0]+right[2])/s,(back[1]+up[2])/s,0.25*s,(right[1]-up[0])/s]
    };
    // Preserve the placed cube mesh's original 180-degree X orientation.
    (right,up,[q[3],q[2],-q[1],-q[0]])
}
fn scene_seeds(scene: Option<&crate::network::VfxScene>, palette: &[u16], world: &[u8],
    camera: &RetainedCamera) -> Result<Vec<RetainedTransformSeed>,i32>
{
    let mut seeds=landmark_seeds();
    let Some(scene)=scene.filter(|s|s.info.event!=0) else { return Ok(seeds); };
    let age=scene.age_ms();
    let (right,up,rotation)=billboard(camera);
    for anchor in scene.info.terrain(age).into_iter().flatten() {
        let mut terrain=terrain_seed(anchor);
        if let Some(rgb)=world.get(16..19) {
            terrain.flags=0x8000|((rgb[0] as u32*31+127)/255)
                |(((rgb[1] as u32*31+127)/255)<<5)|(((rgb[2] as u32*31+127)/255)<<10);
        }
        seeds.push(terrain);
    }
    for (slot,asset) in scene.info.slots.iter().zip(&scene.assets) {
        let Some(frame)=slot.frame(age) else { continue; };
        let Some(asset)=asset else { continue; };
        let sequence=cubes_protocol::vfx::Sequence::parse(asset).ok_or(ERR_UNSUPPORTED)?;
        let base=slot.anchor.map(|v|v as f32*slideshow::contract::C1);
        let pixel_side=slot.pixel_side_c1 as f32*slideshow::contract::C1;
        for run in sequence.lifetimes().filter(|r|r.first<=frame && frame<r.end) {
            let pixel=run.pixel;
            let start=cubes_protocol::vfx::DELAY_MS as u64+run.first as u64*slot.period_ms as u64;
            let lifetime=(run.end-run.first) as u64*slot.period_ms as u64;
            let growth=crate::reveal::lifetime_scale(age.saturating_sub(start),lifetime);
            // Match placed assets' tiny initial seed, avoiding degenerate GPU transforms.
            let scale=(pixel_side*0.5*growth).max(0.00101);
            let x=(pixel.x as f32+0.5-16.)*pixel_side;
            let y=(31.5-pixel.y as f32)*pixel_side;
            let translation=core::array::from_fn(|a| base[a]+if a==1 {0.8} else {0.}
                +right[a]*x+up[a]*y);
            let color=*palette.get(pixel.palette as usize).ok_or(ERR_UNSUPPORTED)?;
            if color&0x8000==0 { return Err(ERR_UNSUPPORTED); }
            seeds.push(RetainedTransformSeed {translation,previous_translation:translation,
                scale:[scale;3],rotation,local_radius:1.74,flags:color as u32,
                ..RetainedTransformSeed::default()});
        }
    }
    for (i,s) in seeds.iter_mut().enumerate() { s.flags=(s.flags&0xffff)|((i as u32)<<16); }
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
    #[test]
    fn billboard_follows_yaw_pitch_and_preserves_cube_rotation_length() {
        let mut camera=RetainedCamera::default();
        for angle in [0.,0.5,1.57,3.14] {
            let (s,c)=(libm::sinf(angle),libm::cosf(angle));
            camera.view=[c,0.,s,0., 0.,1.,0.,0., -s,0.,c,0., 0.,0.,0.,1.];
            let (right,up,q)=billboard(&camera);
            assert_eq!(right,[c,0.,-s]); assert_eq!(up,[0.,1.,0.]);
            assert!((q.iter().map(|v|v*v).sum::<f32>()-1.).abs()<0.00001);
        }
        camera.view=[1.,0.,0.,0., 0.,0.,1.,0., 0.,-1.,0.,0., 0.,0.,0.,1.];
        let (right,up,_)=billboard(&camera);
        assert_eq!(right,[1.,0.,0.]); assert_eq!(up,[0.,0.,-1.]);
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
