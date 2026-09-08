//! Tiny CPU-rasterized status panel composited over the retained cube frame.

use alloc::vec;
use trueos::ui4_scene::{Error, Frame, SpriteCorner, SpriteQuad};

const SPRITE_ID: u32 = 1;
const TEXT_BYTES: usize = 14; // `M3 E0000 U1024`
const PADDING: usize = 2;
const WIDTH: usize = TEXT_BYTES * microfont::FWIDTH + PADDING * 2;
const HEIGHT: usize = microfont::FHEIGHT + PADDING * 2;
const INSET: u32 = 8;

/// Expanded means a full cube is drawn; unexpanded means its small marker is drawn.
pub fn stamp(frame: &mut Frame, mode: u8, expanded: usize, unexpanded: usize) -> Result<(), Error> {
    let text = status_text(mode, expanded, unexpanded);
    let mut mask = vec![0u8; WIDTH * HEIGHT];
    let text = core::str::from_utf8(&text).map_err(|_| Error::Invalid)?;
    microfont::stamp_text(
        &mut mask,
        WIDTH,
        HEIGHT,
        PADDING as i32,
        PADDING as i32,
        text,
        1u8,
    )
    .map_err(|_| Error::Invalid)?;

    let mut rgba = vec![0u8; WIDTH * HEIGHT * 4];
    for (ink, pixel) in mask.iter().zip(rgba.chunks_exact_mut(4)) {
        if *ink != 0 {
            pixel.copy_from_slice(&[235, 235, 235, 255]);
        } else {
            pixel.copy_from_slice(&[0, 0, 0, 150]);
        }
    }
    frame.upload_sprite_rgba8(SPRITE_ID, WIDTH as u32, HEIGHT as u32, &rgba)?;

    let x = frame.width().saturating_sub(WIDTH as u32 + INSET) as f32;
    let y = frame.height().saturating_sub(HEIGHT as u32 + INSET) as f32;
    let right = x + WIDTH as f32;
    let bottom = y + HEIGHT as f32;
    frame.draw_sprite_quads(&[SpriteQuad {
        sprite_id: SPRITE_ID,
        c0: SpriteCorner {
            x,
            y,
            u: 0.0,
            v: 0.0,
        },
        c1: SpriteCorner {
            x: right,
            y,
            u: 1.0,
            v: 0.0,
        },
        c2: SpriteCorner {
            x: right,
            y: bottom,
            u: 1.0,
            v: 1.0,
        },
        c3: SpriteCorner {
            x,
            y: bottom,
            u: 0.0,
            v: 1.0,
        },
        color_rgba: u32::from_le_bytes([255, 255, 255, 255]),
        source_over: true,
    }])
}

fn status_text(mode: u8, expanded: usize, unexpanded: usize) -> [u8; TEXT_BYTES] {
    let mut text = *b"M0 E0000 U0000";
    text[1] = b'0' + mode.min(9);
    write_count(&mut text[4..8], expanded);
    write_count(&mut text[10..14], unexpanded);
    text
}

fn write_count(output: &mut [u8], value: usize) {
    let mut value = value.min(9_999);
    for byte in output.iter_mut().rev() {
        *byte = b'0' + (value % 10) as u8;
        value /= 10;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_keeps_four_digit_counts_and_mode() {
        assert_eq!(status_text(3, 7, 1024), *b"M3 E0007 U1024");
    }
}
