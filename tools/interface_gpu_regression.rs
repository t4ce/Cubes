//! Exercise the real vGPU/vmedia wrappers with an injected busy submission.
//! These host ABI stubs never run on TRUEOS.
use super::*;
use crate::cube_interface::EXAMPLES;
use core::{
    future::Future,
    task::{Context, Poll, Waker},
};
use std::cell::RefCell;
use v::bp_abi::TrueosVmediaRetainedTextureInfo;

#[derive(Default)]
struct Driver {
    next: u64,
    submit_error: i32,
    wait_error: i32,
    leased: bool,
    submissions: usize,
    discards: usize,
    waits: usize,
    released_textures: usize,
    sampled: Vec<[u64; 5]>,
    last_draw: Option<(RetainedCamera, RetainedTransformSeed)>,
    vertices: [[f32; 12]; 4],
    indices: [u32; 6],
}
thread_local! { static DRIVER: RefCell<Driver> = RefCell::new(Driver::default()); }
fn driver<T>(f: impl FnOnce(&mut Driver) -> T) -> T {
    DRIVER.with(|d| f(&mut d.borrow_mut()))
}
fn handle(out: *mut u64) -> i32 {
    driver(|d| {
        d.next += 1;
        unsafe {
            *out = d.next;
        }
    });
    0
}
fn texture(device: Device) -> vmedia::RetainedTexture {
    let mut future = core::pin::pin!(vmedia::decode_retained(
        device,
        vmedia::ImageFormat::Bmp,
        b"fixture"
    ));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(Ok(texture)) => texture,
        _ => panic!("fixture decode must complete immediately"),
    }
}
fn setup() -> (Renderer, Queue, Demo) {
    driver(|d| *d = Driver::default());
    let device = Device::open(Capabilities::RENDER).unwrap();
    let queue = device.create_queue(QueueClass::Render).unwrap();
    let mut renderer = Renderer::new(device).unwrap();
    renderer.textures = Some(Textures {
        page: 0,
        color: texture(device),
        normal: texture(device),
    });
    (renderer, queue, Demo::new())
}
fn draw(renderer: &Renderer, queue: Queue, demo: &Demo) -> Result<bool, i32> {
    let surface = renderer.device.acquire_ui4_surface(1).unwrap();
    renderer.render(queue, surface, demo, RetainedCamera::default())
}

#[test]
fn busy_submit_discards_lease_then_retries_with_live_textures() {
    let (renderer, queue, demo) = setup();
    for _ in 0..50 {
        driver(|d| d.submit_error = ERR_BUSY);
        assert_eq!(draw(&renderer, queue, &demo), Ok(false));
        assert!(!driver(|d| d.leased));
        driver(|d| d.submit_error = 0);
        assert_eq!(draw(&renderer, queue, &demo), Ok(true));
    }
    driver(|d| {
        assert_eq!((d.submissions, d.discards, d.waits), (100, 50, 50));
        assert_eq!(d.released_textures, 0);
        assert!(d.sampled.iter().all(|textures| textures == &d.sampled[0]));
    });
    drop(renderer);
    assert_eq!(driver(|d| d.released_textures), 2);
}

#[test]
fn upload_keeps_visible_pair_but_defers_gpu_work_until_publication_finishes() {
    let (renderer, queue, demo) = setup();
    renderer.work.lock().unwrap().busy = true;
    assert!(renderer.ready(0)); // Input still belongs to the visible example.
    assert!(renderer.uploading());
    assert!(!renderer.can_render(0));
    assert_eq!(draw(&renderer, queue, &demo), Ok(false));
    driver(|d| assert_eq!((d.submissions, d.discards, d.waits), (0, 1, 0)));
    renderer.work.lock().unwrap().busy = false;
    assert!(renderer.can_render(0));
    assert!(!renderer.can_render(1));
    assert_eq!(draw(&renderer, queue, &demo), Ok(true));
}

#[test]
fn real_errors_and_unretired_submissions_are_not_treated_as_skipped_frames() {
    let (renderer, queue, demo) = setup();
    driver(|d| d.submit_error = ERR_DEVICE_LOST);
    assert_eq!(draw(&renderer, queue, &demo), Err(ERR_DEVICE_LOST));
    assert_eq!(driver(|d| d.waits), 0);
    driver(|d| {
        d.submit_error = 0;
        d.wait_error = ERR_BUSY;
    });
    assert_eq!(draw(&renderer, queue, &demo), Err(ERR_BUSY));
    assert_eq!(driver(|d| d.discards), 1);
    assert_eq!(driver(|d| d.waits), 1);
}

#[test]
fn rendered_menu_matches_hitboxes_after_native_y_adjustment() {
    let (mut renderer, queue, mut demo) = setup();
    for page in 0..EXAMPLES.len() {
        demo.select(page);
        renderer.textures.as_mut().unwrap().page = page;
        let (columns, rows) = (demo.layout.width, demo.layout.height);
        let tier = EXAMPLES[page].tier;
        for (width, height) in [(784, 441), (441, 784), (1920, 1080)] {
            let mut logical =
                camera(width, height, columns, rows, tier).retained(width, height, [0.; 16]);
            logical.previous_view_projection = logical.view_projection;
            let surface = renderer.device.acquire_ui4_surface(1).unwrap();
            assert_eq!(renderer.render(queue, surface, &demo, logical), Ok(true));
            let (gpu, seed) = driver(|d| d.last_draw.unwrap());
            let vertices = driver(|d| d.vertices);
            // Follow the actual uploaded quad's UV -> position mapping.
            let v00 = vertices.iter().find(|v| v[6..8] == [0., 0.]).unwrap();
            let v10 = vertices.iter().find(|v| v[6..8] == [1., 0.]).unwrap();
            let v01 = vertices.iter().find(|v| v[6..8] == [0., 1.]).unwrap();
            let project = |u: f32, v: f32| {
                let local = core::array::from_fn(|a| {
                    (v00[a] + u * (v10[a] - v00[a]) + v * (v01[a] - v00[a])) * seed.scale[a]
                });
                let world = Quaternion(seed.rotation).rotate(local);
                let point = [
                    world[0] + seed.translation[0],
                    world[1] + seed.translation[1],
                    world[2] + seed.translation[2],
                    1.,
                ];
                let clip: [f32; 4] = core::array::from_fn(|r| {
                    (0..4)
                        .map(|c| gpu.view_projection[c * 4 + r] * point[c])
                        .sum()
                });
                // Native contract: Naga negates Position.y, then SF applies
                // scale_y=-height/2. See TRUEOS picasso_vue_compare.rs and
                // intel/render/pipeline.rs; CPU projection alone missed this.
                let lowered_y = -clip[1];
                [
                    (clip[0] / clip[3] + 1.) * width as f32 * 0.5 - 0.5,
                    (1. - lowered_y / clip[3]) * height as f32 * 0.5 - 0.5,
                ]
            };
            assert!(
                project(0.5, 0.05)[1] < project(0.5, 0.95)[1],
                "title must be above buttons"
            );
            for (id, widget) in demo
                .layout
                .widgets
                .iter()
                .enumerate()
                .filter(|(_, w)| !w.disabled)
            {
                let [x, y, w, h] = widget.rect;
                let screen = project(
                    (x as f32 + w as f32 * 0.5) / columns as f32,
                    (y as f32 + h as f32 * 0.5) / rows as f32,
                );
                let (o, d) = crate::picking::ray(
                    &logical.inverse_view_projection,
                    screen[0].round() as i32,
                    screen[1].round() as i32,
                    width,
                    height,
                )
                .unwrap();
                assert_eq!(demo.hit(hit(o, d, columns, rows, tier).unwrap()), Some(id));
            }
            // The retained renderer declares clockwise screen triangles as
            // front faces. Preserve that so double-sided PBR does not invert normals.
            for tri in driver(|d| d.indices).chunks_exact(3) {
                let [a, b, c] = core::array::from_fn::<_, 3, _>(|i| {
                    let vertex = vertices[tri[i] as usize];
                    project(vertex[6], vertex[7])
                });
                assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.);
            }
            for r in 0..4 {
                for c in 0..4 {
                    let value: f32 = (0..4)
                        .map(|k| {
                            gpu.view_projection[k * 4 + r] * gpu.inverse_view_projection[c * 4 + k]
                        })
                        .sum();
                    assert!((value - if r == c { 1. } else { 0. }).abs() < 0.001);
                }
            }
            assert_eq!(gpu.previous_view_projection, gpu.view_projection);
            assert_eq!(gpu.view, logical.view);
            assert_eq!(gpu.position_near, logical.position_near);
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_open(_: u64, out: *mut u64) -> i32 {
    handle(out)
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_queue_create(_: u64, _: u32, out: *mut u64) -> i32 {
    handle(out)
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_buffer_create(_: u64, _: usize, _: u32, out: *mut u64) -> i32 {
    handle(out)
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_buffer_write(
    _: u64,
    _: u64,
    _: usize,
    data: *const u8,
    len: usize,
) -> isize {
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    driver(|d| match len {
        192 => {
            for (i, value) in d.vertices.iter_mut().flatten().enumerate() {
                *value = f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
            }
        }
        24 => {
            for (i, value) in d.indices.iter_mut().enumerate() {
                *value = u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
            }
        }
        _ => {}
    });
    len as isize
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_buffer_destroy(_: u64, _: u64) -> i32 {
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_retained_mesh_create(
    _: u64,
    _: *const RetainedMeshDescriptor,
    out: *mut u64,
) -> i32 {
    handle(out)
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_retained_mesh_destroy(_: u64, _: u64) -> i32 {
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_ui4_surface_acquire(_: u64, _: u32, out: *mut SurfaceInfo) -> i32 {
    driver(|d| {
        assert!(!d.leased, "retry must release the previous UI4 write lease");
        d.leased = true;
    });
    unsafe {
        *out = SurfaceInfo {
            surface: 100,
            bytes: 4,
            width: 1,
            height: 1,
            pitch: 4,
            format: SURFACE_FORMAT_RGBA8_UNORM_SRGB,
        };
    }
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_ui4_surface_discard(_: u64, _: u64) -> i32 {
    driver(|d| {
        assert!(d.leased);
        d.leased = false;
        d.discards += 1;
    });
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_retained_frame_submit_v2(
    _: u64,
    _: u64,
    submit: *const RetainedFrameSubmitV2,
    point: *mut TimelinePoint,
) -> i32 {
    driver(|d| {
        assert!(d.leased);
        d.submissions += 1;
        d.sampled.push(unsafe { (*submit).frame.material.textures });
        d.last_draw = Some(unsafe { ((*submit).frame.camera, (*submit).frame.seeds[0]) });
        if d.submit_error == 0 {
            d.leased = false;
            unsafe {
                *point = TimelinePoint {
                    value: d.submissions as u64,
                    physical_serial: 1,
                };
            }
        }
        d.submit_error
    })
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vgpu_wait(_: u64, _: u64, _: u64) -> i32 {
    driver(|d| {
        d.waits += 1;
        d.wait_error
    })
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_texture_decode_begin(_: u64, _: u32, _: usize) -> i32 {
    let mut id = 0;
    handle(&mut id);
    id as i32
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_image_decode_begin(_: u32, _: usize) -> i32 {
    unreachable!()
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_image_decode_write(
    _: u32,
    _: usize,
    _: *const u8,
    _: usize,
) -> i32 {
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_image_decode_commit(_: u32) -> i32 {
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_image_decode_status(_: u32) -> i32 {
    1
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_image_decode_discard(_: u32) -> i32 {
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_texture_decode_info(
    id: u32,
    out: *mut TrueosVmediaRetainedTextureInfo,
) -> i32 {
    unsafe {
        *out = TrueosVmediaRetainedTextureInfo {
            texture_id: id as u64,
            width: 1,
            height: 1,
            stride_bytes: 4,
            byte_len: 4,
            source_format: 3,
            pixel_format: 1,
            backend: 3,
            revision: 1,
            residency: 2,
            reserved: 0,
        };
    }
    0
}
#[unsafe(no_mangle)]
extern "C" fn trueos_cabi_vmedia_texture_release(_: u64, _: u64) -> i32 {
    driver(|d| d.released_textures += 1);
    0
}
