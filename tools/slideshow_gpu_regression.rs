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
    Slide { world: vec![], session, revision, texture, palette: vec![0x801f,0x83e0,0xfc00],
        layout: slideshow::contract::Layout { tiers:[1;6] }, spawn:[0.;3] }
}
fn setup()->(Wall,Queue) {
    driver(|d| *d=Driver::default());
    let device=Device::open(Capabilities::RENDER).unwrap();
    let queue=device.create_queue(QueueClass::Render).unwrap();
    (Wall::new(device,slide(device,1,7)).unwrap(),queue)
}
fn draw(wall:&mut Wall, queue:Queue, now:u64)->Result<(),i32> {
    wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),RetainedCamera::default(),441,now,&[],&[])
}
fn animation(pixels:&[(u8,u8,u8)])->crate::network::VfxScene {
    use cubes_protocol::vfx::{Scene,Slot};
    let mut bytes=b"VFX1".to_vec();
    bytes.extend_from_slice(&[1,32,32,10,150,0,3,0,255,0,0,0,255,0,0,0,255]);
    for &(x,y,p) in pixels {bytes.extend_from_slice(&[x,y,p,0,10]);}
    let asset=std::sync::Arc::new(bytes);
    crate::network::VfxScene {session:1, received:trueos::time::Instant::now(),
        info:Scene {gallery_revision:7,event:1,age_ms:500,
            slots:core::array::from_fn(|i|Slot {revision:3,bytes:asset.len() as u32,
                anchor:[[40,8,0],[0,8,40],[-40,8,0],[0,8,-40]][i/6],frames:10,period_ms:150})},
        assets:core::array::from_fn(|_|Some(asset.clone()))}
}
fn active_bytes(wall:&Wall)->Vec<u8> {
    driver(|d| d.buffers[&wall.cubes.seeds[wall.cubes.active].raw()][..wall.cubes.count as usize*64].to_vec())
}
#[test]
fn spawned_terrain_is_opaque_and_does_not_consume_navigation_slots() {
    let (mut wall,queue)=setup();
    let spawn=animation(&[(0,31,0)]);
    wall.replace_vfx(spawn).unwrap();
    let overlay=RetainedTransformSeed {scale:[0.4;3],rotation:[1.,0.,0.,0.],
        local_radius:1.74,flags:24576|512|4096,draw_group:1,
        ..RetainedTransformSeed::default()};
    let terrain=vec![terrain_seed([40,8,0]);TERRAIN_BUDGET];
    wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),
        RetainedCamera::default(),441,0,&terrain,&vec![overlay;OVERLAY_BUDGET]).unwrap();
    assert!(wall.cubes.count as usize <= MAX_CUBES);
    let bytes=active_bytes(&wall);
    let spawned=&bytes[CENTER_COUNT*64..(CENTER_COUNT+1)*64];
    assert_eq!(u32::from_le_bytes(spawned[56..60].try_into().unwrap()),0);
    assert_eq!(f32::from_le_bytes(spawned[4..8].try_into().unwrap()),1.6);
    let mut expired=animation(&[]);expired.info.age_ms=2000;
    wall.replace_vfx(expired).unwrap();
    draw(&mut wall,queue,3000).unwrap();
    assert_eq!(wall.cubes.count,27);
}
#[test]
fn navigation_overlay_has_separate_compact_slots_after_opaque_cubes() {
    let (mut wall,queue)=setup();
    let overlay = RetainedTransformSeed {translation:[0.,3.,0.],scale:[0.4;3],
        rotation:[1.,0.,0.,0.],local_radius:1.74,draw_group:1,
        flags:24576 | 512 | 4096 | (3<<10),..RetainedTransformSeed::default()};
    wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),
        RetainedCamera::default(),441,0,&[],&[overlay,overlay]).unwrap();
    assert_eq!(wall.cubes.count,29);
    let bytes=active_bytes(&wall);
    for (slot,row) in bytes[27*64..].chunks_exact(64).enumerate() {
        assert_eq!(u32::from_le_bytes(row[56..60].try_into().unwrap()),1);
        assert_eq!(u32::from_le_bytes(row[60..64].try_into().unwrap())>>16,slot as u32);
    }
    draw(&mut wall,queue,16).unwrap();
    assert_eq!(wall.cubes.count,27);
}
#[test]
fn terrain_and_vfx_share_compact_slots_and_one_depth_tested_frame() {
    let (mut wall,queue)=setup();
    let terrain = [RetainedTransformSeed {translation:[10.,0.,0.], scale:[0.8;3],
        rotation:[1.,0.,0.,0.],local_radius:1.74,flags:0xffff,
        ..RetainedTransformSeed::default()}];
    wall.replace_vfx(animation(&[(3,4,0)])).unwrap();
    for now in [0,333,1033] {
        wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),
            RetainedCamera::default(),441,now,&terrain,&[]).unwrap();
        assert_eq!(wall.cubes.count,56);
        let bytes=active_bytes(&wall);
        let last=&bytes[bytes.len()-64..];
        assert_eq!(f32::from_le_bytes(last[..4].try_into().unwrap()),10.);
    }
}
#[test]
fn frames_replace_instances_without_rebuilding_either_mesh() {
    let (mut wall,queue)=setup();
    let center=active_bytes(&wall);
    assert_eq!(wall.cubes.count,27);
    draw(&mut wall,queue,0).unwrap();
    let original=driver(|d| (d.creates,d.submissions[0]));
    for (step, pixels) in [&[(0,31,0),(31,0,2)][..], &[(24,24,1)][..], &[][..]].into_iter().enumerate() {
        wall.replace_vfx(animation(pixels)).unwrap();
        let now = step as u64*2000;
        draw(&mut wall,queue,now).unwrap();
        draw(&mut wall,queue,now+333).unwrap();
        draw(&mut wall,queue,now+1033).unwrap();
        assert_eq!(wall.cubes.count,31+24*pixels.len() as u32);
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
    wall.replace_vfx(animation(&[(2,3,0)])).unwrap();
    draw(&mut wall,queue,0).unwrap();
    draw(&mut wall,queue,333).unwrap();
    draw(&mut wall,queue,1033).unwrap();
    let before=active_bytes(&wall);
    driver(|d| d.short_write=true);
    wall.replace_vfx(animation(&[(5,6,1),(7,8,2)])).unwrap();
    assert_eq!(draw(&mut wall,queue,1100),Err(ERR_IO));
    assert!(!driver(|d|d.leased));
    driver(|d| d.short_write=false);
    assert_eq!(active_bytes(&wall),before);
    let writes=driver(|d|d.writes);
    let mut old=animation(&[]);old.session=0;
    wall.replace_vfx(old).unwrap();
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
fn billboard_pixel_spacing_tracks_viewport_right_and_up() {
    let scene=animation(&[(0,31,0),(31,0,1)]);
    let camera=RetainedCamera {view:[
        0.,0.,1.,0., 0.,1.,0.,0., -1.,0.,0.,0., 0.,0.,0.,1.
    ],..Default::default()};
    let seeds=scene_seeds(Some(&scene),&[0x801f,0xfc00],&[],&camera).unwrap();
    assert_eq!(seeds.len(),79);
    for slot in 0..24 {
        let terrain=seeds[27+slot/6];
        assert_eq!(terrain.rotation,[1.,0.,0.,0.]);
        let a=seeds[31+slot*2];let b=seeds[32+slot*2];
        let n=cubes_protocol::vfx::FACE_NORMALS[slot%6];
        // First pixel is at local (-3.1,0.1) relative to the face's bottom-center anchor.
        let expected=core::array::from_fn::<_,3,_>(|axis|
            terrain.translation[axis]+n[axis] as f32*0.8+[0.,0.1,3.1][axis]);
        for axis in 0..3 {assert!((a.translation[axis]-expected[axis]).abs()<0.00001);}
        assert!((b.translation[0]-a.translation[0]).abs()<0.00001);
        assert!((b.translation[1]-a.translation[1]-6.2).abs()<0.00001);
        assert!((b.translation[2]-a.translation[2]+6.2).abs()<0.00001);
        assert_ne!(a.rotation,terrain.rotation);
        assert_eq!(a.flags&0xffff,0x801f);assert_eq!(b.flags&0xffff,0xfc00);
    }
}

#[test]
fn twenty_four_full_planes_fit_with_terrain_and_navigation_and_expire_locally() {
    let (mut wall,queue)=setup();
    let pixels:Vec<_>=(0..32).flat_map(|y|(0..32).map(move |x|(x,y,0))).collect();
    wall.replace_vfx(animation(&pixels)).unwrap();
    let overlay=RetainedTransformSeed {scale:[0.4;3],rotation:[1.,0.,0.,0.],
        local_radius:1.74,flags:24576|512|4096,draw_group:1,..Default::default()};
    wall.render(queue,wall.device.acquire_ui4_surface(1).unwrap(),
        RetainedCamera::default(),441,0,&vec![terrain_seed([40,8,0]);TERRAIN_BUDGET],
        &vec![overlay;OVERLAY_BUDGET]).unwrap();
    assert_eq!(wall.cubes.count as usize,MAX_CUBES);
    let mut scene=animation(&pixels);scene.info.age_ms=499;
    wall.replace_vfx(scene).unwrap();draw(&mut wall,queue,0).unwrap();
    assert_eq!(wall.cubes.count,31);
    let mut scene=animation(&pixels);scene.info.age_ms=2000;
    wall.replace_vfx(scene).unwrap();draw(&mut wall,queue,0).unwrap();
    assert_eq!(wall.cubes.count,27);
    assert_eq!(wall.spawned(),[None;4]);
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
        let submission = unsafe { &*submit };
        if submission.cubes.seed_count as usize>MAX_RETAINED_SCENE_INSTANCES { return ERR_UNSUPPORTED; }
        // Native Picasso requires contiguous group-local output slots, even when
        // authored IDs or visible pixels are sparse (resources.rs draw templates).
        let bytes = &d.buffers[&submission.cubes.seed_buffer];
        let mut slots = [0u32;2];
        for row in bytes[..submission.cubes.seed_count as usize*64].chunks_exact(64) {
            let group = u32::from_le_bytes(row[56..60].try_into().unwrap()) as usize;
            let flags = u32::from_le_bytes(row[60..64].try_into().unwrap());
            if group >= slots.len() || flags >> 16 != slots[group] { return ERR_UNSUPPORTED; }
            if flags & 32768 != 0 && group != 0 { return ERR_UNSUPPORTED; }
            slots[group] += 1;
        }
        d.submissions.push(*submission);d.leased=false;
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
