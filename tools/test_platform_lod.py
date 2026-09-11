#!/usr/bin/env python3
"""Exercise platform replacement against all 27 decoded exports on the host."""
from pathlib import Path
import hashlib
import json
import struct
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
'''
for module, file in [('orchard','orchard'), ('asset_brush','asset_brush'),
                     ('subcubes','SubCubes'), ('cube_format','cube_format'),
                     ('marker_lod','marker_lod'), ('platform_lod','platform_lod')]:
    source += f'#[path="{APP}/src/{file}.rs"] mod {module};\n'
manifest = json.loads((APP/'Cube/lvl27/platform-hulls.json').read_text())
assert len(manifest['worlds']) == 27
with tempfile.TemporaryDirectory(prefix='cubes-platform-lod-') as temporary:
    root = Path(temporary)
    source += 'static WORLDS: &[(&str, &[u8], platform_lod::Metadata)] = &[\n'
    for world in manifest['worlds']:
        path = APP/'Cube/lvl27'/world['filename']
        raw = path.read_bytes()
        assert hashlib.sha256(raw).hexdigest() == world['sha256']
        unit = struct.unpack_from('<f', raw, 12)[0]
        owners = bytearray(world['decoded'])
        hulls = []
        for i,h in enumerate(world['hulls']):
            for start,end in h['ranges']:
                assert not any(owners[start:end])
                owners[start:end] = bytes([i+1])*(end-start)
            def orient(v): return [v[0]*unit,v[1]*unit,-v[2]*unit]
            lo = orient(h['lo']); hi = orient(h['hi']); lo[2],hi[2]=hi[2],lo[2]
            flags = (1<<15) | sum(((c*31+127)//255) << (a*5) for a,c in enumerate(h['rgb']))
            hulls.append(f'platform_lod::Hull {{cube: orchard::Cube {{center: {orient(h["center"])}, scale: {h["side"]*unit*.5}, flags: {flags}}}, lo: {lo}, hi: {hi}}}')
        owner_path = root/f'{world["filename"]}.owners'
        owner_path.write_bytes(owners)
        source += f'({json.dumps(world["filename"])},include_bytes!("{path}"),platform_lod::Metadata {{hulls: &[{",".join(hulls)}],owners:include_bytes!("{owner_path}")}}),\n'
    source += '];\n'
    source += r'''
#[test]
fn every_world_keeps_two_complete_platforms_and_one_proxy_per_remainder() {
    let mut before = 0; let mut after = 0;
    let mut view = platform_lod::View::new();
    for (name, bytes, m) in WORLDS {
        let mut asset = orchard::decode(name, bytes).unwrap();
        for cube in &mut asset.cubes { cube.center = orchard::world_from_demo(cube.center); }
        let records: Vec<_> = cube_format::cubes(bytes).collect();
        assert_eq!(records.len(), m.owners.len());
        // Move to every platform, reusing the same view across all world switches.
        for target in 0..m.hulls.len() {
            view.prepare(&asset, Some(m), m.hulls[target].cube.center);
            let mut originals = vec![false;asset.cubes.len()];
            let mut kept = vec![0;m.hulls.len()]; let mut proxies = 0;
            for id in 0..view.asset(&asset).cubes.len() {
                if let Some(src) = view.source_id(id) {
                    assert!(!originals[src]); originals[src] = true;
                    let owner = m.owners[src];
                    if owner != 0 { kept[owner as usize-1] += 1; assert!(view.solid(id)); }
                    assert_eq!(view.asset(&asset).cubes[id].center, asset.cubes[src].center);
                } else { proxies += 1; assert!(view.solid(id)); }
            }
            assert_eq!(kept.iter().filter(|&&n| n != 0).count(), platform_lod::DETAIL_PLATFORMS.min(m.hulls.len()));
            assert!(kept[target] != 0, "camera's current platform must expand");
            assert_eq!(proxies, m.hulls.len().saturating_sub(platform_lod::DETAIL_PLATFORMS));
            for (id, &owner) in m.owners.iter().enumerate() {
                if owner == 0 { assert!(originals[id], "independent paths/portals disappear"); }
                else { assert_eq!(originals[id], kept[owner as usize-1] != 0, "partial platform"); }
                if records[id].part != 0 { assert_eq!(owner,0, "portal assigned to platform"); }
                if owner != 0 {
                    let h = &m.hulls[owner as usize-1]; let c = asset.cubes[id];
                    for a in 0..3 { assert!((c.center[a]-h.cube.center[a]).abs()+c.scale < h.cube.scale); }
                }
            }
            before += asset.cubes.len(); after += view.asset(&asset).cubes.len();
        }
    }
    println!("all-world platform visits: {before} original vs {after} candidate cubes before visibility ({:.1}% fewer)", 100.*(1.-after as f64/before as f64));
    assert!(after < before);
}
#[test]
fn cached_view_updates_portals_and_preserves_placed_ids_and_disabled_mode() {
    let (name, bytes, m) = &WORLDS[0];
    let mut asset = orchard::decode(name, bytes).unwrap();
    for cube in &mut asset.cubes { cube.center = orchard::world_from_demo(cube.center); }
    let mut view = platform_lod::View::new(); let eye=m.hulls[0].cube.center;
    view.prepare(&asset, Some(m), eye);
    let portal=m.owners.iter().position(|&o| o==0).unwrap();
    asset.cubes[portal].scale=0.123;
    view.prepare(&asset,Some(m),eye);
    let rendered=(0..view.asset(&asset).cubes.len()).find(|&id| view.source_id(id)==Some(portal)).unwrap();
    assert_eq!(view.asset(&asset).cubes[rendered].scale,0.123);
    let base=asset.cubes.len(); asset.cubes.push(asset.cubes[portal]);
    view.prepare(&asset,Some(m),eye);
    assert!((0..view.asset(&asset).cubes.len()).any(|id| view.source_id(id)==Some(base)));
    view.prepare(&asset,None,eye);
    assert!(core::ptr::eq(view.asset(&asset),&asset), "disabled path must not copy all cubes");
    assert_eq!(view.source_id(base),Some(base)); assert!(!view.solid(base));
    view.prepare(&asset,Some(m),eye);
    assert!(view.asset(&asset).cubes.len()<asset.cubes.len());
}
#[test]
fn platform_solids_survive_distance_but_respect_the_global_detail_cap() {
    let mut reducer=marker_lod::Reducer::new();
    let source=[orchard::Cube {center:[0.,0.,-700.],scale:1.,flags:orchard::CUSTOM_RGB555}];
    let view=[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.];
    reducer.prepare_with_solids(&source,&[0],[0.;3],&view,2.414,480,1,|_|1.,|_|true);
    assert_eq!(reducer.cubes.len(),1); assert_eq!(reducer.cubes[0].scale,1.);
    assert_eq!(reducer.dots_before,0);
    reducer.prepare_with_solids(&source,&[0],[0.;3],&view,2.414,480,0,|_|1.,|_|true);
    assert!(reducer.cubes[0].scale<0.001); assert_eq!(reducer.dots_before,1);
}
'''
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests'),'--nocapture'],check=True)
