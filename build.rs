use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

fn main() {
    const SOURCE: &str = "Cube/cube.glb";
    const DRIVER: &str = "../TRUEOS/crates/trueos-shader/generated_patch_cube.rs";
    const EXPORTED_MANIFEST: &str = "Cube/cube_driver_manifest.rs";
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={DRIVER}");
    println!("cargo:rerun-if-changed={EXPORTED_MANIFEST}");
    let source = fs::read(SOURCE).expect("read cube reference");
    let digest = format!("{:x}", Sha256::digest(&source));
    let driver = fs::read_to_string(DRIVER)
        .or_else(|_| fs::read_to_string(EXPORTED_MANIFEST))
        .unwrap_or_else(|_| panic!("export the cube HS/DS driver bundle first"));
    assert!(Path::new(EXPORTED_MANIFEST).exists() || Path::new(DRIVER).exists(),
        "export the cube HS/DS driver bundle first");
    assert!(driver.contains(&format!("SOURCE_SHA256: &str = \"{digest}\"")),
        "cube reference differs from baked HS/DS; rebake and export before building");
    // No vertex/index mesh expansion at build time. Runtime uploads one seed.
}
