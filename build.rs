extern crate alloc;
#[allow(dead_code)]
#[path = "src/cube_format.rs"]
mod cube_format;
#[allow(dead_code)]
#[path = "src/SubCubes.rs"]
mod subcubes;
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
    let background_package = fs::read("Cube/mandelbox/mandelbox.stpkg").expect("Mandelbox package");
    let expected =
        fs::read_to_string("Cube/mandelbox/package.sha256").expect("Mandelbox package hash");
    assert_eq!(
        format!("{:x}", Sha256::digest(&background_package)),
        expected.trim(),
        "stale Mandelbox package; run tools/bake_mandelbox.py"
    );
    println!("cargo:rerun-if-changed=Cube");
    let mut assets: Vec<_> = fs::read_dir("Cube")
        .expect("Cube asset directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cubes"))
        .collect();
    assets.sort();
    let write_registry = |constant: &str, assets: Vec<std::path::PathBuf>| {
        let mut registry = format!("const {constant}: &[(&str, &[u8])] = &[\n");
        for path in assets {
            let bytes = fs::read(&path).expect("read CUBES asset");
            orchard::decode("build-validation", &bytes).unwrap_or_else(|error| {
                panic!(
                    "{}: {} (expected valid CUBES geometry v1/v2)",
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
        registry
    };
    let mut registry = write_registry("ORCHARD_ASSETS", assets);
    let asset_dir = std::path::Path::new("Cube/Assets");
    let mut showcase_assets: Vec<_> = fs::read_dir(asset_dir)
        .expect("Cube/Assets directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cubes"))
        .collect();
    showcase_assets.sort();
    assert_eq!(
        showcase_assets.len(),
        49,
        "expected all generated showcase assets"
    );
    let mut colors = std::collections::BTreeSet::from([0x7fff_u32]);
    for path in &showcase_assets {
        let data = fs::read(path).unwrap();
        for rgb in data[16..16 + data[10] as usize * 4].chunks_exact(4) {
            colors.insert(
                (0..3)
                    .map(|a| ((rgb[a] as u32 * 31 + 127) / 255) << (a * 5))
                    .sum(),
            );
        }
    }
    assert_eq!(
        colors.into_iter().collect::<Vec<_>>(),
        exported::CAROUSEL_COLORS,
        "asset colours changed: rebake/export cube shader"
    );
    let names: Vec<_> = showcase_assets
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    let catalogue: serde_json::Value =
        serde_json::from_slice(&fs::read("Cube/asset-groups.json").unwrap()).unwrap();
    registry.push_str("const ASSET_GROUPS: &[(&str, &[usize])] = &[\n");
    let mut grouped = std::collections::BTreeSet::new();
    for group in catalogue["groups"].as_array().unwrap() {
        let ids: Vec<_> = group["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| {
                let index = names
                    .iter()
                    .position(|n| n == name.as_str().unwrap())
                    .expect("unknown grouped asset");
                assert!(grouped.insert(index), "duplicate grouped asset");
                index
            })
            .collect();
        assert!(!ids.is_empty());
        registry.push_str(&format!(
            "({:?}, &{:?}),\n",
            group["name"].as_str().unwrap(),
            ids
        ));
    }
    assert_eq!(grouped.len(), names.len(), "ungrouped asset");
    registry.push_str("];\n");
    registry.push_str(&write_registry("ASSET_GRID_ASSETS", showcase_assets));
    let world_dir = std::path::Path::new("Cube/lvl27");
    let mut worlds: Vec<_> = fs::read_dir(world_dir)
        .expect("Cube/lvl27 world directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cubes"))
        .collect();
    worlds.sort();
    assert_eq!(worlds.len(), 27, "expected all 27 lvl27 world assets");
    registry.push_str(&write_registry("WORLD_ASSETS", worlds));
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
        10,
        "rebake the material-finish cube shader contract"
    );
    assert_eq!(
        digest,
        exported::SOURCE_SHA256,
        "cube reference differs from baked HS/DS; rebake and export before building"
    );
    let palette = fs::read("Cube/subcubes-materials.json").expect("read material palette export");
    assert_eq!(
        format!("{:x}", Sha256::digest(&palette)),
        exported::PALETTE_SHA256,
        "material palette differs from baked HS/DS; run tools/bake_patch_cube.py and tools/export_patch_driver.py"
    );
    // The sidecar validates the exported source, not the currently booted
    // kernel. Driver integration still requires rebuilding/booting TRUEOS.
    // No vertex/index mesh expansion at build time. Runtime uploads one seed.
}
