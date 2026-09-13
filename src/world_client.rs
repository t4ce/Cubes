//! Key5 downloads complete server-owned worlds off the frame thread.
use crate::{cube_format, orchard, platform_lod};
use alloc::{format, sync::Arc, vec, vec::Vec};
use cubes_protocol::worlds::{self, World};
use std::sync::Mutex;
pub struct Page {
    pub bytes: Vec<u8>,
    pub asset: orchard::Asset,
    pub metadata: Arc<platform_lod::Metadata>,
}
pub struct Reply {
    pub index: usize,
    pub result: Result<Page, &'static str>,
}
struct Shared {
    generation: u64,
    reply: Option<Reply>,
}
pub struct Client {
    shared: Arc<Mutex<Shared>>,
}
impl Client {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                generation: 0,
                reply: None,
            })),
        }
    }
    pub fn cancel(&self) {
        let mut s = self.shared.lock().unwrap();
        s.generation = s.generation.wrapping_add(1);
        s.reply = None;
    }
    pub fn take_reply(&self) -> Option<Reply> {
        self.shared.lock().unwrap().reply.take()
    }
    pub fn request(&self, index: usize) {
        self.request_at(index, format!("http://127.0.0.1:18/worlds/{}", index + 1));
    }
    pub(crate) fn request_at(&self, index: usize, url: alloc::string::String) {
        self.cancel();
        let generation = self.shared.lock().unwrap().generation;
        let shared = self.shared.clone();
        if trueos::worker::spawn(move || {
            let result = trueos::runtime::current_thread_net()
                .build()
                .map_err(|_| "world network runtime")
                .and_then(|rt| rt.block_on(fetch(index, &url)));
            let mut s = shared.lock().unwrap();
            if s.generation == generation {
                s.reply = Some(Reply { index, result });
            }
        })
        .is_err()
        {
            self.shared.lock().unwrap().reply = Some(Reply {
                index,
                result: Err("world worker unavailable"),
            });
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(crate) async fn fetch(index: usize, url: &str) -> Result<Page, &'static str> {
    if index >= worlds::COUNT {
        return Err("unknown world");
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(core::time::Duration::from_secs(15))
        .build()
        .map_err(|_| "world HTTP client")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "cubesrv unavailable; press Key5 to retry")?;
    if !response.status().is_success() {
        return Err("server world unavailable");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "world transfer interrupted")?
    {
        if bytes.len() + chunk.len() > worlds::MAX_BYTES {
            return Err("world response too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    decode(index, &bytes)
}
pub(crate) fn decode(index: usize, bytes: &[u8]) -> Result<Page, &'static str> {
    if index >= worlds::COUNT || bytes.len() > worlds::MAX_BYTES {
        return Err("world response bounds");
    }
    let world: World = serde_json::from_slice(bytes).map_err(|_| "invalid world response")?;
    if world.id as usize != index + 1 || world.platforms.filename != worlds::NAMES[index] {
        return Err("world identity mismatch");
    }
    let asset = orchard::decode_world(worlds::NAMES[index], &world.cubes)?;
    let records: Vec<_> = cube_format::cubes(&world.cubes).collect();
    if world.platforms.decoded != records.len() || world.platforms.hulls.len() >= 256 {
        return Err("world LOD count mismatch");
    }
    let unit = f32::from_le_bytes(world.cubes[12..16].try_into().unwrap());
    let mut owners = vec![0; records.len()];
    let mut hulls = Vec::new();
    for (id, h) in world.platforms.hulls.into_iter().enumerate() {
        if !h.side.is_finite()
            || h.side <= 0.
            || (0..3).any(|a| {
                !h.lo[a].is_finite()
                    || !h.hi[a].is_finite()
                    || !h.center[a].is_finite()
                    || h.lo[a] >= h.hi[a]
                    || h.center[a] != (h.lo[a] + h.hi[a]) * 0.5
                    || h.center[a] - h.side * 0.5 > h.lo[a] - 1.
                    || h.center[a] + h.side * 0.5 < h.hi[a] + 1.
            })
        {
            return Err("invalid platform bounds");
        }
        for [start, end] in h.ranges {
            if start >= end || end > records.len() {
                return Err("invalid platform range");
            }
            for i in start..end {
                let r = records[i];
                if owners[i] != 0
                    || r.part != 0
                    || (0..3).any(|a| {
                        (r.origin[a] as f32) < h.lo[a] || (r.origin[a] + r.side) as f32 > h.hi[a]
                    })
                {
                    return Err("invalid platform ownership");
                }
                owners[i] = id as u8 + 1;
            }
        }
        let flags = orchard::CUSTOM_RGB555
            | (0..3)
                .map(|a| ((h.rgb[a] as u32 * 31 + 127) / 255) << (a * 5))
                .sum::<u32>();
        hulls.push(platform_lod::Hull {
            cube: orchard::Cube {
                center: [h.center[0] * unit, h.center[1] * unit, -h.center[2] * unit],
                scale: h.side * unit * 0.5,
                flags,
            },
            lo: [h.lo[0] * unit, h.lo[1] * unit, -h.hi[2] * unit],
            hi: [h.hi[0] * unit, h.hi[1] * unit, -h.lo[2] * unit],
        });
    }
    Ok(Page {
        bytes: world.cubes,
        asset,
        metadata: Arc::new(platform_lod::Metadata { hulls, owners }),
    })
}
