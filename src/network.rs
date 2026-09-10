//! Key-8 localhost client for the minimal cubesrv UDP world stream.

use alloc::{sync::Arc, vec, vec::Vec};
use core::net::Ipv4Addr;
use std::sync::Mutex;
use trueos::{runtime, time, tokio::net::UdpSocket};

const SERVER_PORT: u16 = 30_018;
const WORLD_ID: u8 = 27;
const MAGIC: &[u8; 4] = b"CUB1";
const VERSION: u8 = 1;
const HEADER: usize = 8;
const TELEMETRY: u8 = 2;
const WORLD_REQUEST: u8 = 3;
const WELCOME: u8 = 0x81;
const WORLD_CHUNK: u8 = 0x83;
const MAX_DATAGRAM: usize = 1_200;
const CHUNK_BYTES: usize = 1_024;
const ATTEMPTS: usize = 4;
const TIMEOUT_MS: u64 = 300;

enum State {
    Idle,
    Fetching,
    Ready(Vec<u8>),
    Failed(&'static str),
}

pub struct Client {
    shared: Arc<Mutex<State>>,
    held: bool,
}

impl Client {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(State::Idle)),
            held: false,
        }
    }

    /// Start one non-blocking localhost fetch on the Key-8 rising edge.
    pub fn key(&mut self, held: bool, position: [f32; 3], orientation: [f32; 3]) {
        let pressed = held && !self.held;
        self.held = held;
        if !pressed {
            return;
        }
        {
            let mut state = self.shared.lock().unwrap();
            if matches!(*state, State::Fetching) {
                return;
            }
            *state = State::Fetching;
        }
        let shared = self.shared.clone();
        if trueos::worker::spawn(move || {
            let result = fetch(position, orientation);
            *shared.lock().unwrap() = match result {
                Ok(bytes) => State::Ready(bytes),
                Err(error) => State::Failed(error),
            };
        })
        .is_err()
        {
            *self.shared.lock().unwrap() = State::Failed("worker unavailable");
        }
    }

    pub fn take_ready(&mut self) -> Option<Result<Vec<u8>, &'static str>> {
        let mut state = self.shared.lock().unwrap();
        match core::mem::replace(&mut *state, State::Idle) {
            State::Ready(bytes) => Some(Ok(bytes)),
            State::Failed(error) => Some(Err(error)),
            other => {
                *state = other;
                None
            }
        }
    }
}

fn packet(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.push(kind);
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn payload(bytes: &[u8], kind: u8) -> Option<&[u8]> {
    if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[4] != VERSION || bytes[5] != kind {
        return None;
    }
    let len = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
    (bytes.len() == HEADER + len).then_some(&bytes[HEADER..])
}

fn fetch(position: [f32; 3], orientation: [f32; 3]) -> Result<Vec<u8>, &'static str> {
    let runtime = runtime::current_thread_net()
        .build()
        .map_err(|_| "network runtime")?;
    runtime.block_on(fetch_async(position, orientation))
}

async fn fetch_async(position: [f32; 3], orientation: [f32; 3]) -> Result<Vec<u8>, &'static str> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "udp bind")?;
    socket
        .connect((Ipv4Addr::LOCALHOST, SERVER_PORT))
        .await
        .map_err(|_| "cubesrv connect")?;

    let mut datagram = [0_u8; MAX_DATAGRAM];
    let mut telemetry = 1_u32.to_le_bytes().to_vec();
    telemetry.push(WORLD_ID);
    for value in position.into_iter().chain(orientation) {
        telemetry.extend_from_slice(&value.to_le_bytes());
    }
    let welcome = request(
        &socket,
        &packet(TELEMETRY, &telemetry),
        WELCOME,
        &mut datagram,
    )
    .await?;
    if welcome.len() != 12 || welcome[4] != WORLD_ID {
        return Err("cubesrv welcome");
    }
    let total_bytes = u32::from_le_bytes(welcome[5..9].try_into().unwrap()) as usize;
    let total_chunks = u16::from_le_bytes(welcome[9..11].try_into().unwrap()) as usize;
    if total_bytes < 16 || total_chunks == 0 || total_chunks != total_bytes.div_ceil(CHUNK_BYTES) {
        return Err("cubesrv world size");
    }

    let mut world = vec![0_u8; total_bytes];
    for chunk in 0..total_chunks {
        let request_packet = packet(WORLD_REQUEST, &(chunk as u16).to_le_bytes());
        let part = request(&socket, &request_packet, WORLD_CHUNK, &mut datagram).await?;
        if part.len() < 5
            || part[0] != WORLD_ID
            || u16::from_le_bytes(part[1..3].try_into().unwrap()) as usize != chunk
            || u16::from_le_bytes(part[3..5].try_into().unwrap()) as usize != total_chunks
        {
            return Err("cubesrv world chunk");
        }
        let start = chunk * CHUNK_BYTES;
        let expected = (total_bytes - start).min(CHUNK_BYTES);
        if part.len() != 5 + expected {
            return Err("cubesrv world chunk length");
        }
        world[start..start + expected].copy_from_slice(&part[5..]);
    }
    Ok(world)
}

async fn request(
    socket: &UdpSocket,
    outgoing: &[u8],
    expected_kind: u8,
    incoming: &mut [u8; MAX_DATAGRAM],
) -> Result<Vec<u8>, &'static str> {
    for _ in 0..ATTEMPTS {
        socket.send(outgoing).await.map_err(|_| "cubesrv send")?;
        let received = time::timeout(
            time::Duration::from_millis(TIMEOUT_MS),
            socket.recv(incoming),
        )
        .await;
        if let Ok(Ok(length)) = received
            && let Some(payload) = payload(&incoming[..length], expected_kind)
        {
            return Ok(payload.to_vec());
        }
    }
    Err("cubesrv unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_header_round_trips() {
        let encoded = packet(TELEMETRY, &[WORLD_ID]);
        assert_eq!(payload(&encoded, TELEMETRY), Some(&[WORLD_ID][..]));
        assert_eq!(payload(&encoded, WELCOME), None);
    }
}
