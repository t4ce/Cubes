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

#[path = "tools/build_interface.rs"]
mod build_interface;
#[path = "tools/build_joined_world.rs"]
mod build_joined_world;

fn main() {
    build_interface::generate(
        std::path::Path::new("Cube/CubeInterface"),
        &std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join("interface_examples.rs"),
    );
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
    assert!(!showcase_assets.is_empty(), "no generated showcase assets");
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
    assert_eq!(
        worlds[0].file_name().unwrap().to_str().unwrap(),
        "world_01_sky.cubes",
        "joined Key5 demo must track the first world",
    );
    let first_world = fs::read(&worlds[0]).expect("read first joined world");
    let joined = build_joined_world::build(&first_world);
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    fs::write(output.join("joined_world_01_vertices.bin"), &joined.vertices)
        .expect("write first joined world vertices");
    fs::write(output.join("joined_world_01_indices.bin"), &joined.indices)
        .expect("write first joined world indices");
    println!(
        "cargo:warning=joined world 01: {} cubes -> {} exposed quads, {} vertex bytes, {} index bytes",
        joined.source_cubes,
        joined.quads,
        joined.vertices.len(),
        joined.indices.len(),
    );
    registry.push_str(&platform_registry(&worlds));
    registry.push_str(&write_registry("WORLD_ASSETS", worlds));
    registry.push_str(&material_palette_registry());
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
        11,
        "rebake the flight-indicator cube shader contract"
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

fn material_palette_registry() -> String {
    let palette: serde_json::Value = serde_json::from_slice(
        &fs::read("Cube/subcubes-materials.json").expect("read shared material palette"),
    ).expect("valid material palette JSON");
    let materials = palette["materials"].as_array().expect("palette materials");
    let colors: Vec<u32> = ["red", "orange", "yellow", "green", "blue", "violet"]
        .into_iter().map(|id| {
            let matches: Vec<_> = materials.iter().filter(|m| m["id"] == id).collect();
            assert_eq!(matches.len(), 1, "expected one palette material {id}");
            let mut rgba = [255; 4];
            for (axis, channel) in ["r", "g", "b"].into_iter().enumerate() {
                let value = matches[0]["rgb"][channel].as_f64().expect("palette RGB number");
                assert!(value.is_finite() && (0. ..=1.).contains(&value));
                rgba[axis] = (value * 255.).round() as u8;
            }
            u32::from_le_bytes(rgba)
        }).collect();
    format!("const MATERIAL_PALETTE_RGBA: [u32; 6] = {colors:?};\n")
}

// Geometry-only sidecar: validate against the exact asset and compile ownership
// to one byte per decoded cell. Runtime never parses JSON or scans hull AABBs.
fn platform_registry(worlds: &[std::path::PathBuf]) -> String {
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read("Cube/lvl27/platform-hulls.json").expect("export platform hulls")).unwrap();
    assert_eq!(manifest["version"], 1);
    let entries = manifest["worlds"].as_array().unwrap();
    assert_eq!(entries.len(), worlds.len());
    let mut out = String::from("const WORLD_PLATFORM_HULLS: &[platform_lod::Metadata] = &[\n");
    for (path, entry) in worlds.iter().zip(entries) {
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), entry["filename"].as_str().unwrap());
        let bytes = fs::read(path).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), entry["sha256"].as_str().unwrap(), "stale platform hull export");
        let records: Vec<_> = cube_format::cubes(&bytes).collect();
        assert_eq!(records.len(), entry["decoded"].as_u64().unwrap() as usize);
        let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let mut owners = vec![0u8; records.len()];
        let hulls = entry["hulls"].as_array().unwrap();
        assert!(hulls.len() < 256);
        out.push_str("platform_lod::Metadata { hulls: &[\n");
        for (id, h) in hulls.iter().enumerate() {
            let array = |key: &str| -> [f32; 3] { std::array::from_fn(|a| h[key][a].as_f64().unwrap() as f32) };
            let lo = array("lo"); let hi = array("hi"); let center = array("center");
            let side = h["side"].as_f64().unwrap() as f32;
            assert!(side.is_finite() && side > 0.);
            for a in 0..3 {
                assert!(lo[a].is_finite() && hi[a].is_finite() && lo[a] < hi[a]);
                assert_eq!(center[a], (lo[a]+hi[a])*0.5);
                assert!(center[a]-side*0.5 <= lo[a]-1. && center[a]+side*0.5 >= hi[a]+1.);
            }
            let mut count = 0;
            for range in h["ranges"].as_array().unwrap() {
                let start = range[0].as_u64().unwrap() as usize;
                let end = range[1].as_u64().unwrap() as usize;
                assert!(start < end && end <= records.len());
                for index in start..end {
                    assert_eq!(owners[index], 0, "duplicate platform owner");
                    let r = records[index];
                    assert_eq!(r.part, 0, "portal must never be replaced");
                    for a in 0..3 { assert!(r.origin[a] as f32 >= lo[a] && (r.origin[a]+r.side) as f32 <= hi[a]); }
                    owners[index] = id as u8 + 1; count += 1;
                }
            }
            assert_eq!(count, h["count"].as_u64().unwrap() as usize);
            let rgb = array("rgb");
            let flags = orchard::CUSTOM_RGB555 | (0..3).map(|a| {
                assert!(rgb[a] >= 0. && rgb[a] <= 255.);
                ((rgb[a] as u32 * 31 + 127)/255) << (a*5)
            }).sum::<u32>();
            // World pages invert authored Z; Y is restored by load_world.
            let c = [center[0]*unit, center[1]*unit, -center[2]*unit];
            let l = [lo[0]*unit, lo[1]*unit, -hi[2]*unit];
            let u = [hi[0]*unit, hi[1]*unit, -lo[2]*unit];
            out.push_str(&format!("platform_lod::Hull {{ cube: orchard::Cube {{ center: {c:?}, scale: {:?}, flags: {flags} }}, lo: {l:?}, hi: {u:?} }},\n", side*unit*0.5));
        }
        let owner_path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
            .join(format!("{}.owners", path.file_stem().unwrap().to_str().unwrap()));
        fs::write(&owner_path, owners).unwrap();
        out.push_str(&format!("], owners: include_bytes!({owner_path:?}) }},\n"));
    }
    out.push_str("];\n"); out
}
