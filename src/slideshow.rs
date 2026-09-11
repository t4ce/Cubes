//! RGB slideshow -> largest-tier cube mosaic, bounded by the existing renderer.
use crate::orchard::{Asset, CUSTOM_RGB555, Cube};
use alloc::vec::Vec;
pub const SIDE: usize = 60;
pub const PIXEL_SIDE: f32 = 12. * crate::subcubes::C1;
pub const DISTANCE: f32 = 240.;

pub fn asset(rgb: &[u8], name: &'static str) -> Result<Asset, &'static str> {
    if rgb.len() != 512 * 512 * 3 {
        return Err("slide-rgb-length");
    }
    let mut cubes = Vec::with_capacity(SIDE * SIDE);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut sum = [0u32; 3];
            let mut count = 0;
            for sy in y * 512 / SIDE..(y + 1) * 512 / SIDE {
                for sx in x * 512 / SIDE..(x + 1) * 512 / SIDE {
                    for c in 0..3 {
                        sum[c] += rgb[(sy * 512 + sx) * 3 + c] as u32;
                    }
                    count += 1;
                }
            }
            let color = sum.map(|v| (v * 31 + count * 127) / (count * 255));
            cubes.push(Cube {
                center: [
                    (x as f32 + 0.5 - SIDE as f32 / 2.) * PIXEL_SIDE,
                    (SIDE as f32 / 2. - y as f32 - 0.5) * PIXEL_SIDE,
                    -DISTANCE,
                ],
                scale: (12. - 0.014) * crate::subcubes::C1 * 0.5,
                flags: CUSTOM_RGB555 | color[0] | color[1] << 5 | color[2] << 10,
            });
        }
    }
    Ok(Asset {
        name,
        cubes,
        radius: DISTANCE + SIDE as f32 * PIXEL_SIDE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mosaic_preserves_orientation_color_and_fits_camera_budget() {
        let mut rgb = alloc::vec![0; 512*512*3];
        for y in 0..512 {
            for x in 0..512 {
                let color = if y < 256 { [255, 0, 0] } else { [0, 0, 255] };
                rgb[(y * 512 + x) * 3..(y * 512 + x + 1) * 3].copy_from_slice(&color);
            }
        }
        let image = asset(&rgb, "test").unwrap();
        assert_eq!(image.cubes.len(), 3600);
        assert!(image.cubes.len() + 81 < 3840);
        assert_eq!(image.cubes[0].flags, CUSTOM_RGB555 | 31);
        assert_eq!(image.cubes[3599].flags, CUSTOM_RGB555 | (31 << 10));
        assert!(image.cubes[0].center[0] < 0. && image.cubes[0].center[1] > 0.);
        let half_view = DISTANCE * (crate::walker_camera::FOV * 0.5).tan();
        assert!(SIDE as f32 * PIXEL_SIDE * 0.5 < half_view);
        let camera = crate::walker_camera::CubesWalkerCam::slideshow(&image.cubes);
        assert_eq!(camera.pose().0, [0.; 3]);
        assert_eq!(camera.pose().1.rotate([0., 0., -1.]), [0., 0., -1.]);
        assert!(camera.far_plane() > DISTANCE + PIXEL_SIDE);
        assert!(asset(&rgb[..100], "bad").is_err());
    }
}
