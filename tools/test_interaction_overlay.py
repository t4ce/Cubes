#!/usr/bin/env python3
"""Test real mining/guide projection through UI4 quads, including pixel coverage."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
extern crate self as trueos;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
pub fn ceilf(x:f32)->f32{x.ceil()}
pub mod ui4_scene {
'''
sdk = (ROOT.parent/'TRUEOS-Blueprints/api/src/ui4_solara_text.rs').read_text()
for name in ('SpriteCorner', 'SpriteQuad'):
    start = sdk.index('pub struct '+name+' {')
    end = sdk.index('\n}', start)+2
    source += '#[derive(Copy,Clone,Debug,Default,PartialEq)]\n'+sdk[start:end]+'\n'
source += '''
#[derive(Debug,PartialEq)] pub enum Error {Busy}
#[derive(Default)] pub struct Frame {pub calls:usize,pub fail:bool}
impl Frame {
    pub fn draw_sprite_quads(&mut self,quads:&[SpriteQuad])->Result<(),Error> {
        assert!(!quads.is_empty() && quads.len()<=256);
        self.calls+=1;
        if self.fail {Err(Error::Busy)} else {Ok(())}
    }
}
}
'''
for name, file in [('orchard','orchard'), ('asset_brush','asset_brush'),
                   ('cube_format','cube_format'), ('subcubes','SubCubes'),
                   ('floor','floor'), ('interaction_overlay','interaction_overlay')]:
    source += f'#[path="{ROOT}/src/{file}.rs"] mod {name};\n'
source += r'''
use interaction_overlay::{strokes,draw};
use ui4_scene::{SpriteQuad,Frame,Error};
const MATRIX:[f32;16]=[0.1,0.,0.,0., 0.,0.1,0.,0., 0.,0.,0.05,0., 0.,0.,0.5,1.];
fn line(a:[f32;3],b:[f32;3])->Vec<u8> {
    a.into_iter().chain(b).flat_map(f32::to_le_bytes).collect()
}
fn contains(q:&SpriteQuad,x:f32,y:f32)->bool {
    let corners=[q.c0,q.c1,q.c2,q.c3,q.c0];
    corners.windows(2).all(|p|(p[1].x-p[0].x)*(y-p[0].y)-(p[1].y-p[0].y)*(x-p[0].x)>=-1e-4)
}
#[test]
fn rasterized_strokes_have_three_pixel_width_after_resize_and_contrast_on_light_and_dark() {
    for (w,h) in [(784,442),(1920,1080),(3840,2160)] {
        let quads=strokes(&line([-0.5,0.,0.],[0.5,0.,0.]),w,h,[255,255,255,191]);
        let x=w as f32*0.5+0.5;
        let cy=h as f32*0.5;
        let white_rows=(-4..=4).filter(|y|quads.iter().any(|q|
            q.color_rgba.to_le_bytes()==[255,255,255,191] && contains(q,x,cy+*y as f32+0.25))).count();
        assert_eq!(white_rows,3);
        for background in [0.,1.] {
            for y in [cy+0.25,cy+2.25] { // White core and dark border.
                let mut pixel=background;
                for q in &quads {
                    if contains(q,x,y) {
                        let rgba=q.color_rgba.to_le_bytes();
                        let alpha=rgba[3] as f32/255.;
                        pixel=rgba[0] as f32/255.*alpha+pixel*(1.-alpha);
                    }
                }
                if (background==0. && y<cy+1.) || (background==1. && y>cy+1.) {
                    assert!((pixel-background).abs()>0.4);
                }
            }
        }
    }
}
#[test]
fn every_tool_on_every_cube_size_reaches_screen_overlay() {
    for size in subcubes::SIDES {
        for tool in 0..4 {
            let demo=subcubes::Demo {blocks:vec![subcubes::Block {
                min:[0;3],side:size,material:0}],tool};
            let target=demo.target_details([0.1,0.1,10.],[0.,0.,-1.]).unwrap();
            let mut overlay=floor::Overlay::new();
            overlay.cube(&MATRIX,[0.;3],[size as f32*subcubes::C1;3]);
            overlay.mining_grid(&MATRIX,target);
            let quads=strokes(&overlay.bytes,784,441,[255,255,255,191]);
            assert!(!quads.is_empty());
            assert!(quads.len()<=256);
            assert!(quads.iter().all(|q|q.source_over && q.sprite_id==0));
            assert!(quads.iter().any(|q|q.color_rgba.to_le_bytes()==[255,255,255,191]));
        }
    }
}
#[test]
fn landing_outline_is_visible_without_mining_tool() {
    let demo=subcubes::Demo::new();
    assert!(demo.tool_side().is_none());
    let bytes=floor::cube_outline(&MATRIX,[-1.;3],[1.;3]);
    let quads=strokes(&bytes,784,441,[255;4]);
    assert!(!quads.is_empty());
    assert!(quads.iter().any(|q|q.color_rgba==u32::MAX));
}
#[test]
fn empty_and_clipped_guides_do_not_start_an_overlay_or_clear_the_world() {
    let mut frame=Frame::default();
    for bytes in [floor::vertices(&MATRIX,false),floor::cube_outline(&MATRIX,[0.,0.,-30.],[1.,1.,-29.])] {
        let quads=strokes(&bytes,784,441,[255;4]);
        assert!(quads.is_empty());
        draw(&mut frame,&quads).unwrap();
    }
    assert_eq!(frame.calls,0);
    let quads=strokes(&line([-1.,-1.,0.],[1.,1.,0.]),784,441,[255;4]);
    draw(&mut frame,&quads).unwrap();
    assert_eq!(frame.calls,1);
    frame.fail=true;
    assert_eq!(draw(&mut frame,&quads),Err(Error::Busy));
}
#[test]
fn bounded_batch_keeps_every_line_at_8k_and_skips_degenerate_segments() {
    let bytes=line([-1.,-1.,0.],[1.,1.,0.]).repeat(floor::VERTICES/2);
    let quads=strokes(&bytes,7680,4320,[255;4]);
    assert_eq!(quads.len(),256);
    assert!(strokes(&line([0.;3],[0.;3]),784,441,[255;4]).is_empty());
    assert!(strokes(&line([0.;3],[f32::NAN,0.,0.]),784,441,[255;4]).is_empty());
}
'''

# Check the actual call site, not just the isolated converter: the render must
# retire before UI4 takes over its pixels, and publication must follow both.
main = (ROOT/'src/main.rs').read_text()
wait = main.index('.wait(self.queue, point.value)')
draw = main.index('interaction_overlay::draw(&mut self.frame, &guide_quads)')
publish = main.index('.publish(Damage::full(width, height))', wait)
assert wait < draw < publish

with tempfile.TemporaryDirectory(prefix='cubes-guides-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
