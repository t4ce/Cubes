//! Shared read-only record view. V1 remains unchanged for existing assets/network.
//! World V2 uses c1 units and 12-byte records: i16 xyz, u8 side, palette,
//! part, constituent tier side, and two zero reserved bytes. Packed records
//! retain their constituent tier (e.g. a 32-c1 record packs 4³ c4 cells).
pub const PORTAL_MARKER_FIRST: u8 = 17;
pub const PORTAL_MARKER_LAST: u8 = 30;
pub const PORTAL_MARKER_FACES: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalMarkerKind {
    Spawn,
    Forward,
}

/// V2 portal marker parts are paired by face in runtime order:
/// north, east, south, west, bottom, top, center.
pub fn portal_marker(version: u8, part: u8) -> Option<(usize, PortalMarkerKind)> {
    if version != 2 || !(PORTAL_MARKER_FIRST..=PORTAL_MARKER_LAST).contains(&part) {
        return None;
    }
    let marker = part - PORTAL_MARKER_FIRST;
    Some((
        marker as usize / 2,
        if marker & 1 == 0 {
            PortalMarkerKind::Spawn
        } else {
            PortalMarkerKind::Forward
        },
    ))
}

pub fn is_portal_marker(version: u8, part: u8) -> bool {
    portal_marker(version, part).is_some()
}

#[derive(Clone, Copy)]
pub struct Record {
    pub origin: [i32; 3],
    pub side: i32,
    pub color: usize,
    pub part: u8,
    pub tier: i32,
}
impl Record {
    /// Called after validation. Storage packing never introduces a render tier.
    pub fn cubes(self) -> impl Iterator<Item = Record> {
        let n = self.side / self.tier;
        (0..n * n * n).map(move |i| Record {
            origin: [
                self.origin[0] + i / (n * n) * self.tier,
                self.origin[1] + (i / n) % n * self.tier,
                self.origin[2] + i % n * self.tier,
            ],
            side: self.tier,
            ..self
        })
    }
}
pub fn cubes(bytes: &[u8]) -> impl Iterator<Item = Record> + '_ {
    let version = bytes[4];
    // Arrival markers are point metadata represented by non-overlapping c2
    // records. They never become visible, pickable, collidable, animated, or
    // platform-owned world cubes.
    records(bytes)
        .filter(move |r| !is_portal_marker(version, r.part))
        .flat_map(Record::cubes)
}
pub fn stride(bytes: &[u8]) -> usize {
    if bytes[4] == 2 { 12 } else { 8 }
}
pub fn records(bytes: &[u8]) -> impl Iterator<Item = Record> + '_ {
    bytes[16 + bytes[10] as usize * 4..]
        .chunks_exact(stride(bytes))
        .map(move |r| {
            if bytes[4] == 2 {
                Record {
                    origin: core::array::from_fn(|a| {
                        i16::from_le_bytes([r[a * 2], r[a * 2 + 1]]) as i32
                    }),
                    side: r[6] as i32,
                    color: r[7] as usize,
                    part: r[8],
                    tier: r[9] as i32,
                }
            } else {
                Record {
                    origin: [r[0] as i8 as i32, r[1] as i8 as i32, r[2] as i8 as i32],
                    side: r[3] as i32,
                    color: r[4] as usize,
                    part: r[5],
                    tier: r[3] as i32,
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    fn fixture() -> Vec<u8> {
        let mut b = alloc::vec![0;20];
        b[..4].copy_from_slice(b"CUBE");
        b[4] = 2;
        b[6] = 14;
        b[7] = 12;
        b[8] = 2;
        b[10] = 1;
        b[11] = 4;
        b[19] = 255;
        b[12..16].copy_from_slice(&0.2f32.to_le_bytes());
        for (origin, side, tier, part) in [([-1024i16, 0, 0], 32, 8, 0), ([1000, 0, 0], 2, 2, 11)] {
            let mut r = [0; 12];
            for a in 0..3 {
                r[a * 2..a * 2 + 2].copy_from_slice(&origin[a].to_le_bytes());
            }
            r[6] = side;
            r[8] = part;
            r[9] = tier;
            b.extend_from_slice(&r);
        }
        b
    }
    #[test]
    fn v2_preserves_wide_coordinates_packed_tiers_and_portal_geometry() {
        let b = fixture();
        let r: Vec<_> = records(&b).collect();
        assert_eq!(r[0].origin, [-1024, 0, 0]);
        assert_eq!((r[0].side, r[0].tier), (32, 8));
        assert_eq!((r[1].side, r[1].part), (2, 11));
        let asset = crate::orchard::decode("fixture", &b).unwrap();
        assert_eq!(asset.cubes.len(), 65);
        assert!((asset.cubes[64].scale - 0.1986).abs() < 1e-6);
        assert_eq!(asset.cubes[0].center, [-204., -0.8, 0.8]);
    }
    #[test]
    fn v2_portal_markers_are_records_but_not_render_cubes() {
        let mut b = fixture();
        b[8] = 3;
        let mut marker = [0; 12];
        marker[..2].copy_from_slice(&(-1i16).to_le_bytes());
        marker[2..4].copy_from_slice(&(-2i16).to_le_bytes());
        marker[6] = 2;
        marker[8] = PORTAL_MARKER_FIRST;
        marker[9] = 2;
        b.extend_from_slice(&marker);
        assert_eq!(records(&b).count(), 3);
        assert_eq!(cubes(&b).count(), 65);
        assert_eq!(crate::orchard::decode("partial-marker", &b).err(), Some("cubes-marker"));
        b[8] = 4;
        let mut forward = marker;
        forward[4..6].copy_from_slice(&8i16.to_le_bytes());
        forward[8] = PORTAL_MARKER_FIRST + 1;
        b.extend_from_slice(&forward);
        assert_eq!(crate::orchard::decode("marker-pair", &b).unwrap().cubes.len(), 65);
        assert_eq!(
            portal_marker(2, PORTAL_MARKER_FIRST),
            Some((0, PortalMarkerKind::Spawn))
        );
        assert_eq!(
            portal_marker(2, PORTAL_MARKER_LAST),
            Some((6, PortalMarkerKind::Forward))
        );
        assert_eq!(portal_marker(1, PORTAL_MARKER_FIRST), None);
    }
    #[test]
    fn v2_rejects_overlap_truncation_and_invalid_tier_metadata() {
        let good = fixture();
        let mut b = good.clone();
        b.truncate(b.len() - 1);
        assert!(crate::orchard::decode("bad", &b).is_err());
        let mut b = good.clone();
        b[29] = 0;
        assert!(crate::orchard::decode("bad", &b).is_err());
        let mut b = good.clone();
        b[29] = 1;
        assert!(crate::orchard::decode("bad", &b).is_err());
        let mut b = good.clone();
        b[30] = 1;
        assert!(crate::orchard::decode("bad", &b).is_err());
        let mut b = good.clone();
        b[32..38].copy_from_slice(&good[20..26]);
        assert_eq!(
            crate::orchard::decode("bad", &b).err(),
            Some("cubes-overlap")
        );
    }
}
