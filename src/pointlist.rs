//! Native XYZ POINT_LIST batches, shared by the migrated PotatoStamps Key1
//! background and Key6's far-field markers. No hull/domain or compute shader.
use alloc::vec::Vec;
use trueos::ui4_scene::{BackgroundLayer, Damage, Error};
use trueos::vgpu::*;

pub const MAX_POINTS: usize = 32768;
pub const PATTERN_MS: u64 = 5000;
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub position: [f32; 3],
    pub color: u32,
    pub width: u8,
}

pub fn pattern(now: u64) -> Vec<Point> {
    use potato_stamps::scene::*;
    let colors = decode_palette_rgba(COLOR_TEXTURE_BYTES).unwrap();
    if (now / PATTERN_MS) % 2 == 0 {
        line_grid_positions().iter().enumerate().map(|(i,p)| Point {
            position: [p.x, p.y, p.z], color: colors[usize::from(i % 3 == 0)], width: 0,
        }).collect()
    } else {
        let rings = quad_strip_ring_positions();
        (0..RING_CIRCLE_COUNT).flat_map(|circle| {
            let rings = &rings;
            (0..RING_CIRCLE_VERTEX_COUNT).map(move |step| {
                let p = rings[(circle / 2) * QUAD_STRIP_RING_VERTICES_PER_RING + step * 2 + circle % 2];
                Point { position: [p.x,p.y,p.z], color: colors[circle], width: POINT_RING_POINT_WIDTHS_PX[circle] }
            })
        }).collect()
    }
}

/// Project only forward, in-viewport marker centres. Source cubes were already
/// conservatively frustum/occlusion tested; never mirror points behind the eye.
pub fn project(cubes: &[crate::orchard::Cube], matrix: &[f32;16], out: &mut Vec<Point>) {
    out.clear();
    for cube in cubes.iter().take(MAX_POINTS) {
        let p: [f32;4] = core::array::from_fn(|row| matrix[row]*cube.center[0]
            + matrix[4+row]*cube.center[1] + matrix[8+row]*cube.center[2] + matrix[12+row]);
        if !p.iter().all(|v| v.is_finite()) || p[3] <= 0. || p[2] < 0. || p[2] > p[3]
            || p[0].abs() > p[3] || p[1].abs() > p[3] { continue; }
        let rgb: [u8;3] = core::array::from_fn(|a| (((cube.flags >> (a*5)) & 31)*255/31) as u8);
        out.push(Point { position: [p[0]/p[3], p[1]/p[3], p[2]/p[3]],
            color: u32::from_le_bytes([rgb[0],rgb[1],rgb[2],255]), width: 2 });
    }
}

pub struct Batch {
    pub vertices: Vec<u8>,
    pub draws: Vec<IndexedBatchDrawV2>,
}
impl Batch {
    pub fn new(points: &mut [Point]) -> Self {
        // Preserve exact colors unless averaging produced more colors than the
        // native 600-draw limit. Only that case uses a bounded 8x8x8 color grid.
        points.sort_unstable_by_key(|p| (p.width,p.color));
        let groups = usize::from(!points.is_empty()) + points.windows(2)
            .filter(|p| (p[0].width,p[0].color) != (p[1].width,p[1].color)).count();
        if groups > MAX_INDEXED_BATCH_V2_DRAWS {
            for p in points.iter_mut() {
                let rgb = p.color.to_le_bytes();
                p.color = u32::from_le_bytes([rgb[0]/32*36, rgb[1]/32*36, rgb[2]/32*36, 255]);
                p.width = 2;
            }
            points.sort_unstable_by_key(|p| p.color);
        }
        let mut result = Self { vertices: Vec::with_capacity(points.len()*12), draws: Vec::new() };
        for p in points.iter().take(MAX_POINTS) {
            if result.draws.last().is_none_or(|d| d.rgba8_srgb != p.color || d.reserved != p.width as u32) {
                result.draws.push(IndexedBatchDrawV2 { first_index: (result.vertices.len()/12) as u32,
                    rgba8_srgb: p.color, topology: PRIMITIVE_TOPOLOGY_POINT_LIST,
                    reserved: p.width as u32, ..IndexedBatchDrawV2::default() });
            }
            result.draws.last_mut().unwrap().index_count += 1;
            for v in p.position { result.vertices.extend_from_slice(&v.to_le_bytes()); }
        }
        // A zero-point background still needs a clear and publication to finish
        // resize. This clipped point uses the same native pass, with zero alpha.
        if result.draws.is_empty() {
            for v in [2f32,2.,2.] { result.vertices.extend_from_slice(&v.to_le_bytes()); }
            result.draws.push(IndexedBatchDrawV2 { index_count: 1, topology: PRIMITIVE_TOPOLOGY_POINT_LIST,
                ..IndexedBatchDrawV2::default() });
        }
        result
    }
}

pub struct Renderer {
    device: Device, queue: Queue, shader: ShaderModule, pipeline: RenderPipeline,
    vertices: Buffer, indices: Buffer,
}
impl Renderer {
    pub fn new() -> Result<Self, i32> {
        let device = Device::open(Capabilities::DEFAULT.union(Capabilities::PRESENT))?;
        let queue = device.create_queue(QueueClass::Render)?;
        let shader = device.create_shader_module(SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64)?;
        let pipeline = device.create_render_pipeline(shader, 12, 0)?;
        let vertices = device.create_buffer(MAX_POINTS*12, BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_VERTEX)?;
        let indices = device.create_buffer(MAX_POINTS*4, BUFFER_USAGE_MAP_WRITE | BUFFER_USAGE_INDEX)?;
        let bytes: Vec<u8> = (0..MAX_POINTS as u32).flat_map(u32::to_le_bytes).collect();
        if device.write_buffer(indices,0,&bytes)? != bytes.len() { return Err(ERR_IO); }
        Ok(Self { device,queue,shader,pipeline,vertices,indices })
    }
    pub fn render(&self, layer: &mut BackgroundLayer, batch: &Batch, extent: (u32,u32), stop: impl Fn()->bool) -> Result<(), Error> {
        let fail = |code| { trueos::logl::log(trueos::logl::level::ERROR,
            format_args!("Cubes: point-list background error={code}")); Error::Ui4 };
        if self.device.write_buffer(self.vertices,0,&batch.vertices).map_err(fail)? != batch.vertices.len() { return Err(Error::Ui4); }
        let mut draws = IndexedDrawBatchV2 { draw_count: batch.draws.len() as u32,
            clear_rgba8_srgb: 0, ..IndexedDrawBatchV2::default() };
        draws.draws[..batch.draws.len()].copy_from_slice(&batch.draws);
        let point = loop {
            if stop() { return Err(Error::Busy); }
            match layer.begin_gpu_frame() {
                Ok(()) => {},
                Err(Error::Busy) => { trueos::vsys::sleep_ms(2); continue; },
                Err(e) => return Err(e),
            }
            // Import Busy retains the write lease, so retry acquire without begin.
            let surface = loop {
                match self.device.acquire_ui4_surface(layer.render_target()) {
                    Ok(surface) => break surface,
                    Err(ERR_BUSY) if !stop() => trueos::vsys::sleep_ms(2),
                    Err(code) => return Err(fail(code)),
                }
            };
            match self.device.submit_ui4_indexed_batch_v2(self.queue,surface,self.pipeline,self.vertices,self.indices,draws) {
                Ok(point) => break point,
                // Ui4Surface's error drop cancels this lease. Restart with begin.
                Err(ERR_BUSY) if !stop() => trueos::vsys::sleep_ms(2),
                Err(code) => return Err(fail(code)),
            }
        };
        self.device.wait(self.queue,point.value).map_err(fail)?;
        layer.publish(Damage::full(extent.0,extent.1))
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.device.destroy_buffer(self.vertices);
        let _ = self.device.destroy_buffer(self.indices);
        let _ = self.device.destroy_render_pipeline(self.pipeline);
        let _ = self.device.destroy_shader_module(self.shader);
        let _ = self.device.destroy_queue(self.queue);
        let _ = self.device.close();
    }
}
