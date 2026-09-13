#!/usr/bin/env python3
"""Exercise real CubeSrv HTTP routes and Key5 decoding against all server exports."""
from pathlib import Path
import json, subprocess, tempfile
APP = Path(__file__).resolve().parents[1]
SERVER = APP.parent/'TRUEOS-Blueprints/apps/cubesrv'
with tempfile.TemporaryDirectory(prefix='cubes-world-http-') as temporary:
    root=Path(temporary)
    (root/'Cargo.toml').write_text(f'''[package]
name="cubes-world-http-test"
version="0.0.0"
edition="2024"
[lib]
path="lib.rs"
[dependencies]
libm="0.2"
serde={{version="=1.0.228",features=["derive"]}}
serde_json="=1.0.150"
reqwest={{version="=0.13.3",default-features=false,features=["json"]}}
axum={{version="=0.8.9",default-features=false,features=["http1","json","tokio"]}}
tokio={{version="=1.52.3",features=["rt","macros","net","sync","time"]}}
cubes-protocol={{path="{APP.parent}/TRUEOS-Blueprints/crates/cubes-protocol"}}
''')
    source='''#![allow(dead_code)]
extern crate alloc;
extern crate self as trueos;
pub mod worker {pub fn spawn(f:impl FnOnce()+Send+'static)->Result<(),()> {std::thread::Builder::new().spawn(f).map(|_|()).map_err(|_|())}}
pub mod runtime {pub fn current_thread_net()->tokio::runtime::Builder {let mut b=tokio::runtime::Builder::new_current_thread();b.enable_all();b}}
'''
    for name in ['orchard','cube_format','platform_lod','asset_brush','world_client']:
        source+=f'#[path="{APP}/src/{name}.rs"] mod {name};\n'
    source+=f'#[path="{APP}/src/SubCubes.rs"] mod subcubes;\n#[path="{SERVER}/worlds.rs"] mod server_worlds;\n'
    source+='const BUNDLES: &[&[u8]] = &[\n'
    for i,metadata in enumerate(json.loads((SERVER/'worlds/lvl27/platform-hulls.json').read_text())['worlds']):
        path=root/f'{i}.json'
        path.write_text(json.dumps(dict(id=i+1,cubes=list((SERVER/'worlds/lvl27'/metadata['filename']).read_bytes()),platforms=metadata)))
        source+=f'include_bytes!("{path}"),\n'
    source+='];\n'
    source+='''
#[tokio::test]
async fn all_worlds_arrive_via_http_with_matching_geometry_and_lod() {
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address=listener.local_addr().unwrap();
    let task=tokio::spawn(async move {axum::serve(listener,server_worlds::router(BUNDLES)).await.unwrap()});
    let mut view=platform_lod::View::new();
    for index in 0..27 {
        let page=world_client::fetch(index,&format!("http://{address}/worlds/{}",index+1)).await.unwrap();
        assert_eq!(page.asset.name,cubes_protocol::worlds::NAMES[index]);
        let bundle:cubes_protocol::worlds::World=serde_json::from_slice(BUNDLES[index]).unwrap();
        assert_eq!(page.bytes,bundle.cubes);
        assert_eq!(page.asset.cubes.len(),page.metadata.owners.len());
        let expected=orchard::decode_world(page.asset.name,&page.bytes).unwrap();
        for (a,b) in page.asset.cubes.iter().zip(expected.cubes) {assert_eq!(a.center,b.center);assert_eq!(a.flags,b.flags);}
        view.prepare(&page.asset,Some(page.metadata.clone()),[0.;3]);
        assert!(!view.asset(&page.asset).cubes.is_empty());
    }
    for id in [0,28,255] {
        assert_eq!(reqwest::get(format!("http://{address}/worlds/{id}")).await.unwrap().status(),404);
    }
    assert!(world_client::fetch(0,&format!("http://{address}/worlds/2")).await.is_err());
    task.abort();
}
#[tokio::test]
async fn cancelled_or_superseded_workers_cannot_replace_a_newer_selection() {
    use std::sync::Arc;
    let entered=Arc::new(tokio::sync::Notify::new());
    let release=Arc::new(tokio::sync::Notify::new());
    let app=server_worlds::router(BUNDLES).route("/slow",axum::routing::get({
        let entered=entered.clone();let release=release.clone();
        move || {let entered=entered.clone();let release=release.clone(); async move {
            entered.notify_one();release.notified().await;BUNDLES[0]
        }}
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address=listener.local_addr().unwrap();
    let task=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
    let client=world_client::Client::new();
    client.request_at(0,format!("http://{address}/slow"));
    tokio::time::timeout(std::time::Duration::from_secs(5),entered.notified()).await.unwrap();
    client.request_at(1,format!("http://{address}/worlds/2"));
    let reply=tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {if let Some(reply)=client.take_reply() {break reply;} tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
    }).await.unwrap();
    assert_eq!(reply.index,1);assert!(reply.result.is_ok());
    release.notify_one();tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(client.take_reply().is_none());
    client.request_at(0,format!("http://{address}/slow"));
    tokio::time::timeout(std::time::Duration::from_secs(5),entered.notified()).await.unwrap();
    client.cancel();release.notify_one();tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(client.take_reply().is_none());
    task.abort();
}
#[test]
fn malformed_partial_or_mismatched_worlds_never_become_pages() {
    assert!(world_client::decode(0,&BUNDLES[0][..100]).is_err());
    assert!(world_client::decode(1,BUNDLES[0]).is_err());
    for change in 0..4 {
        let mut bundle:serde_json::Value=serde_json::from_slice(BUNDLES[0]).unwrap();
        match change {
            0=>bundle["platforms"]["decoded"]=0.into(),
            1=>bundle["platforms"]["hulls"][0]["ranges"][0]=serde_json::json!([0,usize::MAX]),
            2=>bundle["platforms"]["hulls"][0]["side"]=0.into(),
            _=>bundle["cubes"][0]=0.into(),
        }
        assert!(world_client::decode(0,&serde_json::to_vec(&bundle).unwrap()).is_err());
    }
}
'''
    (root/'lib.rs').write_text(source)
    subprocess.run(['cargo','test','--offline','--manifest-path',str(root/'Cargo.toml'),'--target-dir',str(APP/'target/world-http')],check=True)
