use std::{env, fs, path::PathBuf};

fn main() {
    const SOURCE: &str = "Cube/cube.glb";
    println!("cargo:rerun-if-changed={SOURCE}");

    let source = fs::read(SOURCE).expect("read Cube/cube.glb");
    let gltf = gltf::Gltf::from_slice(&source).expect("parse Cube/cube.glb");
    let blob = gltf.blob.as_deref().expect("cube GLB BIN chunk");
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            assert_eq!(
                primitive.mode(),
                gltf::mesh::Mode::Triangles,
                "Cubes supports triangle-list primitives"
            );
            let reader = primitive.reader(|buffer| match buffer.source() {
                gltf::buffer::Source::Bin => Some(blob),
                gltf::buffer::Source::Uri(_) => None,
            });
            let positions = reader
                .read_positions()
                .expect("cube primitive POSITION")
                .collect::<Vec<_>>();
            let normals = reader
                .read_normals()
                .expect("cube primitive NORMAL")
                .collect::<Vec<_>>();
            assert_eq!(positions.len(), normals.len(), "cube position/normal count");
            let base_vertex = u32::try_from(vertices.len() / 24).expect("cube vertex count");
            for (position, normal) in positions.into_iter().zip(normals) {
                for component in position.into_iter().chain(normal) {
                    vertices.extend_from_slice(&component.to_le_bytes());
                }
            }
            let primitive_indices = reader
                .read_indices()
                .expect("cube primitive indices")
                .into_u32();
            indices.extend(primitive_indices.map(|index| index + base_vertex));
        }
    }

    assert!(
        !vertices.is_empty() && !indices.is_empty(),
        "cube geometry is empty"
    );
    let mut index_bytes = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        index_bytes.extend_from_slice(&index.to_le_bytes());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("cube.posnormal.f32le"), &vertices).expect("write cube vertices");
    fs::write(out.join("cube.indices.u32le"), &index_bytes).expect("write cube indices");
    fs::write(
        out.join("cube_asset.rs"),
        format!(
            "pub const CUBE_VERTEX_COUNT: u32 = {};\npub const CUBE_INDEX_COUNT: u32 = {};\npub static CUBE_VERTICES: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/cube.posnormal.f32le\"));\npub static CUBE_INDICES: &[u8] = include_bytes!(concat!(env!(\"OUT_DIR\"), \"/cube.indices.u32le\"));\n",
            vertices.len() / 24,
            index_bytes.len() / 4,
        ),
    )
    .expect("write cube catalog");
}
