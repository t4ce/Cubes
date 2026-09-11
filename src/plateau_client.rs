//! Profile IO stays off the frame thread. Only acknowledged snapshots are durable.
use crate::plateau::{self, Create, Profile, Save};
use alloc::{format, sync::Arc};
use std::sync::Mutex;

pub enum Command { Load, Create(u8), Save(Save), Delete }
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind { Load, Save, Delete }
pub struct Reply { pub kind: Kind, pub result: Result<Option<Profile>, &'static str> }
struct Shared { busy: bool, reply: Option<Reply> }
pub struct Client { shared: Arc<Mutex<Shared>>, username: &'static str }
impl Client {
    pub fn new(username: &'static str) -> Self {
        Self { shared: Arc::new(Mutex::new(Shared { busy: false, reply: None })), username }
    }
    pub fn busy(&self) -> bool { self.shared.lock().unwrap().busy }
    pub fn take_reply(&self) -> Option<Reply> { self.shared.lock().unwrap().reply.take() }
    pub fn request(&self, command: Command) -> bool {
        let mut state = self.shared.lock().unwrap();
        if state.busy || state.reply.is_some() { return false; }
        state.busy = true;
        drop(state);
        let shared = self.shared.clone();
        let username = self.username;
        let kind = match &command { Command::Save(_) => Kind::Save, Command::Delete => Kind::Delete, _ => Kind::Load };
        if trueos::worker::spawn(move || {
            let result = trueos::runtime::current_thread_net().build().map_err(|_| "profile runtime")
                .and_then(|rt| rt.block_on(exchange(username, command)));
            let mut state = shared.lock().unwrap();
            state.reply = Some(Reply { kind, result }); state.busy = false;
        }).is_err() {
            let mut state = self.shared.lock().unwrap(); state.busy = false;
            state.reply = Some(Reply { kind, result: Err("profile worker unavailable") });
        }
        true
    }
}
async fn exchange(username: &str, command: Command) -> Result<Option<Profile>, &'static str> {
    let url = format!("http://127.0.0.1:18/plateau/{username}");
    exchange_at(username, command, &url).await
}
pub(crate) async fn exchange_at(username: &str, command: Command, url: &str) -> Result<Option<Profile>, &'static str> {
    if !plateau::valid_username(username) { return Err("invalid username"); }
    let client = reqwest::Client::builder().no_proxy().timeout(core::time::Duration::from_secs(15))
        .build().map_err(|_| "profile HTTP client")?;
    let request = match command {
        Command::Load => client.get(url),
        Command::Create(theme) => client.post(url).json(&Create { theme }),
        Command::Save(ref save) => client.put(url).json(save),
        Command::Delete => client.delete(url),
    };
    let mut response = request.send().await.map_err(|_| "cubesrv unavailable; press Key4 to retry")?;
    if response.status() == 404 && matches!(command, Command::Load) { return Ok(None); }
    if response.status() == 204 && matches!(command, Command::Delete) { return Ok(None); }
    if response.status() == 409 { return Err("profile changed on server; leave and reenter Key4 to reload"); }
    if !response.status().is_success() { return Err("profile rejected or filesystem unavailable; press Key4 to retry"); }
    let mut bytes = alloc::vec::Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "profile download failed")? {
        if bytes.len() + chunk.len() > plateau::MAX_PROFILE_BYTES { return Err("profile too large"); }
        bytes.extend_from_slice(&chunk);
    }
    let profile: Profile = serde_json::from_slice(&bytes).map_err(|_| "invalid profile response")?;
    if !profile.valid(username) { return Err("invalid profile geometry"); }
    Ok(Some(profile))
}
