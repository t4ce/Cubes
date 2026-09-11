//! Persistent Key-8 slideshow connection. Transfers are pinned to a revision;
//! incomplete frames never replace the currently displayed image.
use alloc::{sync::Arc, vec, vec::Vec};
use core::net::Ipv4Addr;
use std::sync::Mutex;
use trueos::{runtime, time, tokio::net::UdpSocket};

const SERVER_PORT: u16 = 30_018;
const MAGIC: &[u8; 4] = b"CUB1";
const HEADER: usize = 8;
const INFO: u8 = 0x85;
const CHUNK: u8 = 0x86;
const CHUNK_BYTES: usize = 1024;
pub const IMAGE_BYTES: usize = 512 * 512 * 3;
const CHUNKS: usize = IMAGE_BYTES / CHUNK_BYTES;
const WINDOW: usize = 32;

pub struct Slide {
    pub session: u64,
    pub rgb: Vec<u8>,
    pub revision: u32,
    pub spawn: [f32; 3],
}
struct Shared {
    session: u64,
    running: bool,
    ready: Option<Result<Slide, &'static str>>,
    position: [f32; 3],
    orientation: [f32; 3],
}
pub struct Client {
    shared: Arc<Mutex<Shared>>,
    held: bool,
}
impl Client {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                session: 0,
                running: false,
                ready: None,
                position: [0.; 3],
                orientation: [0., 0., -1.],
            })),
            held: false,
        }
    }
    pub fn disconnect(&mut self) {
        let mut s = self.shared.lock().unwrap();
        s.session = s.session.wrapping_add(1);
        s.running = false;
        s.ready = None;
    }
    pub fn key(&mut self, held: bool, position: [f32; 3], orientation: [f32; 3]) {
        let pressed = held && !self.held;
        self.held = held;
        let session = {
            let mut s = self.shared.lock().unwrap();
            s.position = position;
            s.orientation = orientation;
            if !pressed {
                return;
            }
            s.running = true;
            s.session = s.session.wrapping_add(1);
            s.ready = None;
            s.session
        };
        let shared = self.shared.clone();
        if trueos::worker::spawn(move || {
            let result = runtime::current_thread_net()
                .build()
                .map_err(|_| "network runtime")
                .and_then(|rt| rt.block_on(stream(&shared, session)));
            let mut s = shared.lock().unwrap();
            if s.session == session {
                s.running = false;
                if let Err(error) = result {
                    s.ready = Some(Err(error));
                }
            }
        })
        .is_err()
        {
            let mut s = self.shared.lock().unwrap();
            s.running = false;
            s.ready = Some(Err("worker unavailable"));
        }
    }
    pub fn take_ready(&mut self) -> Option<Result<Slide, &'static str>> {
        self.shared.lock().unwrap().ready.take()
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn packet(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER + body.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&[1, kind]);
    bytes.extend_from_slice(&(body.len() as u16).to_le_bytes());
    bytes.extend_from_slice(body);
    bytes
}
fn payload(bytes: &[u8], kind: u8) -> Option<&[u8]> {
    if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[4] != 1 || bytes[5] != kind {
        return None;
    }
    let len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    (bytes.len() == HEADER + len).then_some(&bytes[HEADER..])
}
fn info(bytes: &[u8]) -> Option<(u32, [f32; 3])> {
    let p = payload(bytes, INFO)?;
    if p.len() != 20 {
        return None;
    }
    let spawn =
        core::array::from_fn(|i| f32::from_le_bytes(p[8 + i * 4..12 + i * 4].try_into().unwrap()));
    if spawn.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some((u32::from_le_bytes(p[4..8].try_into().unwrap()), spawn))
}
fn accept_chunk(bytes: &[u8], revision: u32, rgb: &mut [u8], received: &mut [bool]) -> bool {
    let Some(p) = payload(bytes, CHUNK) else {
        return false;
    };
    if p.len() != 6 + CHUNK_BYTES || u32::from_le_bytes(p[..4].try_into().unwrap()) != revision {
        return false;
    }
    let index = u16::from_le_bytes(p[4..6].try_into().unwrap()) as usize;
    if index >= received.len() || received[index] {
        return false;
    }
    rgb[index * CHUNK_BYTES..(index + 1) * CHUNK_BYTES].copy_from_slice(&p[6..]);
    received[index] = true;
    true
}
async fn stream(shared: &Mutex<Shared>, session: u64) -> Result<(), &'static str> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "udp bind")?;
    socket
        .connect((Ipv4Addr::LOCALHOST, SERVER_PORT))
        .await
        .map_err(|_| "cubesrv connect")?;
    let mut buffer = [0u8; 1200];
    let mut shown = None;
    let mut sequence = 0u32;
    let mut missed = 0;
    loop {
        let (position, orientation) = {
            let s = shared.lock().unwrap();
            if s.session != session {
                return Ok(());
            }
            (s.position, s.orientation)
        };
        sequence = sequence.wrapping_add(1);
        let mut body = sequence.to_le_bytes().to_vec();
        body.push(27); // Single shared session; the server chooses the slide.
        for v in position.into_iter().chain(orientation) {
            body.extend_from_slice(&v.to_le_bytes());
        }
        socket
            .send(&packet(2, &body))
            .await
            .map_err(|_| "cubesrv send")?;
        // Other players' telemetry must not trigger more outbound telemetry.
        let deadline = time::Instant::now() + time::Duration::from_secs(1);
        let announcement = loop {
            match time::timeout(
                deadline.saturating_duration_since(time::Instant::now()),
                socket.recv(&mut buffer),
            )
            .await
            {
                Ok(Ok(n)) => {
                    if let Some(value) = info(&buffer[..n]) {
                        break Some(value);
                    }
                }
                _ => break None,
            }
            if time::Instant::now() >= deadline {
                break None;
            }
        };
        let Some((revision, spawn)) = announcement else {
            missed += 1;
            if missed >= 10 {
                return Err("cubesrv unavailable");
            }
            continue;
        };
        missed = 0;
        if shown != Some(revision) {
            // Retry loss/reordering in bounded windows without a round trip per pixel block.
            let mut rgb = vec![0; IMAGE_BYTES];
            let mut received = vec![false; CHUNKS];
            let mut failed = false;
            for start in (0..CHUNKS).step_by(WINDOW) {
                let end = (start + WINDOW).min(CHUNKS);
                for _ in 0..8 {
                    if shared.lock().unwrap().session != session {
                        return Ok(());
                    }
                    for chunk in start..end {
                        if !received[chunk] {
                            let mut body = revision.to_le_bytes().to_vec();
                            body.extend_from_slice(&(chunk as u16).to_le_bytes());
                            socket
                                .send(&packet(5, &body))
                                .await
                                .map_err(|_| "cubesrv send")?;
                        }
                    }
                    let deadline = time::Instant::now() + time::Duration::from_millis(200);
                    while !received[start..end].iter().all(|v| *v) {
                        match time::timeout(
                            deadline.saturating_duration_since(time::Instant::now()),
                            socket.recv(&mut buffer),
                        )
                        .await
                        {
                            Ok(Ok(n)) => {
                                accept_chunk(&buffer[..n], revision, &mut rgb, &mut received);
                            }
                            _ => break,
                        }
                        if time::Instant::now() >= deadline {
                            break;
                        }
                    }
                    if received[start..end].iter().all(|v| *v) {
                        break;
                    }
                }
                if !received[start..end].iter().all(|v| *v) {
                    failed = true;
                    break;
                }
            }
            if !failed {
                let mut s = shared.lock().unwrap();
                if s.session != session {
                    return Ok(());
                }
                s.ready = Some(Ok(Slide {
                    session,
                    rgb,
                    revision,
                    spawn,
                }));
                shown = Some(revision);
            }
        }
        // Heartbeats recover lost announcements and keep current telemetry flowing.
        time::sleep(time::Duration::from_millis(250)).await;
    }
}

#[cfg(test)]
#[path = "../../TRUEOS-Blueprints/apps/cubesrv/protocol.rs"]
mod server;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_manifest_carries_origin_spawn() {
        assert_eq!(info(&server::slide_info(7, 12)), Some((12, [0.; 3])));
        let mut bad = server::slide_info(7, 12);
        bad[16..20].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(info(&bad).is_none());
    }
    #[test]
    fn reordered_duplicate_and_stale_chunks_cannot_mix_slides() {
        let source: Vec<_> = (0..IMAGE_BYTES).map(|i| (i % 251) as u8).collect();
        let mut output = vec![0; IMAGE_BYTES];
        let mut received = vec![false; CHUNKS];
        let old = server::slide_chunk(8, 0, &source).unwrap();
        assert!(!accept_chunk(&old, 9, &mut output, &mut received));
        for chunk in (0..CHUNKS).rev() {
            let part = server::slide_chunk(9, chunk as u16, &source).unwrap();
            assert!(part.len() <= 1200);
            assert!(accept_chunk(&part, 9, &mut output, &mut received));
            assert!(!accept_chunk(&part, 9, &mut output, &mut received));
        }
        assert_eq!(output, source);
        assert!(received.iter().all(|v| *v));
    }
}
