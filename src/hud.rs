//! Independent, movable UI4 counter window. Never writes to the cube target.

use alloc::vec;
use trueos::tokio::{
    self,
    sync::watch,
    task::JoinHandle,
    time::{Duration, MissedTickBehavior},
};
use trueos::ui4_scene::{Damage, Error, Frame, ResizeEvent, SpriteCorner, SpriteQuad};

#[derive(Clone, Copy)]
struct Counts {
    mode: u8,
    expanded: usize,
    unexpanded: usize,
}

/// The scene owns only a latest-value mailbox and task lifetime, never the HUD frame.
pub struct Worker {
    sender: Option<watch::Sender<Option<Counts>>>,
    task: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn spawn(parent: &Frame) -> Result<Self, Error> {
        let (x, y) = parent.position()?;
        let x = x.saturating_add(parent.width().saturating_sub(WIDTH as u32 + INSET) as i32);
        let y = y.saturating_add(parent.height().saturating_sub(HEIGHT as u32 + INSET) as i32);
        let (sender, mut receiver) = watch::channel::<Option<Counts>>(None);
        let task = tokio::task::spawn_local(async move {
            let result=async {
                let mut panel=Panel::open(x,y)?;
                let mut interval=tokio::time::interval(Duration::from_millis(100));
                interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        changed=receiver.changed() => {
                            if changed.is_err() { break; }
                        }
                        _=interval.tick() => {
                            let counts=*receiver.borrow_and_update();
                            if let Some(counts)=counts {
                                panel.update(trueos::clock::monotonic_millis(),counts.mode,counts.expanded,counts.unexpanded)?;
                            }
                        }
                    }
                }
                Ok::<(),Error>(())
            }.await;
            if let Err(error) = result {
                trueos::logl::log(
                    trueos::logl::level::WARN,
                    format_args!("Cubes: counter task stopped={error:?}; scene continues"),
                );
            }
        });
        Ok(Self {
            sender: Some(sender),
            task: Some(task),
        })
    }
    pub fn send(&self, mode: u8, expanded: usize, unexpanded: usize) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Some(Counts {
                mode,
                expanded,
                unexpanded,
            }));
        }
    }
    pub async fn shutdown(mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

const SPRITE_ID: u32 = 1;
const TEXT_BYTES: usize = 14; // `M3 E0000 U1024`
const PADDING: usize = 2;
const WIDTH: usize = TEXT_BYTES * microfont::FWIDTH + PADDING * 2;
const HEIGHT: usize = microfont::FHEIGHT + PADDING * 2;
const INSET: u32 = 8;

struct Panel {
    frame: Frame,
    shown: Option<[u8; TEXT_BYTES]>,
    pending: Option<[u8; TEXT_BYTES]>,
    resize: Option<ResizeEvent>,
    next_update: u64,
}

impl Panel {
    fn open(x: i32, y: i32) -> Result<Self, Error> {
        let frame = Frame::open(x, y, WIDTH as u32, HEIGHT as u32)?;
        Ok(Self {
            frame,
            shown: None,
            pending: None,
            resize: None,
            next_update: 0,
        })
    }

    pub fn update(
        &mut self,
        now: u64,
        mode: u8,
        expanded: usize,
        unexpanded: usize,
    ) -> Result<(), Error> {
        // Retry publication without acquiring another lease or drawing twice.
        if let Some(text) = self.pending {
            match self
                .frame
                .publish_compute(Damage::full(self.frame.width(), self.frame.height()))
            {
                Ok(()) => {
                    self.shown = Some(text);
                    self.pending = None;
                }
                Err(Error::Busy) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        while let Some(event) = self.frame.take_resize_event()? {
            self.resize = Some(event);
        }
        if let Some(event) = self.resize {
            match self.frame.resize(event.width, event.height) {
                Ok(()) => {
                    self.resize = None;
                    self.shown = None;
                }
                Err(Error::Busy) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        let text = status_text(mode, expanded, unexpanded);
        if self.shown == Some(text) || now < self.next_update {
            return Ok(());
        }
        match self.frame.begin_sprite_frame(0) {
            Ok(()) => {}
            Err(Error::Busy) => return Ok(()),
            Err(error) => return Err(error),
        }
        stamp(&mut self.frame, mode, expanded, unexpanded)?;
        self.pending = Some(text);
        self.next_update = now.saturating_add(100);
        match self
            .frame
            .publish_compute(Damage::full(self.frame.width(), self.frame.height()))
        {
            Ok(()) => {
                self.shown = Some(text);
                self.pending = None;
                Ok(())
            }
            Err(Error::Busy) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// Expanded means a full cube is drawn; unexpanded means its small marker is drawn.
fn stamp(frame: &mut Frame, mode: u8, expanded: usize, unexpanded: usize) -> Result<(), Error> {
    let text = status_text(mode, expanded, unexpanded);
    let mut mask = vec![0u8; WIDTH * HEIGHT];
    let text = core::str::from_utf8(&text).map_err(|_| Error::Invalid)?;
    microfont::stamp_text(
        &mut mask,
        WIDTH,
        HEIGHT,
        PADDING as i32,
        PADDING as i32,
        text,
        1u8,
    )
    .map_err(|_| Error::Invalid)?;

    let mut rgba = vec![0u8; WIDTH * HEIGHT * 4];
    for (ink, pixel) in mask.iter().zip(rgba.chunks_exact_mut(4)) {
        if *ink != 0 {
            pixel.copy_from_slice(&[235, 235, 235, 255]);
        } else {
            pixel.copy_from_slice(&[0, 0, 0, 150]);
        }
    }
    frame.upload_sprite_rgba8(SPRITE_ID, WIDTH as u32, HEIGHT as u32, &rgba)?;

    let x = frame.width().saturating_sub(WIDTH as u32) as f32;
    let y = frame.height().saturating_sub(HEIGHT as u32) as f32;
    let right = x + WIDTH as f32;
    let bottom = y + HEIGHT as f32;
    frame.draw_sprite_quads(&[SpriteQuad {
        sprite_id: SPRITE_ID,
        c0: SpriteCorner {
            x,
            y,
            u: 0.0,
            v: 0.0,
        },
        c1: SpriteCorner {
            x: right,
            y,
            u: 1.0,
            v: 0.0,
        },
        c2: SpriteCorner {
            x: right,
            y: bottom,
            u: 1.0,
            v: 1.0,
        },
        c3: SpriteCorner {
            x,
            y: bottom,
            u: 0.0,
            v: 1.0,
        },
        color_rgba: u32::from_le_bytes([255, 255, 255, 255]),
        source_over: true,
    }])
}

fn status_text(mode: u8, expanded: usize, unexpanded: usize) -> [u8; TEXT_BYTES] {
    let mut text = *b"M0 E0000 U0000";
    text[1] = b'0' + mode.min(9);
    write_count(&mut text[4..8], expanded);
    write_count(&mut text[10..14], unexpanded);
    text
}

fn write_count(output: &mut [u8], value: usize) {
    let mut value = value.min(9_999);
    for byte in output.iter_mut().rev() {
        *byte = b'0' + (value % 10) as u8;
        value /= 10;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_keeps_four_digit_counts_and_mode() {
        assert_eq!(status_text(3, 7, 1024), *b"M3 E0007 U1024");
    }
}
