//! Independent ShaderToy producer for the layered window's environment.
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use trueos::ui4_scene::{BackgroundLayer, Error, ShadertoyParamsV1};

const SHADER: u32 = 16;
const PACKAGE: &[u8] = include_bytes!("../Cube/mandelbox/mandelbox.stpkg");
// Names and palette order match cube_tree_builder_world_ramps.html.
const THEMES: [&str; 6] = ["sky", "underground", "black-hole", "white-hole", "island", "city"];

pub fn theme_mask(name: &str) -> u32 {
    let name = name.strip_suffix(".cubes").unwrap_or(name);
    THEMES.iter().enumerate().fold(0, |mask, (index, theme)| {
        mask | if name.split('_').any(|word| word == *theme) { 1 << index } else { 0 }
    })
}

struct Shared {
    sequence: AtomicU32,
    // enabled, theme mask, yaw, pitch, tan(fov/2), width, height.
    command: [AtomicU32; 7],
    stop: AtomicBool,
    done: AtomicBool,
    failed: AtomicBool,
}

pub struct Background {
    shared: Arc<Shared>,
    previous: [u32; 7],
}

impl Background {
    pub fn start(mut layer: BackgroundLayer) -> Result<Self, Error> {
        layer.register_shadertoy(SHADER, PACKAGE)?;
        let shared = Arc::new(Shared {
            sequence: AtomicU32::new(0),
            command: core::array::from_fn(|_| AtomicU32::new(0)),
            stop: AtomicBool::new(false), done: AtomicBool::new(false), failed: AtomicBool::new(false),
        });
        let worker = shared.clone();
        // Capacity failure is explicit; a slow shader is never run inline on
        // the foreground/input lane as an implicit fallback.
        drop(trueos::worker::spawn(move || {
            let mut completed = u32::MAX;
            let mut frame = 0;
            while !worker.stop.load(Ordering::Acquire) {
                let sequence = worker.sequence.load(Ordering::SeqCst);
                if sequence & 1 != 0 || sequence == completed {
                    trueos::vsys::sleep_ms(10);
                    continue;
                }
                let command = core::array::from_fn::<_, 7, _>(|i| worker.command[i].load(Ordering::SeqCst));
                if worker.sequence.load(Ordering::SeqCst) != sequence { continue; }
                match layer.begin_gpu_frame() {
                    Ok(()) => {},
                    Err(Error::Busy) => { trueos::vsys::sleep_ms(10); continue; },
                    Err(error) => { failed(&worker, error); break; },
                }
                let params = ShadertoyParamsV1 {
                    shader_id: SHADER, frame, frame_rate: 10.0,
                    mouse_x: f32::from_bits(command[2]), mouse_y: f32::from_bits(command[3]),
                    click_x: f32::from_bits(command[4]).max(0.1),
                    date_year: command[1] as f32, date_month: command[0] as f32,
                    flags: 0, time_seconds: 0.0, delta_seconds: 0.0, sample_rate: 0.0,
                    click_y: 0.0, date_day: 0.0, date_seconds: 0.0,
                };
                if let Err(error) = layer.render_shadertoy(&params) { failed(&worker,error); break; }
                completed = sequence;
                frame = frame.wrapping_add(1);
                // Give the foreground a write-free interval to stage paired
                // resizes even when a large background takes over 100 ms.
                trueos::vsys::sleep_ms(20);
            }
            worker.done.store(true, Ordering::Release);
        }).map_err(|_| Error::Busy)?);
        Ok(Self { shared, previous: [u32::MAX; 7] })
    }

    pub fn update(&mut self, enabled: bool, name: &str, yaw: f32, pitch: f32, tan_half_fov: f32, extent: (u32,u32)) -> Result<(), Error> {
        if self.shared.failed.load(Ordering::Acquire) { return Err(Error::Ui4); }
        let command = [enabled as u32,theme_mask(name),yaw.to_bits(),pitch.to_bits(),tan_half_fov.to_bits(),extent.0,extent.1];
        if command != self.previous {
            // A single publisher and atomic fields give the worker one coherent
            // latest command. No camera-update queue accumulates behind a bake.
            self.shared.sequence.fetch_add(1,Ordering::SeqCst);
            for (destination,value) in self.shared.command.iter().zip(command) { destination.store(value,Ordering::SeqCst); }
            self.shared.sequence.fetch_add(1,Ordering::SeqCst);
            self.previous = command;
        }
        Ok(())
    }
}

fn failed(shared: &Shared, error: Error) {
    trueos::logl::log(trueos::logl::level::ERROR,format_args!("Cubes: independent Mandelbox background failed: {error:?}"));
    shared.failed.store(true,Ordering::Release);
}

impl Drop for Background {
    fn drop(&mut self) {
        self.shared.stop.store(true,Ordering::Release);
        // This controller is declared before Frame: stop/drain native work
        // before the sole window owner closes either surface.
        while !self.shared.done.load(Ordering::Acquire) { trueos::vsys::sleep_ms(1); }
    }
}
