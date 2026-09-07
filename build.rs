use sha2::{Digest, Sha256};
use std::fs;

fn main() {
    const SOURCE: &str = "Cube/cube.glb";
    const DRIVER: &str = "../TRUEOS/crates/trueos-shader/generated_patch_cube.rs";
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-changed={DRIVER}");
    let source = fs::read(SOURCE).expect("read cube reference");
    let digest = format!("{:x}", Sha256::digest(&source));
    let driver = fs::read_to_string(DRIVER).expect("export the cube HS/DS driver bundle first");
    assert!(driver.contains(&format!("SOURCE_SHA256: &str = \"{digest}\"")),
        "cube reference differs from baked HS/DS; rebake and export before building");
    // No vertex/index mesh expansion at build time. Runtime uploads one seed.
}
