//! Persistent Key-8 gallery and sparse-frame connection. Transfers are pinned
//! to revisions; incomplete frames never replace the displayed scene.
use alloc::{sync::Arc, vec, vec::Vec};
use core::net::Ipv4Addr;
use std::sync::Mutex;
use trueos::{runtime, time, tokio::net::UdpSocket, vmedia};

const SERVER_PORT: u16 = 30_018;
const MAGIC: &[u8; 4] = b"CUB1";
const HEADER: usize = 8;
const INFO: u8 = 0x85;
const CHUNK: u8 = 0x86;
const HOLY_INFO: u8 = 0x87;
const HOLY_CHUNK: u8 = 0x88;
const CHUNK_BYTES: usize = 1024;
const MAX_ENCODED_BYTES: usize = 4 * 1024 * 1024;
const WINDOW: usize = 32;

pub struct Slide {
    pub world: Vec<u8>,
    pub session: u64,
    pub texture: vmedia::RetainedTexture,
    pub palette: Vec<u16>,
    pub layout: crate::slideshow::contract::Layout,
    pub revision: u32,
    pub spawn: [f32; 3],
}
#[derive(Debug)]
pub struct HolyFrame {
    pub session: u64,
    pub gallery_revision: u32,
    pub revision: u32,
    pub index: u8,
    pub cubes: Vec<cubes_protocol::holy::Pixel>,
}
pub enum Update { Gallery(Slide), Holy(HolyFrame) }
struct Shared {
    session: u64,
    running: bool,
    gallery_ready: Option<Slide>,
    holy_ready: Option<HolyFrame>,
    error: Option<&'static str>,
    position: [f32; 3],
    orientation: [f32; 3],
}
pub struct Client {
    device: trueos::vgpu::Device,
    username: &'static str,
    shared: Arc<Mutex<Shared>>,
    held: bool,
}
impl Client {
    pub fn new(device: trueos::vgpu::Device, username: &'static str) -> Self {
        Self {
            device,
            username,
            shared: Arc::new(Mutex::new(Shared {
                session: 0,
                running: false,
                gallery_ready: None,
                holy_ready: None,
                error: None,
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
        s.gallery_ready = None;
        s.holy_ready = None;
        s.error = None;
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
            s.gallery_ready = None;
            s.holy_ready = None;
            s.error = None;
            s.session
        };
        let shared = self.shared.clone();
        let device = self.device;
        let username = self.username;
        if trueos::worker::spawn(move || {
            let result = runtime::current_thread_net()
                .build()
                .map_err(|_| "network runtime")
                .and_then(|rt| rt.block_on(stream(&shared, session, device, username)));
            let mut s = shared.lock().unwrap();
            if s.session == session {
                s.running = false;
                if let Err(error) = result {
                    s.error = Some(error);
                }
            }
        })
        .is_err()
        {
            let mut s = self.shared.lock().unwrap();
            s.running = false;
            s.error = Some("worker unavailable");
        }
    }
    pub fn take_ready(&mut self) -> Option<Result<Update, &'static str>> {
        let mut shared = self.shared.lock().unwrap();
        if let Some(slide) = shared.gallery_ready.take() { return Some(Ok(Update::Gallery(slide))); }
        if let Some(frame) = shared.holy_ready.take() { return Some(Ok(Update::Holy(frame))); }
        shared.error.take().map(Err)
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
fn holy_info(bytes: &[u8]) -> Option<(u32, u32, u8, usize)> {
    let p = payload(bytes, HOLY_INFO)?;
    if p.len() != 15 { return None; }
    let size = u16::from_le_bytes(p[13..15].try_into().unwrap()) as usize;
    if size > cubes_protocol::holy::WIDTH as usize * cubes_protocol::holy::HEIGHT as usize * 3
        || size % 3 != 0 { return None; }
    Some((u32::from_le_bytes(p[4..8].try_into().unwrap()),
        u32::from_le_bytes(p[8..12].try_into().unwrap()), p[12], size))
}
fn accept_holy_chunk(bytes: &[u8], revision: u32, frame: u8,
    encoded: &mut [u8], received: &mut [bool]) -> bool
{
    let Some(p) = payload(bytes, HOLY_CHUNK) else { return false; };
    if p.len() < 7 || u32::from_le_bytes(p[..4].try_into().unwrap()) != revision || p[4] != frame {
        return false;
    }
    let index = u16::from_le_bytes(p[5..7].try_into().unwrap()) as usize;
    if index >= received.len() || received[index] { return false; }
    let start = index * CHUNK_BYTES;
    let end = (start + CHUNK_BYTES).min(encoded.len());
    if p.len() != 7 + end - start { return false; }
    encoded[start..end].copy_from_slice(&p[7..]);
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
fn palette_from_rgba(
    layout: crate::slideshow::contract::Layout, width: u32, height: u32,
    stride: u32, rgba: &[u8],
) -> Option<Vec<u16>> {
    if [width,height] != layout.extent() || stride < width.checked_mul(4)? { return None; }
    let row = (height as usize-1).checked_mul(stride as usize)?;
    let colors = rgba.get(row+4..row+(width as usize).checked_mul(4)?)?;
    Some(colors.chunks_exact(4).take(256).map(|p| {
        0x8000 | ((u16::from(p[0])*31+127)/255)
            | (((u16::from(p[1])*31+127)/255)<<5)
            | (((u16::from(p[2])*31+127)/255)<<10)
    }).collect())
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
fn world_info(bytes: &[u8]) -> Option<usize> {
    let p = payload(bytes, 0x81)?;
    if p.len() != 12 || p[4] != 1 { return None; }
    let len = u32::from_le_bytes(p[5..9].try_into().ok()?) as usize;
    let chunks = u16::from_le_bytes(p[9..11].try_into().ok()?) as usize;
    (len >= 16 && len <= MAX_ENCODED_BYTES && chunks == len.div_ceil(CHUNK_BYTES)).then_some(len)
}
fn accept_world_chunk(bytes: &[u8], encoded: &mut [u8], received: &mut [bool]) -> bool {
    let Some(p) = payload(bytes, 0x83) else { return false; };
    if p.len() < 5 || p[0] != 1 { return false; }
    let index = u16::from_le_bytes([p[1],p[2]]) as usize;
    if u16::from_le_bytes([p[3],p[4]]) as usize != received.len()
        || index >= received.len() || received[index] { return false; }
    let start = index*CHUNK_BYTES;
    let end = (start+CHUNK_BYTES).min(encoded.len());
    if p.len() != 5+end-start { return false; }
    encoded[start..end].copy_from_slice(&p[5..]);
    received[index] = true;
    true
}
fn world_welcome_error(gallery_seen: bool, rejected_welcome: bool) -> &'static str {
    if rejected_welcome { "incompatible world welcome; rebuild CubeSrv and Cubes together" }
    else if gallery_seen { "gallery received without world welcome; rebuild and reload CubeSrv with world1 support" }
    else { "world welcome timeout: no valid gallery or world announcement received" }
}
async fn receive_world(socket: &UdpSocket, shared: &Mutex<Shared>, session: u64, username: &str)
    -> Result<Vec<u8>, &'static str>
{
    let mut buffer = [0;1200];
    let mut hello = vec![1];
    hello.extend_from_slice(username.as_bytes());
    let mut length = None;
    let mut gallery_seen = false;
    let mut rejected_welcome = false;
    for _ in 0..10 {
        if shared.lock().unwrap().session != session { return Err("world transfer cancelled"); }
        socket.send(&packet(1, &hello)).await.map_err(|_| "world hello")?;
        let deadline = time::Instant::now()+time::Duration::from_secs(1);
        while time::Instant::now() < deadline {
            let Ok(Ok(n)) = time::timeout(deadline.saturating_duration_since(time::Instant::now()),
                socket.recv(&mut buffer)).await else { break; };
            if let Some(len) = world_info(&buffer[..n]) { length=Some(len); break; }
            gallery_seen |= info(&buffer[..n]).is_some();
            rejected_welcome |= payload(&buffer[..n], 0x81).is_some();
        }
        if length.is_some() { break; }
    }
    let mut encoded = vec![0;length.ok_or(world_welcome_error(gallery_seen, rejected_welcome))?];
    let chunks = encoded.len().div_ceil(CHUNK_BYTES);
    let mut received = vec![false;chunks];
    for start in (0..chunks).step_by(WINDOW) {
        let end = (start+WINDOW).min(chunks);
        for _ in 0..8 {
            if shared.lock().unwrap().session != session { return Err("world transfer cancelled"); }
            for index in start..end {
                if !received[index] {
                    socket.send(&packet(3, &(index as u16).to_le_bytes())).await.map_err(|_| "world request")?;
                }
            }
            let deadline = time::Instant::now()+time::Duration::from_millis(200);
            while !received[start..end].iter().all(|v| *v) && time::Instant::now() < deadline {
                let Ok(Ok(n)) = time::timeout(deadline.saturating_duration_since(time::Instant::now()),
                    socket.recv(&mut buffer)).await else { break; };
                accept_world_chunk(&buffer[..n], &mut encoded, &mut received);
            }
            if received[start..end].iter().all(|v| *v) { break; }
        }
        if !received[start..end].iter().all(|v| *v) { return Err("world chunks timeout"); }
    }
    Ok(encoded)
}
async fn stream(
    shared: &Mutex<Shared>,
    session: u64,
    device: trueos::vgpu::Device,
    username: &str,
) -> Result<(), &'static str> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "udp bind")?;
    socket
        .connect((Ipv4Addr::LOCALHOST, SERVER_PORT))
        .await
        .map_err(|_| "cubesrv connect")?;
    let mut buffer = [0u8; 1200];
    let world = receive_world(&socket, shared, session, username).await?;
    let mut shown = None;
    let mut shown_holy = None;
    let mut pending_holy = None;
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
        let mut hello = vec![1];
        hello.extend_from_slice(username.as_bytes());
        socket.send(&packet(1, &hello)).await.map_err(|_| "cubesrv username")?;
        sequence = sequence.wrapping_add(1);
        let mut body = sequence.to_le_bytes().to_vec();
        body.push(1); // Single shared session; the server chooses the slide.
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
                    if let Some(value) = holy_info(&buffer[..n]) {
                        pending_holy = Some(value);
                    }
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
                                if let Some(value) = holy_info(&buffer[..n]) {
                                    pending_holy = Some(value);
                                }
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
                let (layout, png) = crate::slideshow::contract::Layout::parse(&encoded)
                    .ok_or("gallery package contract")?;
                // Extract the small RGB palette once per gallery revision. Frame
                // messages remain sparse palette indices; GPU cubes need RGB555 flags.
                let image = time::timeout(time::Duration::from_secs(10),
                    vmedia::decode(vmedia::ImageFormat::Png, png)).await
                    .map_err(|_| "gallery palette timeout")?
                    .map_err(|_| "gallery palette decode")?;
                let palette = palette_from_rgba(layout, image.info.width, image.info.height,
                    image.info.stride_bytes, &image.rgba).ok_or("gallery palette dimensions")?;
                drop(image);
                let texture = decode_texture(device, png).await?;
                if [texture.info().width, texture.info().height] != layout.extent() {
                    return Err("gallery atlas dimensions");
                }
                let mut s = shared.lock().unwrap();
                if s.session != session {
                    return Ok(());
                }
                s.gallery_ready = Some(Slide {
                    world: world.clone(),
                    session,
                    texture,
                    palette,
                    layout,
                    revision,
                    spawn,
                });
                shown = Some(revision);
            }
        }
        if let Some((gallery_revision, holy_revision, frame, encoded_len)) = pending_holy.take() {
            if shown != Some(gallery_revision) {
                pending_holy = Some((gallery_revision, holy_revision, frame, encoded_len));
            } else if shown_holy != Some((holy_revision, frame)) {
                let mut encoded = vec![0; encoded_len];
                let chunks = encoded_len.div_ceil(CHUNK_BYTES);
                let mut received = vec![false; chunks];
                let mut failed = false;
                for _ in 0..8 {
                    if shared.lock().unwrap().session != session { return Ok(()); }
                    for chunk in 0..chunks {
                        if !received[chunk] {
                            let mut body = holy_revision.to_le_bytes().to_vec();
                            body.push(frame);
                            body.extend_from_slice(&(chunk as u16).to_le_bytes());
                            socket.send(&packet(6, &body)).await.map_err(|_| "cubesrv holy send")?;
                        }
                    }
                    let deadline = time::Instant::now() + time::Duration::from_millis(200);
                    while !received.iter().all(|value| *value) {
                        match time::timeout(
                            deadline.saturating_duration_since(time::Instant::now()),
                            socket.recv(&mut buffer),
                        ).await {
                            Ok(Ok(n)) => {
                                if let Some(value) = holy_info(&buffer[..n]) { pending_holy = Some(value); }
                                accept_holy_chunk(&buffer[..n], holy_revision, frame, &mut encoded, &mut received);
                            }
                            _ => break,
                        }
                        if time::Instant::now() >= deadline { break; }
                    }
                    if received.iter().all(|value| *value) { break; }
                }
                if !received.iter().all(|value| *value) { failed = true; }
                if !failed {
                    let cubes = cubes_protocol::holy::decode_frame(&encoded)
                        .ok_or("holy frame contract")?.collect();
                    let mut s = shared.lock().unwrap();
                    if s.session != session { return Ok(()); }
                    s.holy_ready = Some(HolyFrame {
                        session, gallery_revision, revision: holy_revision, index: frame, cubes,
                    });
                    shown_holy = Some((holy_revision, frame));
                }
            }
        }
        // Heartbeats recover lost announcements and keep current telemetry flowing.
        time::sleep(time::Duration::from_millis(cubes_protocol::holy::PERIOD_MS as u64)).await;
    }
}

#[cfg(test)]
#[path = "../../TRUEOS-Blueprints/apps/cubesrv/protocol.rs"]
mod server;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_failure_distinguishes_old_gallery_server_from_silence() {
        assert!(world_welcome_error(true,false).contains("rebuild and reload CubeSrv"));
        assert!(world_welcome_error(true,true).contains("incompatible world welcome"));
        assert!(world_welcome_error(false,false).contains("no valid gallery or world"));
    }
    #[test]
    fn world_transfer_validates_identity_counts_duplicates_and_partial_tail() {
        let mut welcome = vec![0;12];
        welcome[4] = 1;
        welcome[5..9].copy_from_slice(&1030u32.to_le_bytes());
        welcome[9..11].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(world_info(&packet(0x81,&welcome)),Some(1030));
        welcome[4]=27;
        assert_eq!(world_info(&packet(0x81,&welcome)),None);
        let mut encoded = vec![0;1030];
        let mut received = vec![false;2];
        let chunk = |id, index: u16, count: u16, size| {
            let mut body = vec![id];
            body.extend_from_slice(&index.to_le_bytes());
            body.extend_from_slice(&count.to_le_bytes());
            body.extend_from_slice(&vec![7;size]);
            packet(0x83,&body)
        };
        assert!(!accept_world_chunk(&chunk(27,1,2,6),&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,3,6),&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,2,5),&mut encoded,&mut received));
        assert!(accept_world_chunk(&chunk(1,1,2,6),&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,2,6),&mut encoded,&mut received));
        assert!(accept_world_chunk(&chunk(1,0,2,1024),&mut encoded,&mut received));
        assert!(received.iter().all(|v| *v));
        assert!(encoded.iter().all(|v| *v == 7));
    }
    #[test]
    fn atlas_palette_uses_the_same_rgb555_rounding_as_placed_assets() {
        let layout = crate::slideshow::contract::Layout { tiers:[1;6] };
        let [width,height] = layout.extent();
        let stride=width*4;
        let mut rgba=vec![0; (stride*height) as usize];
        let start=((height-1)*stride+4) as usize;
        for (i,color) in [[255,0,0,255],[0,255,0,255],[0,0,255,255],
            [255,255,255,255],[252,150,17,255]].iter().enumerate() {
            rgba[start+i*4..start+i*4+4].copy_from_slice(color);
        }
        let palette=palette_from_rgba(layout,width,height,stride,&rgba).unwrap();
        assert_eq!(&palette[..4],&[0x801f,0x83e0,0xfc00,0xffff]);
        assert_eq!(palette[4],0x8000|31|(18<<5)|(2<<10));
        assert!(palette_from_rgba(layout,width,height,stride,&rgba[..start]).is_none());
        assert!(palette_from_rgba(layout,width+1,height,stride,&rgba).is_none());
    }
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
    fn gallery_package_checks_version_tiers_dimensions_and_c1_placement() {
        let source = include_bytes!("../../TRUEOS-Blueprints/apps/cubesrv/slides/gallery.cga");
        let (layout, _) = crate::slideshow::contract::Layout::parse(source).unwrap();
        assert_eq!(layout.extent(), [layout.tile()*3, layout.tile()*2+1]);
        for (offset,value) in [(0,0),(4,1),(4,2),(4,3),(4,4),(4,5),(4,6),(4,7),(5,5),(6,0),(6,4),(12,0),(12,2),(13,0),(14,1),(15,0),(35,0)] {
            let mut bad = source.to_vec(); bad[offset]=value;
            assert!(crate::slideshow::contract::Layout::parse(&bad).is_none());
        }
        assert!(crate::slideshow::contract::Layout::parse(&source[..32]).is_none());
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
    #[test]
    fn sparse_holy_frames_are_revision_and_frame_pinned() {
        let source = vec![1, 2, 3, 47, 47, 4, 9, 8, 7];
        assert_eq!(holy_info(&server::holy_info(7, 11, 12, 3, source.len())),
            Some((11, 12, 3, source.len())));
        let mut output = vec![0; source.len()];
        let mut received = vec![false; 1];
        let packet = server::holy_chunk(12, 3, 0, &source).unwrap();
        assert!(!accept_holy_chunk(&packet, 11, 3, &mut output, &mut received));
        assert!(!accept_holy_chunk(&packet, 12, 2, &mut output, &mut received));
        assert!(accept_holy_chunk(&packet, 12, 3, &mut output, &mut received));
        assert_eq!(output, source);
        assert_eq!(cubes_protocol::holy::decode_frame(&output).unwrap().collect::<Vec<_>>(), vec![
            cubes_protocol::holy::Pixel { x: 1, y: 2, palette: 3 },
            cubes_protocol::holy::Pixel { x: 47, y: 47, palette: 4 },
            cubes_protocol::holy::Pixel { x: 9, y: 8, palette: 7 },
        ]);
    }
}
