use sha2::{Digest, Sha256};
use std::fs;

mod exported {
    // This file travels with the app into Blueprint's source overlay.
    include!("Cube/cube_driver_manifest.rs");
}

fn main() {
    const SOURCE: &str = "Cube/cube.glb";
    const EXPORTED_MANIFEST: &str = "Cube/cube_driver_manifest.rs";
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={EXPORTED_MANIFEST}");
    let source = fs::read(SOURCE).expect("read cube reference");
    let digest = format!("{:x}", Sha256::digest(&source));
    assert_eq!(
        exported::CONTRACT_VERSION,
        5,
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
