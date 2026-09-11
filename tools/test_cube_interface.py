#!/usr/bin/env python3
"""Compare native menus to the HTML editor; test interactions without a rig."""
from pathlib import Path
import subprocess
import tempfile
import json
APP=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='cube-interface-') as directory:
    root=Path(directory)
    subprocess.run(['node',str(APP/'tools/test_cube_interface.cjs'),str(root)],check=True)
    (root/'Cargo.toml').write_text(f'''[package]
name="cube-interface-tests"
version="0.0.0"
edition="2024"
[dependencies]
libm="0.2"
trueos={{path="{APP.parent}/TRUEOS-Blueprints/api",features=["tokio-net-probe"]}}
[features]
default=["blueprint"]
host=[]
blueprint=["trueos/ui4-scene"]
[build-dependencies]
serde_json="1"
[lib]
path="lib.rs"
''')
    (root/'build.rs').write_text(f'''#[path="{APP}/tools/build_interface.rs"] mod build_interface;
fn main(){{build_interface::generate(std::path::Path::new("{APP}/Cube/CubeInterface"),&std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("interface_examples.rs"));}}
''')
    tests='''#![allow(dead_code)]
extern crate alloc;
extern crate self as trueos_picasso;
extern crate trueos as trueos_bp;
'''+f'''#[path="{APP}/src/cube_interface.rs"] mod cube_interface;
#[path="{APP.parent}/TRUEOS-Picasso/src/cam.rs"] pub mod cam;
#[path="{APP.parent}/TRUEOS/crates/trueos-graphics/decoder/bmp.rs"] mod bmp_decoder;
#[path="{APP}/src/interface_gpu.rs"] mod interface_gpu;
#[path="{APP}/src/picking.rs"] mod picking;
#[path="{APP}/src/modes.rs"] mod modes;
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_write(_:u32,_:*const u8,_:usize){{}}
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_blueprint_shutdown(_:*const u8,_:usize)->i32{{0}}
'''
    tests+='''#[test] fn texture_roundtrip_through_trueos_decoder(){
for page in 0..cube_interface::EXAMPLES.len(){
 let mut demo=cube_interface::Demo::new();demo.select(page);
 let canvas=demo.raster();let (color,normal)=cube_interface::textures(&canvas);
 let decoded=bmp_decoder::decode_bmp_rgba(&color).unwrap();
 let normals=bmp_decoder::decode_bmp_rgba(&normal).unwrap();
 assert_eq!((decoded.width,decoded.height),(canvas.width as u32*4,canvas.height as u32*4));
 assert_eq!((normals.width,normals.height),(decoded.width,decoded.height));
 for y in 0..decoded.height as usize {for x in 0..decoded.width as usize {
  let expected=canvas.colors[y/4*canvas.width+x/4];
  let i=(y*decoded.width as usize+x)*4;
  assert_eq!(&decoded.rgba[i..i+4],&[(expected>>16) as u8,(expected>>8) as u8,expected as u8,255]);
  assert!(normals.rgba[i+2]>=128);assert_eq!(normals.rgba[i+3],255);
 }}
 if let Ok(path)=std::env::var("CUBE_INTERFACE_PREVIEW_DIR"){
  std::fs::create_dir_all(&path).unwrap();
  std::fs::write(format!("{path}/{}.bmp",cube_interface::EXAMPLES[page].name),color).unwrap();
 }
}}
#[test] fn screen_pointer_picks_tilted_widgets_at_different_window_sizes(){
 for page in 0..cube_interface::EXAMPLES.len(){
  let mut demo=cube_interface::Demo::new();demo.select(page);
  let tier=cube_interface::EXAMPLES[page].tier;
  let (columns,rows)=(demo.layout.width,demo.layout.height);
  for (width,height) in [(784,441),(441,784),(1920,1080)] {
   let camera=interface_gpu::camera(width,height,columns,rows,tier).retained(width,height,[0.;16]);
   let project=|x:f32,y:f32| {
    let w=interface_gpu::rotation().rotate([(x-columns as f32*0.5)*0.04*tier as f32,(rows as f32*0.5-y)*0.04*tier as f32,0.]);
    let p=[w[0],w[1],w[2],1.];
    let clip:[f32;4]=core::array::from_fn(|r|(0..4).map(|c|camera.view_projection[c*4+r]*p[c]).sum());
    [clip[0]/clip[3],clip[1]/clip[3]]
   };
   for (x,y) in [(0.,0.),(columns as f32,0.),(0.,rows as f32),(columns as f32,rows as f32)] {
    assert!(project(x,y).iter().all(|v|v.abs()<1.),"panel must fit viewport");
   }
   for (id,widget) in demo.layout.widgets.iter().enumerate().filter(|(_,w)|!w.disabled){
    let [x,y,w,h]=widget.rect;
    let p=project(x as f32+w as f32*0.5,y as f32+h as f32*0.5);
    let sx=((p[0]+1.)*width as f32*0.5-0.5).round() as i32;
    let sy=((1.-p[1])*height as f32*0.5-0.5).round() as i32;
    let (o,d)=picking::ray(&camera.inverse_view_projection,sx,sy,width,height).unwrap();
    assert_eq!(demo.hit(interface_gpu::hit(o,d,columns,rows,tier).unwrap()),Some(id));
   }
  }
 }
}
'''
    for i,ref in enumerate(json.loads((root/'reference.json').read_text())):
        rects=[w['rect'] for w in ref['widgets']]
        tests+=f'''#[test] fn reference_{ref['name']}(){{
let mut demo=cube_interface::Demo::new();demo.select({i});
assert_eq!((demo.layout.width,demo.layout.height),({ref['width']},{ref['height']}));
assert_eq!(demo.layout.widgets.iter().map(|w|w.rect).collect::<Vec<_>>(),vec!{rects!r});
let pixels:Vec<_>=demo.raster().colors.iter().flat_map(|c|c.to_le_bytes()).collect();
let expected=include_bytes!("{root/ref['name']}.pixels");
assert_eq!(pixels.len(),expected.len());
for (i,(a,b)) in pixels.iter().zip(expected).enumerate(){{assert_eq!(a,b,"pixel byte {{i}}");}}
}}
'''
    (root/'lib.rs').write_text(tests)
    subprocess.run(['cargo','test','--offline','--manifest-path',str(root/'Cargo.toml'),'--target-dir',str(APP/'target')],check=True)
