//! Runs the real Wall and vGPU wrappers against a host-only driver.
use super::*;
use core::{future::Future, task::{Context, Poll, Waker}};
use std::{cell::RefCell, collections::BTreeMap};
use trueos::vmedia;
use v::bp_abi::TrueosVmediaRetainedTextureInfo;

#[derive(Default)]
struct Driver {
    next: u64,
    buffers: BTreeMap<u64,Vec<u8>>,
    meshes: BTreeMap<u64,RetainedMeshDescriptor>,
    creates: usize,
    writes: usize,
    short_write: bool,
    busy: bool,
    leased: bool,
    submissions: Vec<RetainedFrameSubmitV4>,
}
thread_local! { static DRIVER: RefCell<Driver> = RefCell::new(Driver::default()); }
fn driver<T>(f: impl FnOnce(&mut Driver)->T)->T { DRIVER.with(|d| f(&mut d.borrow_mut())) }
fn handle(out: *mut u64)->i32 {
    driver(|d| { d.next+=1; unsafe { *out=d.next; } }); 0
}
fn slide(device: Device, session: u64, revision: u32) -> Slide {
    let mut future = core::pin::pin!(vmedia::decode_retained(device, vmedia::ImageFormat::Bmp, b"fixture"));
    let texture = match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(Ok(texture)) => texture,
        _ => panic!("fixture decode"),
    };
    Slide { session, revision, texture, palette: vec![0x801f,0x83e0,0xfc00],
        layout: slideshow::contract::Layout { tiers:[1;6] }, spawn:[0.;3] }
}
fn setup()->(Wall,Queue) {
    driver(|d| *d=Driver::default());
    let device=Device::open(Capabilities::RENDER).unwrap();
    let queue=device.create_queue(QueueClass::Render).unwrap();
    (Wall::new(device,slide(device,1,7)).unwrap(),queue)
}
fn draw(wall:&mut Wall, queue:Queue, now:u64)->Result<(),i32> {
    wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),RetainedCamera::default(),441,now)
}
fn animation(pixels:&[(u8,u8,u8)])->crate::network::HolyFrame {
    crate::network::HolyFrame {session:1,gallery_revision:7,revision:3,index:0,
        cubes:pixels.iter().map(|&(x,y,palette)| cubes_protocol::holy::Pixel{x,y,palette}).collect()}
}
fn active_bytes(wall:&Wall)->Vec<u8> {
    driver(|d| d.buffers[&wall.cubes.seeds[wall.cubes.active].raw()][..wall.cubes.count as usize*64].to_vec())
}
#[test]
fn frames_replace_instances_without_rebuilding_either_mesh() {
    let (mut wall,queue)=setup();
    let center=active_bytes(&wall);
    assert_eq!(wall.cubes.count,27);
    draw(&mut wall,queue,0).unwrap();
    let original=driver(|d| (d.creates,d.submissions[0]));
    for (step, pixels) in [&[(0,47,0),(47,0,2)][..], &[(24,24,1)][..], &[][..]].into_iter().enumerate() {
        wall.replace_holy(animation(pixels)).unwrap();
        let now = step as u64*2000;
        draw(&mut wall,queue,now).unwrap();
        draw(&mut wall,queue,now+333).unwrap();
        draw(&mut wall,queue,now+1033).unwrap();
        assert_eq!(wall.cubes.count,27+pixels.len() as u32);
        assert_eq!(&active_bytes(&wall)[..27*64],&center);
        driver(|d| {
            assert_eq!(d.creates,original.0);
            let frame=d.submissions.last().unwrap();
            assert_eq!(frame.frame.frame.mesh,original.1.frame.frame.mesh);
            assert_eq!(frame.cubes.mesh,original.1.cubes.mesh);
            assert_eq!(frame.frame.frame.material.textures,original.1.frame.frame.material.textures);
            let patch=d.meshes[&frame.cubes.mesh];
            assert_eq!(patch.vertex_layout,RETAINED_VERTEX_LAYOUT_CUBE_PATCH_SEED);
            assert_eq!(patch.index_count,44);
            assert_eq!(patch.topology,RETAINED_TOPOLOGY_CUBE_PATCHLIST_1|RETAINED_MESH_FLAG_DOUBLE_SIDED);
        });
    }
    drop(wall);
    driver(|d| { assert!(d.meshes.is_empty());assert!(d.buffers.is_empty()); });
}
#[test]
fn incomplete_upload_and_old_session_keep_the_displayed_frame() {
    let (mut wall,queue)=setup();
    wall.replace_holy(animation(&[(2,3,0)])).unwrap();
    draw(&mut wall,queue,0).unwrap();
    draw(&mut wall,queue,333).unwrap();
    draw(&mut wall,queue,1033).unwrap();
    let before=active_bytes(&wall);
    driver(|d| d.short_write=true);
    wall.replace_holy(animation(&[(5,6,1),(7,8,2)])).unwrap();
    assert_eq!(draw(&mut wall,queue,1100),Err(ERR_IO));
    assert!(!driver(|d|d.leased));
    driver(|d| d.short_write=false);
    assert_eq!(active_bytes(&wall),before);
    let writes=driver(|d|d.writes);
    let mut old=animation(&[]);old.session=0;
    wall.replace_holy(old).unwrap();
    assert_eq!(driver(|d|d.writes),writes);
    assert_eq!(active_bytes(&wall),before);
    driver(|d| d.busy=true);
    assert_eq!(draw(&mut wall,queue,0),Err(ERR_BUSY));
    assert!(!driver(|d|d.leased));
    driver(|d| d.busy=false);
    draw(&mut wall,queue,0).unwrap();
    wall.replace(slide(wall.device,1,8)).unwrap();
    draw(&mut wall,queue,1200).unwrap();
    assert_eq!(wall.cubes.count,27);
}
#[test]
fn seeds_preserve_landmark_bounds_sparse_pixel_positions_and_colors() {
    let frame=animation(&[(0,47,0),(47,0,1)]);
    let seeds=cube_seeds(&frame.cubes,&[0x801f,0xfc00]).unwrap();
    assert_eq!(seeds.len(),29);
    for (i,seed) in seeds.iter().enumerate() {
        assert_eq!(seed.draw_group,0);
        assert_eq!(seed.flags>>16,i as u32);
        assert_eq!(seed.previous_translation,seed.translation);
        assert_eq!(seed.rotation,[1.,0.,0.,0.]);
    }
    for seed in &seeds[..27] {
        assert_eq!(seed.scale,[0.8;3]);assert_eq!(seed.flags&0xffff,0xffff);
    }
    for axis in 0..3 {
        let lo=seeds[..27].iter().map(|s|s.translation[axis]-s.scale[axis]).fold(f32::INFINITY,f32::min);
        let hi=seeds[..27].iter().map(|s|s.translation[axis]+s.scale[axis]).fold(f32::NEG_INFINITY,f32::max);
        assert!((lo+2.4).abs()<0.00001 && (hi-2.4).abs()<0.00001);
    }
    assert_eq!(seeds[27].scale,[0.1;3]);
    assert!((seeds[27].translation[0]+4.7).abs()<0.00001);
    assert!((seeds[27].translation[1]-2.5).abs()<0.00001);
    assert!((seeds[28].translation[1]-11.9).abs()<0.00001);
    assert_eq!(seeds[27].flags&0xffff,0x801f);
    assert_eq!(seeds[28].flags&0xffff,0xfc00);
    let bytes=seed_bytes(&seeds);
    assert_eq!(bytes.len(),29*64);
    assert_eq!(&bytes[27*64+60..28*64],&seeds[27].flags.to_le_bytes());
    assert!(cube_seeds(&animation(&[(48,0,0)]).cubes,&[0xffff]).is_err());
    assert!(cube_seeds(&frame.cubes,&[]).is_err());
}

#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_open(_:u64,out:*mut u64)->i32 {handle(out)}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_queue_create(_:u64,_:u32,out:*mut u64)->i32 {handle(out)}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_buffer_create(_:u64,len:usize,_:u32,out:*mut u64)->i32 {
    handle(out);driver(|d|{d.buffers.insert(unsafe{*out},vec![0;len]);});0
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_buffer_write(_:u64,id:u64,offset:usize,data:*const u8,len:usize)->isize {
    driver(|d| {
        d.writes+=1;
        let count=if d.short_write {len/2} else {len};
        d.buffers.get_mut(&id).unwrap()[offset..offset+count].copy_from_slice(unsafe{core::slice::from_raw_parts(data,count)});
        count as isize
    })
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_buffer_destroy(_:u64,id:u64)->i32 {driver(|d|{assert!(d.buffers.remove(&id).is_some());});0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_retained_mesh_create(_:u64,desc:*const RetainedMeshDescriptor,out:*mut u64)->i32 {
    handle(out);driver(|d|{d.creates+=1;d.meshes.insert(unsafe{*out},unsafe{*desc});});0
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_retained_mesh_destroy(_:u64,id:u64)->i32 {driver(|d|{assert!(d.meshes.remove(&id).is_some());});0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_ui4_surface_acquire(_:u64,_:u32,out:*mut SurfaceInfo)->i32 {
    driver(|d|{assert!(!d.leased);d.leased=true;});
    unsafe{*out=SurfaceInfo{surface:100,bytes:4,width:1,height:1,pitch:4,format:SURFACE_FORMAT_RGBA8_UNORM_SRGB};}0
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_ui4_surface_discard(_:u64,_:u64)->i32 {driver(|d|{assert!(d.leased);d.leased=false;});0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_retained_frame_submit_v4(_:u64,_:u64,submit:*const RetainedFrameSubmitV4,out:*mut TimelinePoint)->i32 {
    driver(|d|{assert!(d.leased);if d.busy{return ERR_BUSY;}
        d.submissions.push(unsafe{*submit});d.leased=false;
        unsafe{*out=TimelinePoint{value:d.submissions.len() as u64,physical_serial:1};}0})
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vgpu_wait(_:u64,_:u64,_:u64)->i32 {0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_texture_decode_begin(_:u64,_:u32,_:usize)->i32 {let mut id=0;handle(&mut id);id as i32}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_image_decode_begin(_:u32,_:usize)->i32 {unreachable!()}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_image_decode_write(_:u32,_:usize,_:*const u8,_:usize)->i32 {0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_image_decode_commit(_:u32)->i32 {0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_image_decode_status(_:u32)->i32 {1}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_image_decode_discard(_:u32)->i32 {0}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_texture_decode_info(id:u32,out:*mut TrueosVmediaRetainedTextureInfo)->i32 {
    unsafe{*out=TrueosVmediaRetainedTextureInfo{texture_id:id as u64,width:1,height:1,stride_bytes:4,byte_len:4,source_format:3,pixel_format:1,backend:3,revision:1,residency:2,reserved:0};}0
}
#[unsafe(no_mangle)] extern "C" fn trueos_cabi_vmedia_texture_release(_:u64,_:u64)->i32 {0}
