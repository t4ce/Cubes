//! Revision-cached compressed assets and independently timed six-slot snapshots.
use super::{packet,payload,Shared};
use alloc::{collections::BTreeMap,sync::Arc,vec,vec::Vec};
use std::sync::Mutex;
use trueos::{time,tokio::net::UdpSocket};
use cubes_protocol::vfx::{self,Sequence};

#[derive(Clone)]
pub struct Scene {
    pub session: u64,
    pub info: vfx::Scene,
    pub received: time::Instant,
    pub assets: [Option<Arc<Vec<u8>>>;vfx::INSTANCES],
}
impl Scene {
    pub fn age_ms(&self) -> u64 { self.info.age_ms as u64+self.received.elapsed().as_millis() as u64 }

}

#[derive(Default)]
pub struct Stream {
    snake: cubes_protocol::snake::Replica,
    snake_published: Option<(u32,u32,u32)>,
    snake_requested: Option<time::Instant>,
    worm: cubes_protocol::worm::Replica,
    worm_published: Option<(u32,u32,u32)>,
    worm_requested: Option<time::Instant>,
    pending: Option<(vfx::Scene,time::Instant)>,
    cache: BTreeMap<u32,Arc<Vec<u8>>>,
}
impl Stream {
    pub fn observe(&mut self, bytes: &[u8]) {
        if let Some(state)=payload(bytes,0x8b).and_then(cubes_protocol::snake::State::parse) {
            self.snake.snapshot(state);
        }
        if let Some(step)=payload(bytes,0x8c).and_then(cubes_protocol::snake::Step::parse) {
            self.snake.step(step);
        }
        if let Some(state)=payload(bytes,0x8d).and_then(cubes_protocol::worm::State::parse) {
            self.worm.snapshot(state);
        }
        if let Some(step)=payload(bytes,0x8e).and_then(cubes_protocol::worm::Step::parse) {
            self.worm.step(step);
        }
        let Some(info)=payload(bytes,0x89).and_then(vfx::Scene::parse) else { return; };
        if let Some((old,received))=self.pending {
            if !info.newer_than(old) { return; }
            // A newer snapshot can still be behind local playback after network
            // jitter. Keep the running clock so pixels cannot replay or resurrect.
            if info.event==old.event
                && (info.age_ms as u64)<old.age_ms as u64+received.elapsed().as_millis() as u64 { return; }
        }
        self.pending=Some((info,time::Instant::now()));
    }
    fn publish(&mut self, shared: &Mutex<Shared>, session: u64, gallery: u32) {
        if let Some(snake)=self.snake.state.filter(|s|s.gallery==gallery) {
            let identity=(snake.gallery,snake.epoch,snake.tick);
            if self.snake_published!=Some(identity) {
                let mut state=shared.lock().unwrap();
                if state.session==session {
                    state.snake_ready=Some(snake);
                    self.snake_published=Some(identity);
                }
            }
        }
        if let Some(worm)=self.worm.state.filter(|s|s.gallery==gallery) {
            let identity=(worm.gallery,worm.epoch,worm.tick);
            if self.worm_published!=Some(identity) {
                let mut state=shared.lock().unwrap();
                if state.session==session {
                    state.worm_ready=Some(worm);
                    self.worm_published=Some(identity);
                }
            }
        }
        let Some((info,received))=self.pending else { return; };
        if info.gallery_revision!=gallery { return; }
        let scene=Scene { session,info,received,
            assets:core::array::from_fn(|i| self.cache.get(&info.slots[i].revision).cloned()) };
        let mut state=shared.lock().unwrap();
        if state.session==session { state.vfx_ready=Some(scene); }
    }
    /// Fetch up to four missing assets per pass to fill six cold slots promptly.
    pub async fn service(&mut self,socket:&UdpSocket,shared:&Mutex<Shared>,session:u64,gallery:u32)
        -> Result<(), &'static str>
    {
        if (self.snake.state.is_none() || self.snake.needs_snapshot)
            && self.snake_requested.is_none_or(|t|t.elapsed()>=time::Duration::from_millis(200)) {
            socket.send(&packet(8,&[])).await.map_err(|_| "snake snapshot request")?;
            self.snake_requested=Some(time::Instant::now());
        }
        if (self.worm.state.is_none() || self.worm.needs_snapshot)
            && self.worm_requested.is_none_or(|t|t.elapsed()>=time::Duration::from_millis(200)) {
            socket.send(&packet(9,&[])).await.map_err(|_| "worm snapshot request")?;
            self.worm_requested=Some(time::Instant::now());
        }
        for _ in 0..4 {
            if !self.fetch_one(socket,shared,session,gallery).await? { break; }
        }
        Ok(())
    }
    /// Fetch one missing immutable asset; snapshots and already
    /// cached slots continue to publish during the transfer. Repeated rolls reuse it.
    async fn fetch_one(&mut self,socket: &UdpSocket,shared: &Mutex<Shared>,session:u64,gallery:u32)
        -> Result<bool, &'static str>
    {
        self.publish(shared,session,gallery);
        let Some((info,_))=self.pending else { return Ok(false); };
        if info.gallery_revision!=gallery { return Ok(false); }
        let Some(slot)=info.slots.into_iter().find(|s| !self.cache.contains_key(&s.revision)) else { return Ok(false); };
        let mut bytes=vec![0;slot.bytes as usize];
        let mut received=vec![false;bytes.len().div_ceil(1024)];
        let mut buffer=[0;1200];
        for _ in 0..4 {
            if shared.lock().unwrap().session!=session { return Ok(false); }
            for (chunk,done) in received.iter().enumerate() {
                if *done { continue; }
                let mut body=slot.revision.to_le_bytes().to_vec();
                body.extend_from_slice(&(chunk as u16).to_le_bytes());
                socket.send(&packet(7,&body)).await.map_err(|_| "VFX asset request")?;
            }
            let deadline=time::Instant::now()+time::Duration::from_millis(100);
            while received.iter().any(|done| !done) {
                match time::timeout(deadline.saturating_duration_since(time::Instant::now()),socket.recv(&mut buffer)).await {
                    Ok(Ok(n))=>{
                        self.observe(&buffer[..n]);
                        self.publish(shared,session,gallery);
                        accept_chunk(&buffer[..n],slot.revision,&mut bytes,&mut received);
                    },
                    _=>break,
                }
                if time::Instant::now()>=deadline { break; }
            }
            if received.iter().all(|done| *done) {
                let sequence=Sequence::parse(&bytes).ok_or("invalid VFX lifetimes")?;
                if sequence.frame_count()!=slot.frames || sequence.period_ms()!=slot.period_ms {
                    return Err("VFX asset/scene mismatch");
                }
                // Bound long-lived sessions, including server catalog revisions.
                if self.cache.values().map(|v| v.len()).sum::<usize>()+bytes.len()>4*1024*1024 { self.cache.clear(); }
                self.cache.insert(slot.revision,Arc::new(bytes));
                self.publish(shared,session,gallery);
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn accept_chunk(packet: &[u8],revision:u32,bytes:&mut [u8],received:&mut [bool])->bool {
    let Some(p)=payload(packet,0x8a) else { return false; };
    if p.len()<6 || u32::from_le_bytes(p[..4].try_into().unwrap())!=revision { return false; }
    let index=u16::from_le_bytes(p[4..6].try_into().unwrap()) as usize;
    if index>=received.len() || received[index] { return false; }
    let start=index*1024;
    let end=(start+1024).min(bytes.len());
    if p.len()!=6+end-start { return false; }
    bytes[start..end].copy_from_slice(&p[6..]);received[index]=true;true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worm_and_snake_publish_and_recover_independently() {
        let mut worm=crate::server_worm::Worm::new(9,1);
        let mut snake=crate::server_snake::Snake::new(9,2);
        let mut stream=Stream::default();
        let shared=Mutex::new(Shared {session:1,running:true,gallery_ready:None,vfx_ready:None,snake_ready:None,worm_ready:None,
            error:None,position:[0.;3],orientation:[0.;3]});
        stream.observe(&packet(0x8d,&worm.state.encode()));
        stream.observe(&packet(0x8b,&snake.state.encode()));
        stream.publish(&shared,1,9);
        assert_eq!(shared.lock().unwrap().snake_ready.take(),Some(snake.state));
        assert_eq!(shared.lock().unwrap().worm_ready.take(),Some(worm.state));
        worm.step(0); // Lose one worm update.
        stream.observe(&packet(0x8e,&worm.step(1).encode()));
        stream.observe(&packet(0x8c,&snake.step(0).encode()));
        stream.publish(&shared,1,9);
        assert!(stream.worm.needs_snapshot);
        assert!(!stream.snake.needs_snapshot);
        assert!(shared.lock().unwrap().worm_ready.is_none());
        assert_eq!(shared.lock().unwrap().snake_ready.take(),Some(snake.state));
        stream.observe(&packet(0x8d,&worm.state.encode()));
        stream.publish(&shared,2,9);
        assert!(shared.lock().unwrap().worm_ready.is_none());
        stream.publish(&shared,1,9);
        assert_eq!(shared.lock().unwrap().worm_ready.take(),Some(worm.state));
        assert!(shared.lock().unwrap().snake_ready.is_none());
        stream.publish(&shared,1,9);
        assert!(shared.lock().unwrap().worm_ready.is_none());
    }
    #[test]
    fn snake_packets_publish_only_changes_and_ignore_old_sessions() {
        let mut snake=crate::server_snake::Snake::new(9,1);
        let mut stream=Stream::default();
        let shared=Mutex::new(Shared {session:1,running:true,gallery_ready:None,vfx_ready:None,snake_ready:None,worm_ready:None,
            error:None,position:[0.;3],orientation:[0.;3]});
        stream.observe(&packet(0x8b,&snake.state.encode()));
        stream.publish(&shared,1,9);
        assert_eq!(shared.lock().unwrap().snake_ready.take(),Some(snake.state));
        stream.publish(&shared,1,9);
        assert!(shared.lock().unwrap().snake_ready.is_none());
        let step=snake.step(0);
        stream.observe(&packet(0x8c,&step.encode()));
        stream.publish(&shared,2,9);
        assert!(shared.lock().unwrap().snake_ready.is_none());
        stream.publish(&shared,1,9);
        assert_eq!(shared.lock().unwrap().snake_ready.take(),Some(snake.state));
        stream.observe(&packet(0x8c,&step.encode()));
        stream.publish(&shared,1,9);
        assert!(shared.lock().unwrap().snake_ready.is_none());
    }
    #[test]
    fn repeated_slots_share_cached_bytes_and_stale_events_do_not_rewind() {
        let slot=vfx::Slot {revision:7,bytes:20,anchor:[40,8,0],frames:3,period_ms:150,pixel_side_c1:1};
        let info=vfx::Scene {gallery_revision:9,event:4,age_ms:500,slots:[slot;vfx::INSTANCES]};
        let mut stream=Stream::default();
        stream.observe(&packet(0x89,&info.encode()));
        let asset=Arc::new(vec![1,2,3]);
        stream.cache.insert(7,asset.clone());
        let shared=Mutex::new(Shared {session:1,running:true,gallery_ready:None,vfx_ready:None,snake_ready:None,worm_ready:None,
            error:None,position:[0.;3],orientation:[0.;3]});
        stream.publish(&shared,1,9);
        let published=shared.lock().unwrap().vfx_ready.take().unwrap();
        for cached in published.assets.iter().flatten() { assert!(Arc::ptr_eq(&asset,cached)); }
        let mut old=info;old.event=3;old.age_ms=2900;
        stream.observe(&packet(0x89,&old.encode()));
        assert_eq!(stream.pending.unwrap().0,info);
        old=info;old.age_ms=499;stream.observe(&packet(0x89,&old.encode()));
        assert_eq!(stream.pending.unwrap().0,info);
        old.age_ms=950;stream.observe(&packet(0x89,&old.encode()));
        stream.publish(&shared,1,9);
        let expired=shared.lock().unwrap().vfx_ready.take().unwrap();
        assert!(expired.info.slots.iter().all(|s|s.frame(expired.age_ms()).is_none()));
        stream.publish(&shared,2,9);
        assert!(shared.lock().unwrap().vfx_ready.is_none());
    }
    #[test]
    fn chunks_reject_wrong_revision_duplicates_and_bad_length() {
        let mut bytes=[0;3]; let mut received=[false];
        let mut b=7u32.to_le_bytes().to_vec(); b.extend_from_slice(&[0,0,1,2,3]);
        let p=packet(0x8a,&b);
        assert!(!accept_chunk(&p,8,&mut bytes,&mut received));
        assert!(accept_chunk(&p,7,&mut bytes,&mut received));
        assert_eq!(bytes,[1,2,3]);
        assert!(!accept_chunk(&p,7,&mut bytes,&mut received));
    }
}
