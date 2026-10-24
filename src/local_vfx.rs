//! The server's baked catalog, bundled with Cubes for offline single-effect playback.
use alloc::{sync::Arc, vec::Vec};
use cubes_protocol::vfx::{self, Sequence};

include!(concat!(env!("OUT_DIR"), "/local_vfx_catalog.rs"));
const BUNDLE: &[u8] = include_bytes!("../Cube/Vfx/vfx.bin");

pub fn random_scene() -> Option<(&'static str, crate::network::VfxScene, Vec<u16>)> {
    let count = EFFECTS.len() as u32;
    if count == 0 { return None; }
    let threshold = count.wrapping_neg() % count;
    let index = loop {
        let value = trueos::rng::u32();
        if value >= threshold { break (value % count) as usize; }
    };
    scene(index)
}

fn scene(index: usize) -> Option<(&'static str, crate::network::VfxScene, Vec<u16>)> {
    let &(name, start, end) = EFFECTS.get(index)?;
    let bytes = BUNDLE.get(start..end)?;
    let sequence = Sequence::parse(bytes)?;
    // The VFX header carries the same exposed RGB palette as the gallery atlas.
    // Use the network path's exact RGB555 rounding.
    let palette = bytes[vfx::HEADER..vfx::HEADER + bytes[10] as usize * 3]
        .chunks_exact(3).map(|p| 0x8000 | ((u16::from(p[0])*31+127)/255)
            | (((u16::from(p[1])*31+127)/255)<<5)
            | (((u16::from(p[2])*31+127)/255)<<10)).collect();
    let slot = vfx::Slot {
        revision: index as u32 + 1, bytes: bytes.len() as u32, anchor: [0;3],
        frames: sequence.frame_count(), period_ms: sequence.period_ms(),
        pixel_side_c1: vfx::DEMO_PIXEL_SIDES_C1[0],
    };
    Some((name, crate::network::VfxScene {
        session: 0, received: trueos::time::Instant::now(),
        info: vfx::Scene { gallery_revision: 0, event: 1, age_ms: 0,
            slots: [slot;vfx::INSTANCES] },
        // One occupied slot; the other five never produce cubes.
        assets: core::array::from_fn(|i| (i == 0).then(|| Arc::new(bytes.to_vec()))),
    }, palette))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_bundled_effect_uses_one_slot_and_the_server_timing_and_palette() {
        assert_eq!(EFFECTS.len(), 150);
        for index in 0..EFFECTS.len() {
            let (_, scene, palette) = scene(index).unwrap();
            assert_eq!(scene.assets.iter().flatten().count(), 1);
            assert_eq!(scene.info.age_ms, 0);
            let slot = scene.info.slots[0];
            assert_eq!(slot.anchor, [0;3]);
            assert_eq!(slot.pixel_side_c1, vfx::DEMO_PIXEL_SIDES_C1[0]);
            let sequence = Sequence::parse(scene.assets[0].as_ref().unwrap()).unwrap();
            assert_eq!(slot.frames, sequence.frame_count());
            assert_eq!(slot.period_ms, sequence.period_ms());
            assert_eq!(slot.frame(vfx::DELAY_MS as u64 - 1), None);
            assert_eq!(slot.frame(vfx::DELAY_MS as u64), Some(0));
            assert_eq!(slot.frame(vfx::DELAY_MS as u64 + slot.frames as u64 * slot.period_ms as u64), None);
            assert!(sequence.lifetimes().all(|run| palette[run.pixel.palette as usize] & 0x8000 != 0));
        }
    }
}
