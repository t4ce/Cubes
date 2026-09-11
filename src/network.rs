//! Persistent Key-8 slideshow connection. Transfers are pinned to a revision;
//! incomplete frames never replace the currently displayed image.
use alloc::{sync::Arc, vec, vec::Vec};
use core::net::Ipv4Addr;
use std::sync::Mutex;
use trueos::{runtime, time, tokio::net::UdpSocket, vmedia};

const SERVER_PORT: u16 = 30_018;
const MAGIC: &[u8; 4] = b"CUB1";
const HEADER: usize = 8;
const INFO: u8 = 0x85;
const CHUNK: u8 = 0x86;
const CHUNK_BYTES: usize = 1024;
const MAX_ENCODED_BYTES: usize = 4 * 1024 * 1024;
const WINDOW: usize = 32;

pub struct BevelMaps {
    pub normal: vmedia::RetainedTexture,
    pub occlusion: vmedia::RetainedTexture,
}

pub struct Slide {
    pub session: u64,
    pub texture: vmedia::RetainedTexture,
    pub bevel: Arc<BevelMaps>,
    pub revision: u32,
    pub spawn: [f32; 3],
}
struct Shared {
    session: u64,
    bevel: Option<Arc<BevelMaps>>,
    bevel_loading: bool,
    running: bool,
    ready: Option<Result<Slide, &'static str>>,
    position: [f32; 3],
    orientation: [f32; 3],
}
pub struct Client {
    device: trueos::vgpu::Device,
    shared: Arc<Mutex<Shared>>,
    held: bool,
}
impl Client {
    pub fn new(device: trueos::vgpu::Device) -> Self {
        Self {
            device,
            shared: Arc::new(Mutex::new(Shared {
                session: 0,
                bevel: None,
                bevel_loading: false,
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
        let device = self.device;
        if trueos::worker::spawn(move || {
            let result = runtime::current_thread_net()
                .build()
                .map_err(|_| "network runtime")
                .and_then(|rt| rt.block_on(stream(&shared, session, device)));
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
fn info(bytes: &[u8]) -> Option<(u32, [f32; 3], usize)> {
    let p = payload(bytes, INFO)?;
    if p.len() != 24 {
        return None;
    }
    let spawn =
        core::array::from_fn(|i| f32::from_le_bytes(p[8 + i * 4..12 + i * 4].try_into().unwrap()));
    if spawn.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let size = u32::from_le_bytes(p[20..24].try_into().unwrap()) as usize;
    if size == 0 || size > MAX_ENCODED_BYTES {
        return None;
    }
    Some((u32::from_le_bytes(p[4..8].try_into().unwrap()), spawn, size))
}
fn accept_chunk(bytes: &[u8], revision: u32, encoded: &mut [u8], received: &mut [bool]) -> bool {
    let Some(p) = payload(bytes, CHUNK) else {
        return false;
    };
    if p.len() < 6 || u32::from_le_bytes(p[..4].try_into().unwrap()) != revision {
        return false;
    }
    let index = u16::from_le_bytes(p[4..6].try_into().unwrap()) as usize;
    if index >= received.len() || received[index] {
        return false;
    }
    let start = index * CHUNK_BYTES;
    if start >= encoded.len() {
        return false;
    }
    let end = (start + CHUNK_BYTES).min(encoded.len());
    if p.len() != 6 + end - start {
        return false;
    }
    encoded[start..end].copy_from_slice(&p[6..]);
    received[index] = true;
    true
}
fn image_format(bytes: &[u8]) -> Option<vmedia::ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(vmedia::ImageFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(vmedia::ImageFormat::Jpeg)
    } else {
        None
    }
}
fn valid_image_extent(width: u32, height: u32) -> bool {
    width == 512 && height == 512
}
async fn decode_texture(
    device: trueos::vgpu::Device,
    encoded: &[u8],
) -> Result<vmedia::RetainedTexture, &'static str> {
    let format = image_format(encoded).ok_or("slide image format")?;
    time::timeout(
        time::Duration::from_secs(10),
        vmedia::decode_retained(device, format, encoded),
    )
    .await
    .map_err(|_| "slide texture timeout")?
    .map_err(|_| "slide texture decode failed")
}
async fn bevel_maps(device: trueos::vgpu::Device) -> Result<Arc<BevelMaps>, &'static str> {
    let normal = decode_texture(device, include_bytes!("../assets/slideshow/normal.png")).await?;
    let occlusion =
        decode_texture(device, include_bytes!("../assets/slideshow/occlusion.png")).await?;
    Ok(Arc::new(BevelMaps { normal, occlusion }))
}
/// One shared pair of data maps per device, including across reconnects.
async fn shared_bevel_maps(
    shared: &Mutex<Shared>,
    session: u64,
    device: trueos::vgpu::Device,
) -> Result<Arc<BevelMaps>, &'static str> {
    loop {
        let load = {
            let mut state = shared.lock().unwrap();
            if state.session != session {
                return Err("slide session cancelled");
            }
            if let Some(maps) = &state.bevel {
                return Ok(maps.clone());
            }
            if state.bevel_loading {
                false
            } else {
                state.bevel_loading = true;
                true
            }
        };
        if load {
            let result = bevel_maps(device).await;
            let mut state = shared.lock().unwrap();
            state.bevel_loading = false;
            if let Ok(maps) = &result {
                state.bevel = Some(maps.clone());
            }
            return result;
        }
        time::sleep(time::Duration::from_millis(10)).await;
    }
}
async fn stream(
    shared: &Mutex<Shared>,
    session: u64,
    device: trueos::vgpu::Device,
) -> Result<(), &'static str> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "udp bind")?;
    socket
        .connect((Ipv4Addr::LOCALHOST, SERVER_PORT))
        .await
        .map_err(|_| "cubesrv connect")?;
    let mut buffer = [0u8; 1200];
    let mut shown = None;
    let mut bevel = None;
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
        let mut hello = vec![27];
        hello.extend_from_slice(b"t4ce");
        socket.send(&packet(1, &hello)).await.map_err(|_| "cubesrv username")?;
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
        let Some((revision, spawn, encoded_len)) = announcement else {
            missed += 1;
            if missed >= 10 {
                return Err("cubesrv unavailable");
            }
            continue;
        };
        missed = 0;
        if shown != Some(revision) {
            // Retry loss/reordering in bounded windows without a round trip per pixel block.
            let mut encoded = vec![0; encoded_len];
            let chunks = encoded_len.div_ceil(CHUNK_BYTES);
            let mut received = vec![false; chunks];
            let mut failed = false;
            for start in (0..chunks).step_by(WINDOW) {
                let end = (start + WINDOW).min(chunks);
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
                                accept_chunk(&buffer[..n], revision, &mut encoded, &mut received);
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
                if shared.lock().unwrap().session != session {
                    return Ok(());
                }
                let texture = decode_texture(device, &encoded).await?;
                if !valid_image_extent(texture.info().width, texture.info().height) {
                    return Err("slide image dimensions");
                }
                if bevel.is_none() {
                    bevel = Some(shared_bevel_maps(shared, session, device).await?);
                }
                let mut s = shared.lock().unwrap();
                if s.session != session {
                    return Ok(());
                }
                s.ready = Some(Ok(Slide {
                    session,
                    texture,
                    bevel: bevel.as_ref().unwrap().clone(),
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
    fn manifest_bounds_and_short_chunk_lengths_are_checked() {
        assert!(info(&server::slide_info(1, 0, 0)).is_none());
        assert!(info(&server::slide_info(1, 0, MAX_ENCODED_BYTES + 1)).is_none());
        let source = vec![5; CHUNK_BYTES + 7];
        let mut output = vec![0; source.len()];
        let mut received = vec![false; 2];
        let valid = server::slide_chunk(1, 1, &source).unwrap();
        let mut body = payload(&valid, CHUNK).unwrap().to_vec();
        body.pop();
        assert!(!accept_chunk(
            &packet(CHUNK, &body),
            1,
            &mut output,
            &mut received
        ));
        assert!(accept_chunk(&valid, 1, &mut output, &mut received));
    }
    #[test]
    fn native_texture_extent_and_image_format_are_checked() {
        assert!(valid_image_extent(512, 512));
        assert!(!valid_image_extent(511, 512));
        assert!(!valid_image_extent(512, 0));
        assert_eq!(image_format(b"raw-rgb"), None);
    }
    #[test]
    fn server_manifest_carries_origin_spawn() {
        assert_eq!(
            info(&server::slide_info(7, 12, 12345)),
            Some((12, [0.; 3], 12345))
        );
        let mut bad = server::slide_info(7, 12, 12345);
        bad[16..20].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(info(&bad).is_none());
    }
    #[test]
    fn reordered_duplicate_and_stale_chunks_cannot_mix_slides() {
        let source: Vec<_> = (0..(CHUNK_BYTES * 3 + 17))
            .map(|i| (i % 251) as u8)
            .collect();
        let mut output = vec![0; source.len()];
        let chunks = source.len().div_ceil(CHUNK_BYTES);
        let mut received = vec![false; chunks];
        let old = server::slide_chunk(8, 0, &source).unwrap();
        assert!(!accept_chunk(&old, 9, &mut output, &mut received));
        for chunk in (0..chunks).rev() {
            let part = server::slide_chunk(9, chunk as u16, &source).unwrap();
            assert!(part.len() <= 1200);
            assert!(accept_chunk(&part, 9, &mut output, &mut received));
            assert!(!accept_chunk(&part, 9, &mut output, &mut received));
        }
        assert_eq!(output, source);
        assert!(received.iter().all(|v| *v));
    }
}
