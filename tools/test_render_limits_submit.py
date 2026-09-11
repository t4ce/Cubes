#!/usr/bin/env python3
"""Validate actual Key9 cube geometry through the production retained decoder."""
from pathlib import Path
import subprocess,sys,tempfile
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT.parent/'TRUEOS/tools'))
from test_retained_scene import source
from test_clip_position3_uv_texture import item
s=source()+'''
extern crate alloc;
extern crate self as libm;
pub fn roundf(x:f32)->f32{x.round()}
pub fn tanf(x:f32)->f32{x.tan()}
mod orchard {
    pub const CUSTOM_RGB555:u32=1<<15;
    #[derive(Clone,Copy)] pub struct Cube { pub center:[f32;3],pub scale:f32,pub flags:u32 }
}
'''
s+=f'#[path="{ROOT}/src/render_limits.rs"] mod render_limits;\n'
s+=item(str(ROOT/'src/main.rs'),'encode_seed')
s+='''
#[test]
fn slider_scene_accepts_one_opaque_draw_group_at_every_step() {
    let mut limits=render_limits::Limits::new();
    for step in 0..16 {
        for row in 0..2 {
            limits.pointer([render_limits::step_x(step),render_limits::ROW_Y[row]+0.4,-10.],[0.,0.,1.],true,true);
            limits.cancel_drag();
        }
        let mut bytes=vec![0;limits.cubes.len()*64];
        for (i,cube) in limits.cubes.iter().enumerate() {
            encode_seed(RetainedTransformSeed {
                translation:cube.center,previous_translation:cube.center,
                scale:[cube.scale;3],rotation:[0.,0.,0.,1.],local_radius:1.74,
                draw_group:0,flags:((i as u32)<<16)|cube.flags,
            },&mut bytes[i*64..(i+1)*64]);
        }
        let seeds=decode_retained_scene_seeds(&bytes).unwrap();
        assert!(seeds.len()<=limits.full() && seeds.len()<=limits.seeds());
        assert_eq!(picasso_retained_draw_templates(44,&seeds,&[RetainedDrawRange{first_index:0,index_count:44}]).unwrap(),
            [[44,0,0,0,seeds.len() as u32,0]]);
    }
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-limits-submit-') as tmp:
    p=Path(tmp);(p/'tests.rs').write_text(s)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(p/'tests.rs'),'-o',str(p/'tests')],check=True)
    subprocess.run([str(p/'tests')],check=True)
