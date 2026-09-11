#!/usr/bin/env python3
"""Exercise the app's actual picker transition methods with a retained world fixture."""
from pathlib import Path
import json,subprocess,tempfile
ROOT=Path(__file__).resolve().parents[1]
source='''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
pub fn sinf(x:f32)->f32{x.sin()}
pub fn cosf(x:f32)->f32{x.cos()}
pub fn tanf(x:f32)->f32{x.tan()}
'''
for name,file in [('orchard','orchard'),('subcubes','SubCubes'),('cube_format','cube_format'),('reveal','reveal'),('asset_brush','asset_brush'),('carousel','carousel'),('modes','modes')]:
    source+=f'#[path="{ROOT}/src/{file}.rs"] mod {name};\n'
assets=sorted((ROOT/'Cube/Assets').glob('*.cubes'))
groups=json.loads((ROOT/'Cube/asset-groups.json').read_text())['groups']
source+='static ASSETS: &[(&str,&[u8])] = &[\n'+''.join(f'("{p.name}",include_bytes!("{p}")),\n' for p in assets)+'];\n'
source+='static GROUPS: &[(&str,&[usize])] = &[\n'+''.join('('+json.dumps(g['name'])+', &'+str([assets.index(ROOT/'Cube/Assets'/n) for n in g['assets']])+'),\n' for g in groups)+'];\n'
source+='''
use modes::SceneMode;
#[derive(Debug)] enum CubeError {Contract}
mod level {pub const INFO:u8=1;}
mod logl {pub fn log(_:u8,_:core::fmt::Arguments) {}}
#[derive(Clone,Copy,Debug,PartialEq)] struct Camera {tag:u32}
struct Retained {view_projection:[f32;16]}
impl Camera {fn retained(&self,_:u32,_:u32,_:[f32;16])->Retained {Retained{view_projection:[1.;16]}}}
#[derive(Clone,Copy,Debug,PartialEq)] struct FlyCam {camera:Camera}
struct Frame;
impl Frame {fn width(&self)->u32{784} fn height(&self)->u32{441}}
struct Harness {
    carousel:carousel::Carousel,asset_brush:asset_brush::Brush,
    flycam:FlyCam,picker_camera:Option<FlyCam>,picker_mode:SceneMode,mode:SceneMode,
    frame:Frame,previous_view_projection:[f32;16],cursors:Vec<u8>,
    world:Vec<u8>,world_index:usize,walker_position:[f32;3],spawn_progress:usize,network_world:bool,
}
impl Harness {
    fn set_mode_projection(&mut self,_:SceneMode) {}
'''
for name in ('toggle_asset_picker','confirm_asset_picker','open_asset_picker','restore_picker_world'):
    main=(ROOT/'src/main.rs').read_text()
    start=main.index('    fn '+name+'(')
    end=main.index('\n    }',start)+6
    source+=main[start:end]+'\n'
source+='''
}
#[test]
fn world_picker_confirm_disable_reopen_preserves_world_and_choice() {
    let world_camera=FlyCam{camera:Camera{tag:42}};
    let mut app=Harness {carousel:carousel::Carousel::new(ASSETS,GROUPS),asset_brush:asset_brush::Brush::new(),
        flycam:world_camera,picker_camera:None,picker_mode:SceneMode::World,mode:SceneMode::World,frame:Frame,previous_view_projection:[0.;16],cursors:vec![1],
        world:vec![10,20,30],world_index:11,walker_position:[1.,2.,3.],spawn_progress:57,network_world:true};
    let original_world=app.world.as_ptr();
    assert!(!app.asset_brush.tool_active);
    app.toggle_asset_picker().unwrap();
    assert_eq!(app.mode,SceneMode::Orchard);
    app.carousel.select_item(2,1).unwrap();
    let chosen=app.carousel.selected_id();
    app.flycam.camera.tag=99; // The picker camera moves independently.
    app.confirm_asset_picker().unwrap();
    assert_eq!(app.mode,SceneMode::World);assert_eq!(app.flycam,world_camera);
    assert!(app.asset_brush.tool_active);assert_eq!(app.asset_brush.selected,chosen);
    assert_eq!(app.world.as_ptr(),original_world);assert_eq!(app.world,vec![10,20,30]);
    assert_eq!((app.world_index,app.walker_position,app.spawn_progress,app.network_world),(11,[1.,2.,3.],57,true));
    assert!(app.asset_brush.worlds.iter().all(Vec::is_empty), "confirm must not place");
    app.carousel.cycle_selection(-1).unwrap();app.asset_brush.confirm(app.carousel.selected_id());
    let last=(app.carousel.group,app.carousel.selected,app.asset_brush.selected);
    app.toggle_asset_picker().unwrap();
    assert_eq!(app.mode,SceneMode::World);assert!(!app.asset_brush.tool_active);
    app.toggle_asset_picker().unwrap();
    assert_eq!(app.mode,SceneMode::Orchard);
    assert_eq!((app.carousel.group,app.carousel.selected,app.carousel.selected_id()),last);
    app.restore_picker_world().unwrap(); // Abandon via a mode key; tool stays off.
    assert!(!app.asset_brush.tool_active);assert_eq!(app.flycam,world_camera);
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-picker-') as tmp:
    p=Path(tmp);(p/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(p/'tests.rs'),'-o',str(p/'tests')],check=True)
    subprocess.run([str(p/'tests')],check=True)
