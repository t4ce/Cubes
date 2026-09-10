//! Independent ShaderToy producer for the layered window's environment.
use crate::environment::{Palette, RotationFollower};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use alloc::vec::Vec;
use std::sync::Mutex;
use crate::pointlist;
use trueos::ui4_scene::{BackgroundLayer, Error, ShadertoyParamsV1};

/// Enable Key5's Mandelbox geometry, resident cubemap bake and view projection.
/// Disabled builds never register or submit that shader; Key2 stays independent.
pub const WORLD_MANDELBOX_ENABLED: bool = false;

const SHADER: u32 = 16;
const PACKAGE: &[u8] = include_bytes!("../Cube/mandelbox/mandelbox.stpkg");
const PALETTE_PACKAGE: &[u8] = include_bytes!("../Cube/palette_grid/palette_grid.stpkg");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Neutral,
    World,
    Cube,
}

const COMMAND_WORDS: usize = 15;

struct Shared {
    sequence: AtomicU32,
    // enabled, generation, RGB x3, color count, preset, quaternion x4, FOV, extent.
    command: [AtomicU32; COMMAND_WORDS],
    points: Mutex<Vec<pointlist::Point>>,
    stop: AtomicBool,
    done: AtomicBool,
    failed: AtomicBool,
}

pub struct Background {
    shared: Arc<Shared>,
    previous: [u32; COMMAND_WORDS],
    generation: u32,
    palette: Palette,
    follower: RotationFollower,
    palette_time: f64,
    resize_generation: u32,
    point_generation: u32,
}

impl Background {
    pub fn start(mut layer: BackgroundLayer) -> Result<Self, Error> {
        layer.set_opacity(0)?;
        if WORLD_MANDELBOX_ENABLED {
            layer.register_shadertoy(SHADER, PACKAGE)?;
        }
        layer.register_shadertoy(4, PALETTE_PACKAGE)?;
        let shared = Arc::new(Shared {
            sequence: AtomicU32::new(0),
            command: core::array::from_fn(|_| AtomicU32::new(0)),
            points: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            done: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        });
        let worker = shared.clone();
        // Capacity failure is explicit; a slow shader is never run inline on
        // the foreground/input lane as an implicit fallback.
        drop(
            trueos::worker::spawn(move || {
                let mut completed = u32::MAX;
                let mut opacity = 0;
                let mut points_renderer = None;
                while !worker.stop.load(Ordering::Acquire)
                    && !trueos::worker::cancellation_requested()
                {
                    let sequence = worker.sequence.load(Ordering::SeqCst);
                    if sequence == 0 || sequence & 1 != 0 || sequence == completed {
                        trueos::vsys::sleep_ms(10);
                        continue;
                    }
                    let command = core::array::from_fn::<_, COMMAND_WORDS, _>(|i| {
                        worker.command[i].load(Ordering::SeqCst)
                    });
                    let mut points = if command[0] == 3 {
                        worker.points.lock().unwrap().clone()
                    } else { Vec::new() };
                    if worker.sequence.load(Ordering::SeqCst) != sequence {
                        continue;
                    }
                    let next_opacity = if command[0] == 3 { 255 } else { 128 };
                    if next_opacity != opacity {
                        if let Err(error) = layer.set_opacity(next_opacity) {
                            failed(&worker, error);
                            break;
                        }
                        opacity = next_opacity;
                    }
                    if command[0] == 0 || command[0] == 3 {
                        if points_renderer.is_none() {
                            match pointlist::Renderer::new() {
                                Ok(renderer) => points_renderer = Some(renderer),
                                Err(code) => {
                                    trueos::logl::log(trueos::logl::level::ERROR,
                                        format_args!("Cubes: point-list renderer init error={code}"));
                                    failed(&worker, Error::Ui4); break;
                                }
                            }
                        }
                        if command[0] == 0 { points = pointlist::pattern(command[1] as u64 * pointlist::PATTERN_MS); }
                        let batch = pointlist::Batch::new(&mut points);
                        let result = points_renderer.as_ref().unwrap().render(&mut layer, &batch,
                            (command[12],command[13]), || worker.stop.load(Ordering::Acquire)
                                || trueos::worker::cancellation_requested());
                        if worker.stop.load(Ordering::Acquire) || trueos::worker::cancellation_requested() { break; }
                        if let Err(error) = result { failed(&worker,error); break; }
                        completed = sequence;
                        trueos::vsys::sleep_ms(10);
                        continue;
                    }
                    match layer.begin_gpu_frame() {
                        Ok(()) => {}
                        Err(Error::Busy) => {
                            trueos::vsys::sleep_ms(10);
                            continue;
                        }
                        Err(error) => {
                            failed(&worker, error);
                            break;
                        }
                    }
                    // Program 16 owns one resident cubemap. Only a new generation
                    // runs the expensive bake; orientation/extent only resample it.
                    let params = ShadertoyParamsV1 {
                        shader_id: if command[0] == 2 { 4 } else { SHADER },
                        frame: command[1],
                        frame_rate: 60.0,
                        mouse_x: f32::from_bits(command[7]),
                        mouse_y: f32::from_bits(command[8]),
                        click_x: f32::from_bits(command[9]),
                        click_y: f32::from_bits(command[10]),
                        delta_seconds: f32::from_bits(command[11]),
                        date_year: command[2] as f32,
                        date_month: command[3] as f32,
                        date_day: command[4] as f32,
                        sample_rate: command[5] as f32,
                        date_seconds: command[6] as f32,
                        time_seconds: if command[0] == 2 {
                            f32::from_bits(command[2])
                        } else {
                            command[0] as f32
                        },
                        flags: 0,
                    };
                    if let Err(error) = layer.render_shadertoy(&params) {
                        failed(&worker, error);
                        break;
                    }
                    completed = sequence;
                    // Leave a write-free interval for paired resize staging.
                    trueos::vsys::sleep_ms(if command[0] == 2 { 1 } else { 16 });
                }
                worker.done.store(true, Ordering::Release);
            })
            .map_err(|_| Error::Busy)?,
        );
        Ok(Self {
            shared,
            previous: [u32::MAX; COMMAND_WORDS],
            generation: 0,
            palette: Palette::for_mandelbox_world("world_27_void").unwrap(),
            follower: RotationFollower::new([0.0, 0.0, 0.0, 1.0]),
            palette_time: 0.,
            resize_generation: 0,
            point_generation: 0,
        })
    }

    /// Every Key 5 selection requests a fresh bake, including revisiting a world.
    pub fn select_world(&mut self, name: &str, rotation: [f32; 4]) -> Result<(), Error> {
        if !WORLD_MANDELBOX_ENABLED {
            return Ok(());
        }
        self.palette = Palette::for_mandelbox_world(name).ok_or(Error::Invalid)?;
        self.generation = self.generation.wrapping_add(1).max(1);
        self.follower = RotationFollower::new(rotation);
        Ok(())
    }

    pub fn update(
        &mut self,
        mode: Mode,
        rotation: [f32; 4],
        delta_seconds: f32,
        tan_half_fov: f32,
        extent: (u32, u32),
    ) -> Result<(), Error> {
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(Error::Ui4);
        }
        let mut command = [0; COMMAND_WORDS];
        if mode == Mode::World && WORLD_MANDELBOX_ENABLED {
            let q = self.follower.advance(rotation, delta_seconds);
            command[..12].copy_from_slice(&[
                1,
                self.generation,
                self.palette.colors[0],
                self.palette.colors[1],
                self.palette.colors[2],
                self.palette.count,
                self.palette.cathedral as u32,
                q[0].to_bits(),
                q[1].to_bits(),
                q[2].to_bits(),
                q[3].to_bits(),
                tan_half_fov.to_bits(),
            ]);
        }
        if mode == Mode::Cube {
            self.palette_time += delta_seconds.max(0.) as f64;
            // Native procedural evaluation: no cached frames, resolution cap, or replay.
            command[0] = 2;
            command[1] = (self.palette_time * 60.) as u32;
            command[2] = (self.palette_time as f32).to_bits();
        }
        if command[0] == 0 {
            command[1] = (trueos::clock::monotonic_millis() / pointlist::PATTERN_MS) as u32;
        }
        command[12] = extent.0;
        command[13] = extent.1;
        command[14] = self.resize_generation;
        self.send(command, None);
        Ok(())
    }

    /// A resize transaction needs publication even if it returns to an earlier
    /// extent before the worker wakes (A -> B -> A). Do not deduplicate by size.
    pub fn resized(&mut self) {
        self.resize_generation = self.resize_generation.wrapping_add(1);
    }

    pub fn world_points(&mut self, cubes: &[crate::orchard::Cube], matrix: &[f32;16], extent: (u32,u32)) -> Result<(), Error> {
        if self.shared.failed.load(Ordering::Acquire) { return Err(Error::Ui4); }
        let mut points = Vec::with_capacity(cubes.len());
        pointlist::project(cubes, matrix, &mut points);
        self.point_generation = self.point_generation.wrapping_add(1);
        let mut command = [0; COMMAND_WORDS];
        command[0] = 3;
        command[1] = self.point_generation;
        command[12] = extent.0;
        command[13] = extent.1;
        command[14] = self.resize_generation;
        self.send(command, Some(points));
        Ok(())
    }

    fn send(&mut self, command: [u32; COMMAND_WORDS], points: Option<Vec<pointlist::Point>>) {
        if command != self.previous {
            // A single publisher and atomic fields give the worker one coherent
            // latest command. No camera-update queue accumulates behind a bake.
            self.shared.sequence.fetch_add(1, Ordering::SeqCst);
            if let Some(points) = points { *self.shared.points.lock().unwrap() = points; }
            for (destination, value) in self.shared.command.iter().zip(command) {
                destination.store(value, Ordering::SeqCst);
            }
            self.shared.sequence.fetch_add(1, Ordering::SeqCst);
            self.previous = command;
        }
    }
}

fn failed(shared: &Shared, error: Error) {
    trueos::logl::log(
        trueos::logl::level::ERROR,
        format_args!("Cubes: resident Chroma background failed: {error:?}"),
    );
    shared.failed.store(true, Ordering::Release);
}

impl Drop for Background {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // This controller is declared before Frame: stop/drain native work
        // before the sole window owner closes either surface.
        while !self.shared.done.load(Ordering::Acquire) {
            trueos::vsys::sleep_ms(1);
        }
    }
}
