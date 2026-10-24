//! Validate and embed the local VFX catalog at build time.
use std::{fs, path::Path};
use sha2::{Digest, Sha256};

pub fn generate(root: &Path) {
    println!("cargo:rerun-if-changed={}", root.display());
    let bundle = fs::read(root.join("vfx.bin")).expect("local VFX bundle");
    let catalog: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("catalog.json")).expect("local VFX catalog")).unwrap();
    let mut source = String::from("const EFFECTS: &[(&str, usize, usize)] = &[\n");
    for effect in catalog.as_array().expect("VFX entries") {
        let name = effect["name"].as_str().unwrap();
        let start = effect["offset"].as_u64().unwrap() as usize;
        let end = start + effect["length"].as_u64().unwrap() as usize;
        assert_eq!(format!("{:x}", Sha256::digest(&bundle[start..end])),
            effect["sha256"].as_str().unwrap(), "stale local VFX {name}");
        source.push_str(&format!("({name:?}, {start}, {end}),\n"));
    }
    assert!(!catalog.as_array().unwrap().is_empty());
    source.push_str("];\n");
    fs::write(std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("local_vfx_catalog.rs"), source).unwrap();
}
