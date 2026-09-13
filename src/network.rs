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
#[path = "vfx_stream.rs"]
mod vfx_stream;
pub use vfx_stream::Scene as VfxScene;
pub enum Update { World(cubes_protocol::world::World), Gallery(Slide), Vfx(VfxScene), Snake {session:u64, state:cubes_protocol::snake::State}, Worm {session:u64, state:cubes_protocol::worm::State} }
struct Shared {
    session: u64,
    running: bool,
    connected: bool,
    world_id: u8,
    empty_ready: bool,
    structure: Option<cubes_protocol::world::World>,
    gallery_ready: Option<Slide>,
    vfx_ready: Option<VfxScene>,
    snake_ready: Option<cubes_protocol::snake::State>,
    worm_ready: Option<cubes_protocol::worm::State>,
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
                connected: false,
                world_id: 1,
                empty_ready: false,
                structure: None,
                gallery_ready: None,
                vfx_ready: None,
                snake_ready: None,
                worm_ready: None,
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
        s.connected = false;
        s.empty_ready = false;
        s.structure = None;
        s.gallery_ready = None;
        s.vfx_ready = None;
        s.snake_ready = None;
        s.worm_ready = None;
        s.error = None;
    }
    pub fn key(&mut self, held: bool, position: [f32; 3], orientation: [f32; 3]) {
        let pressed = held && !self.held;
        self.held = held;
        let world_id = {
            let mut s = self.shared.lock().unwrap();
            s.position = position;
            s.orientation = orientation;
            if !pressed || (s.running && !s.connected) { return; }
            if s.running && s.world_id == 1 { 2 } else { 1 }
        };
        self.connect_world(world_id);
    }
    /// Explicit entry is independent of Key8's toggle and held-key state.
    pub fn connect_empty(&mut self) { self.connect_world(2); }
    fn connect_world(&mut self, world_id: u8) {
        let session = {
            let mut s = self.shared.lock().unwrap();
            let already_empty = s.running && s.connected && s.world_id == 2 && world_id == 2;
            s.world_id = world_id;
            s.empty_ready = already_empty;
            s.gallery_ready = None;
            s.vfx_ready = None;
            s.snake_ready = None;
            s.worm_ready = None;
            s.error = None;
            if s.running { return; }
            s.connected = false;
            s.running = true;
            s.session = s.session.wrapping_add(1);
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
        if core::mem::take(&mut shared.empty_ready) {
            if let Some(world) = shared.structure.clone() {
                shared.connected = true;
                return Some(Ok(Update::World(world)));
            }
        }
        if shared.world_id == 2 { return shared.error.take().map(Err); }
        if let Some(slide) = shared.gallery_ready.take() { shared.connected = true; return Some(Ok(Update::Gallery(slide))); }
        if let Some(state) = shared.snake_ready.take() { return Some(Ok(Update::Snake {session:shared.session,state})); }
        if let Some(state) = shared.worm_ready.take() { return Some(Ok(Update::Worm {session:shared.session,state})); }
        if let Some(frame) = shared.vfx_ready.take() { return Some(Ok(Update::Vfx(frame))); }
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
fn world_info_for(bytes: &[u8], world_id: u8) -> Option<usize> {
    let p = payload(bytes, 0x81)?;
    if p.len() != 12 || p[4] != world_id { return None; }
    let len = u32::from_le_bytes(p[5..9].try_into().ok()?) as usize;
    let chunks = u16::from_le_bytes(p[9..11].try_into().ok()?) as usize;
    (len >= 16 && len <= MAX_ENCODED_BYTES && chunks == len.div_ceil(CHUNK_BYTES)).then_some(len)
}
fn accept_world_chunk(bytes: &[u8], world_id: u8, encoded: &mut [u8], received: &mut [bool]) -> bool {
    let Some(p) = payload(bytes, 0x83) else { return false; };
    if p.len() < 5 || p[0] != world_id { return false; }
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
async fn receive_world(socket: &UdpSocket, shared: &Mutex<Shared>, session: u64, username: &str, world_id: u8)
    -> Result<Vec<u8>, &'static str>
{
    let mut buffer = [0;1200];
    let mut hello = vec![world_id];
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
            if let Some(len) = world_info_for(&buffer[..n], world_id) { length=Some(len); break; }
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
                accept_world_chunk(&buffer[..n], world_id, &mut encoded, &mut received);
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
    loop {
        let world_id = {
            let s = shared.lock().unwrap();
            if s.session != session { return Ok(()); }
            s.world_id
        };
        if world_id == 1 {
            stream_preview(&socket, shared, session, device, username).await?;
        } else {
            let bytes = receive_world(&socket, shared, session, username, 2).await?;
            let world = cubes_protocol::world::World::parse(&bytes).ok_or("invalid server world")?;
            {
                let mut s = shared.lock().unwrap();
                if s.session != session { return Ok(()); }
                if s.world_id == 2 { s.structure = Some(world); s.empty_ready = true; }
            }
            // Keep the existing peer alive without preview scene traffic.
            let mut heartbeat = time::Instant::now();
            while { let s=shared.lock().unwrap(); s.session == session && s.world_id == 2 } {
                time::sleep(time::Duration::from_millis(25)).await;
                if heartbeat.elapsed() >= time::Duration::from_secs(1) {
                    let mut hello = vec![2];
                    hello.extend_from_slice(username.as_bytes());
                    socket.send(&packet(1, &hello)).await.map_err(|_| "world heartbeat")?;
                    let mut buffer = [0;1200];
                    let _ = time::timeout(time::Duration::from_millis(200), socket.recv(&mut buffer)).await;
                    heartbeat = time::Instant::now();
                }
            }
        }
    }
}
async fn stream_preview(
    socket: &UdpSocket, shared: &Mutex<Shared>, session: u64,
    device: trueos::vgpu::Device, username: &str,
) -> Result<(), &'static str> {
    let mut buffer = [0u8; 1200];
    let world = receive_world(socket, shared, session, username, 1).await?;
    let mut shown = None;
    let mut vfx = vfx_stream::Stream::default();
    let mut sequence = 0u32;
    let mut missed = 0;
    loop {
        let (position, orientation) = {
            let s = shared.lock().unwrap();
            if s.session != session || s.world_id != 1 {
                return Ok(());
            }
            (s.position, s.orientation)
        };
        // receive_world already joined. Telemetry sends one welcome/scene
        // snapshot back; sending another Hello here builds an announcement backlog.
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
                    vfx.observe(&buffer[..n]);
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
                                vfx.observe(&buffer[..n]);
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
                if s.session != session || s.world_id != 1 {
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
        if let Some(gallery) = shown { vfx.service(&socket,shared,session,gallery).await?; }
        // Heartbeats recover lost announcements and keep current telemetry flowing.
        time::sleep(time::Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
#[path = "../../TRUEOS-Blueprints/apps/cubesrv/protocol.rs"]
mod server;

#[cfg(test)]
#[path = "../../TRUEOS-Blueprints/apps/cubesrv/structure.rs"]
mod structure;
#[cfg(test)]
mod tests {
    use super::*;

    fn test_shared() -> Mutex<Shared> {
        Mutex::new(Shared {session:1, running:true, connected:true, world_id:1, empty_ready:false, structure:None,
            gallery_ready:None, vfx_ready:None, snake_ready:None, worm_ready:None, error:None,
            position:[0.;3], orientation:[0.,0.,-1.]})
    }

    #[test]
    fn world_two_welcome_requires_matching_id_and_geometry() {
        let empty=server::welcome(1,2,464,1,0);
        assert_eq!(world_info_for(&empty,2),Some(464));
        assert_eq!(world_info_for(&empty,1),None);
        assert_eq!(world_info_for(&server::welcome(1,1,16,1,0),2),None);
        assert_eq!(world_info_for(&server::welcome(1,2,0,0,0),2),None);
        assert_eq!(world_info_for(&server::welcome(1,2,0,1,0),2),None);
    }

    #[test]
    fn preview_empty_preview_reuses_one_udp_socket() {
        runtime::current_thread_net().build().unwrap().block_on(async {
            let server_socket=UdpSocket::bind((Ipv4Addr::LOCALHOST,0)).await.unwrap();
            let client=UdpSocket::bind((Ipv4Addr::LOCALHOST,0)).await.unwrap();
            client.connect(server_socket.local_addr().unwrap()).await.unwrap();
            let original_peer=client.local_addr().unwrap();
            let shared=test_shared();
            let bytes=include_bytes!("../Cube/lvl27/world_01_sky.cubes");
            let responder=trueos::tokio::spawn(async move {
                let mut buf=[0;1200];
                let mut selected=1;
                loop {
                    let (n,peer)=server_socket.recv_from(&mut buf).await.unwrap();
                    assert_eq!(peer,original_peer);
                    match server::decode(&buf[..n]).unwrap() {
                        server::ClientPacket::Hello {world_id,..} => {
                            selected=world_id;
                            let len=if selected==2 {structure::world().encode().len()} else {bytes.len()};
                            // A delayed welcome from the previous world must not win.
                            let stale=server::welcome(1,if selected==2 {1} else {2},0,0,0);
                            server_socket.send_to(&stale,peer).await.unwrap();
                            server_socket.send_to(&server::welcome(1,selected,len,len.div_ceil(CHUNK_BYTES) as u16,0),peer).await.unwrap();
                        }
                        server::ClientPacket::WorldRequest {chunk} => {
                            let world2=structure::world().encode();
                            let source=if selected==2 {world2.as_slice()} else {bytes.as_slice()};
                            let packet=server::blob_chunk(server::BlobKind::World,selected,chunk,source).unwrap();
                            server_socket.send_to(&packet,peer).await.unwrap();
                        }
                        _ => panic!("unexpected packet"),
                    }
                }
            });
            for id in [2,1,2,1,2,1] {
                let world=receive_world(&client,&shared,1,"test",id).await.unwrap();
                if id==2 {
                    let decoded=cubes_protocol::world::World::parse(&world).unwrap();
                    assert_eq!(decoded,structure::world());
                    assert_eq!(decoded.cubes.len(),27);
                    assert_eq!(decoded.spawn,[0,576,0]);
                    assert!(decoded.cubes.iter().all(|c|c.side==384 && c.material==0));
                } else {assert_eq!(world,bytes);}
            }
            responder.abort();
        });
    }


    #[test]
    fn worm_wire_has_nine_slots_and_independent_messages() {
        let mut worm=crate::server_worm::Worm::new(9,4);
        assert_eq!(server::decode(&packet(9,&[])),Ok(server::ClientPacket::WormRequest));
        assert!(server::decode(&packet(9,&[0])).is_err());
        let snapshot=server::worm_snapshot(worm.state);
        assert_eq!(snapshot.len(),75);
        assert_eq!(payload(&snapshot,0x8d).and_then(cubes_protocol::worm::State::parse),Some(worm.state));
        assert!(payload(&snapshot,0x8b).is_none());
        let step=worm.step(0);
        let update=server::worm_step(step);
        assert_eq!(update.len(),27);
        assert_eq!(payload(&update,0x8e).and_then(cubes_protocol::worm::Step::parse),Some(step));
    }
    #[test]
    fn snake_wire_uses_one_tail_replacement_and_explicit_resync() {
        let mut snake=crate::server_snake::Snake::new(9,4);
        assert_eq!(server::decode(&packet(8,&[])),Ok(server::ClientPacket::SnakeRequest));
        assert!(server::decode(&packet(8,&[0])).is_err());
        let snapshot=server::snake_snapshot(snake.state);
        assert_eq!(snapshot.len(),51);
        assert_eq!(payload(&snapshot,0x8b).and_then(cubes_protocol::snake::State::parse),Some(snake.state));
        let step=snake.step(0);
        let update=server::snake_step(step);
        assert_eq!(update.len(),27);
        assert_eq!(payload(&update,0x8c).and_then(cubes_protocol::snake::Step::parse),Some(step));
    }
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
        assert_eq!(world_info_for(&packet(0x81,&welcome),1),Some(1030));
        welcome[4]=27;
        assert_eq!(world_info_for(&packet(0x81,&welcome),1),None);
        let mut encoded = vec![0;1030];
        let mut received = vec![false;2];
        let chunk = |id, index: u16, count: u16, size| {
            let mut body = vec![id];
            body.extend_from_slice(&index.to_le_bytes());
            body.extend_from_slice(&count.to_le_bytes());
            body.extend_from_slice(&vec![7;size]);
            packet(0x83,&body)
        };
        assert!(!accept_world_chunk(&chunk(27,1,2,6),1,&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(2,1,2,6),1,&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,2,6),2,&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,3,6),1,&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,2,5),1,&mut encoded,&mut received));
        assert!(accept_world_chunk(&chunk(1,1,2,6),1,&mut encoded,&mut received));
        assert!(!accept_world_chunk(&chunk(1,1,2,6),1,&mut encoded,&mut received));
        assert!(accept_world_chunk(&chunk(1,0,2,1024),1,&mut encoded,&mut received));
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

}
