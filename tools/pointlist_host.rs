#![allow(dead_code)]
extern crate alloc;
extern crate self as trueos;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64,Ordering};
static NOW:AtomicU64=AtomicU64::new(0);
#[derive(Default)]
struct State { lease:bool, pending:bool, foreground_ready:bool, commits:usize,
    publishes:usize, shaders:Vec<u32>, batches:Vec<Vec<vgpu::IndexedBatchDrawV2>>, opacity:u8,
    begin_busy:usize, submit_busy:usize, next_surface:u64 }
static STATE:Mutex<State>=Mutex::new(State{lease:false,pending:false,foreground_ready:false,commits:0,
    publishes:0,shaders:Vec::new(),batches:Vec::new(),opacity:0,begin_busy:0,submit_busy:0,next_surface:0});
pub mod clock { pub fn monotonic_millis()->u64 {super::NOW.load(super::Ordering::Relaxed)} }
pub mod vsys { pub fn sleep_ms(ms:u64){std::thread::sleep(std::time::Duration::from_millis(ms));} }
pub mod worker {
    pub fn cancellation_requested()->bool{false}
    pub fn spawn(f:impl FnOnce()+Send+'static)->Result<std::thread::JoinHandle<()>,()> {Ok(std::thread::spawn(f))}
}
pub mod logl { pub mod level {pub const ERROR:u8=1;pub const INFO:u8=2;} pub fn log(_:u8,_:std::fmt::Arguments){} }
pub mod ui4_scene {
    pub use trueos_sdk::ui4_scene::{Damage,Error,ShadertoyParamsV1};
    pub struct BackgroundLayer;
    impl BackgroundLayer {
        pub fn set_opacity(&mut self,v:u8)->Result<(),Error>{super::STATE.lock().unwrap().opacity=v;Ok(())}
        pub fn register_shadertoy(&mut self,_:u32,_:&[u8])->Result<(),Error>{Ok(())}
        pub fn render_target(&self)->u32{1}
        pub fn begin_gpu_frame(&mut self)->Result<(),Error>{
            let mut s=super::STATE.lock().unwrap();
            if s.begin_busy>0{s.begin_busy-=1;return Err(Error::Busy);}
            assert!(!s.lease,"duplicate begin without releasing frame");s.lease=true;Ok(())
        }
        pub fn publish(&mut self,_:Damage)->Result<(),Error>{
            let mut s=super::STATE.lock().unwrap();assert!(s.lease);s.lease=false;s.publishes+=1;
            if s.pending&&s.foreground_ready{s.pending=false;s.commits+=1;}Ok(())
        }
        pub fn render_shadertoy(&mut self,p:&ShadertoyParamsV1)->Result<(),Error>{
            super::STATE.lock().unwrap().shaders.push(p.shader_id);
            self.publish(Damage::full(1,1))
        }
    }
}
pub mod vgpu {
    pub use trueos_sdk::vgpu::{IndexedBatchDrawV2,IndexedDrawBatchV2,Capabilities,QueueClass,
        BUFFER_USAGE_MAP_WRITE,BUFFER_USAGE_VERTEX,BUFFER_USAGE_INDEX,ERR_IO,ERR_BUSY,
        MAX_INDEXED_BATCH_V2_DRAWS,PRIMITIVE_TOPOLOGY_POINT_LIST,SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64};
    #[derive(Clone,Copy)] pub struct Device;
    #[derive(Clone,Copy)] pub struct Buffer;
    #[derive(Clone,Copy)] pub struct Queue;
    #[derive(Clone,Copy)] pub struct ShaderModule;
    #[derive(Clone,Copy)] pub struct RenderPipeline;
    pub struct Surface{live:bool}
    pub struct Point{pub value:u64}
    impl Drop for Surface {fn drop(&mut self){if self.live{super::STATE.lock().unwrap().lease=false;}}}
    impl Device {
        pub fn open(_:Capabilities)->Result<Self,i32>{Ok(Self)}
        pub fn create_queue(self,_:QueueClass)->Result<Queue,i32>{Ok(Queue)}
        pub fn create_shader_module(self,id:u64)->Result<ShaderModule,i32>{assert_eq!(id,SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64);Ok(ShaderModule)}
        pub fn create_render_pipeline(self,_:ShaderModule,stride:u32,offset:u32)->Result<RenderPipeline,i32>{assert_eq!((stride,offset),(12,0));Ok(RenderPipeline)}
        pub fn create_buffer(self,_:usize,_:u32)->Result<Buffer,i32>{Ok(Buffer)}
        pub fn write_buffer(self,_:Buffer,_:usize,b:&[u8])->Result<usize,i32>{Ok(b.len())}
        pub fn acquire_ui4_surface(self,_:u32)->Result<Surface,i32>{assert!(super::STATE.lock().unwrap().lease);Ok(Surface{live:true})}
        pub fn submit_ui4_indexed_batch_v2(self,_:Queue,mut surface:Surface,_:RenderPipeline,_:Buffer,_:Buffer,b:IndexedDrawBatchV2)->Result<Point,i32>{
            let mut s=super::STATE.lock().unwrap();
            if s.submit_busy>0{s.submit_busy-=1;return Err(ERR_BUSY);}
            assert!(b.draw_count>0);assert_eq!(b.clear_rgba8_srgb,0);
            let draws=&b.draws[..b.draw_count as usize];
            assert!(draws.iter().all(|d|d.topology==PRIMITIVE_TOPOLOGY_POINT_LIST));
            s.batches.push(draws.to_vec());surface.live=false;Ok(Point{value:1})
        }
        pub fn wait(self,_:Queue,_:u64)->Result<(),i32>{Ok(())}
        pub fn destroy_buffer(self,_:Buffer)->Result<(),i32>{Ok(())}
        pub fn destroy_render_pipeline(self,_:RenderPipeline)->Result<(),i32>{Ok(())}
        pub fn destroy_shader_module(self,_:ShaderModule)->Result<(),i32>{Ok(())}
        pub fn destroy_queue(self,_:Queue)->Result<(),i32>{Ok(())}
        pub fn close(self)->Result<(),i32>{Ok(())}
    }
}
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_blueprint_shutdown(_: *const u8,_:usize)->i32{0}
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_write(_:u32,_:*const u8,_:usize){}
// MODULES
fn reset(){*STATE.lock().unwrap()=State::default();NOW.store(0,Ordering::Relaxed);}
fn await_publications(n:usize){
    let start=std::time::Instant::now();
    while STATE.lock().unwrap().publishes<n {
        assert!(start.elapsed().as_secs()<3,"background never published; resize barrier stuck");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
#[test] fn key1_circles_use_exact_potato_geometry_colors_and_widths(){
    use potato_stamps::scene::*;
    let circles=pointlist::circles();let colors=decode_palette_rgba(COLOR_TEXTURE_BYTES).unwrap();
    let rings=quad_strip_ring_positions();assert_eq!(circles.len(),256);
    for circle in 0..4 {for step in 0..64 {
        let p=circles[circle*64+step];
        let r=rings[(circle/2)*QUAD_STRIP_RING_VERTICES_PER_RING+step*2+circle%2];
        assert_eq!(p.position,[r.x,r.y,r.z]);assert_eq!(p.color,colors[circle]);
        assert_eq!(p.width,POINT_RING_POINT_WIDTHS_PX[circle]);
    }}
}
#[test] fn real_background_worker_releases_resize_during_idle_world_and_aba(){
    reset();let mut bg=background::Background::start(ui4_scene::BackgroundLayer).unwrap();
    let q=[0.,0.,0.,1.];
    bg.update(background::Mode::Neutral,q,0.,1.,(784,441)).unwrap();await_publications(1);
    NOW.store(15000,Ordering::Relaxed);
    bg.update(background::Mode::Neutral,q,15.,1.,(784,441)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(25));assert_eq!(STATE.lock().unwrap().publishes,1);
    {let mut s=STATE.lock().unwrap();s.pending=true;s.foreground_ready=true;s.begin_busy=2;s.submit_busy=1;}
    // Two successful stages may return to the original extent before the worker polls.
    bg.resized();bg.resized();
    bg.update(background::Mode::Neutral,q,0.,1.,(784,441)).unwrap();await_publications(2);
    assert_eq!(STATE.lock().unwrap().commits,1);
    {let mut s=STATE.lock().unwrap();s.pending=true;}
    bg.resized();bg.update(background::Mode::World,q,0.,1.,(2560,1440)).unwrap();await_publications(3);
    {let s=STATE.lock().unwrap();assert_eq!(s.commits,2);assert_eq!(s.batches.last().unwrap().iter().map(|d|d.index_count).sum::<u32>(),256);assert!(s.shaders.is_empty());}
    bg.update(background::Mode::Cube,q,0.016,1.,(2560,1440)).unwrap();await_publications(4);
    assert_eq!(STATE.lock().unwrap().shaders,vec![4]);
    NOW.store(5000,Ordering::Relaxed);
    bg.update(background::Mode::Neutral,q,0.,1.,(2560,1440)).unwrap();await_publications(5);
    assert_eq!(STATE.lock().unwrap().batches.last().unwrap().iter().map(|d|d.index_count).sum::<u32>(),256);
    drop(bg);
}
#[test] fn point_batch_offsets_avoid_duplicate_vertex_prefixes_and_bound_colors(){
    let mut points:Vec<_>=(0..pointlist::MAX_POINTS).map(|i|pointlist::Point{position:[0.;3],color:(i as u32)|0xff000000,width:2}).collect();
    let batch=pointlist::Batch::new(&mut points);
    assert!(batch.draws.len()<=600);assert_eq!(batch.vertices.len(),pointlist::MAX_POINTS*12);
    let mut cursor=0;
    for d in batch.draws{assert_eq!(d.first_index,0);assert_eq!(d.base_vertex,cursor);cursor+=d.index_count as i32;}
    assert_eq!(cursor,pointlist::MAX_POINTS as i32);
}
