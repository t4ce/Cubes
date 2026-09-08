use sha2::{Digest, Sha256};
use std::fs;
#[allow(dead_code)]
#[path = "src/orchard.rs"]
mod orchard;

mod exported {
    // This file travels with the app into Blueprint's source overlay.
    include!("Cube/cube_driver_manifest.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=Cube");
    let mut assets: Vec<_> = fs::read_dir("Cube")
        .expect("Cube asset directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cubes"))
        .collect();
    assets.sort();
    let mut registry = String::from("const ORCHARD_ASSETS: &[(&str, &[u8])] = &[\n");
    for path in assets {
        let bytes = fs::read(&path).expect("read CUBES asset");
        orchard::decode("build-validation", &bytes).unwrap_or_else(|error| {
            panic!(
                "{}: {} (expected strict-grid nature v1)",
                path.display(),
                error
            )
        });
        let absolute = fs::canonicalize(&path).unwrap();
        registry.push_str(&format!(
            "({:?}, include_bytes!({:?})),\n",
            path.file_name().unwrap().to_str().unwrap(),
            absolute
        ));
    }
    registry.push_str("];\n");
    fs::write(
        std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("orchard_assets.rs"),
        registry,
    )
    .unwrap();
    const SOURCE: &str = "Cube/cube.glb";
    const EXPORTED_MANIFEST: &str = "Cube/cube_driver_manifest.rs";
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={EXPORTED_MANIFEST}");
    let source = fs::read(SOURCE).expect("read cube reference");
    let digest = format!("{:x}", Sha256::digest(&source));
    assert_eq!(
        exported::CONTRACT_VERSION,
        8,
        "rebake the oriented palette cube HS/DS shader contract"
    );
    assert_eq!(
        digest,
        exported::SOURCE_SHA256,
        "cube reference differs from baked HS/DS; rebake and export before building"
    );
    // The sidecar validates the exported source, not the currently booted
    // kernel. Driver integration still requires rebuilding/booting TRUEOS.
    // No vertex/index mesh expansion at build time. Runtime uploads one seed.
}
