#!/usr/bin/env python3
"""Compile the reference cube's one-seed HS/DS experiment on pinned Mesa.

This is an offline compiler lane. It does not install a runtime pipeline or
claim a rendered-image proof. All generated files stay in the selected output
directory; the imported baseline remains available until runtime integration.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TRUEOS = ROOT.parent / "TRUEOS"
PALETTE = ROOT / "Cube/subcubes-materials.json"
MATERIAL_IDS = ("red", "orange", "yellow", "green", "blue", "violet")


def load_palette(path: Path):
    raw = path.read_bytes()
    document = json.loads(raw)
    if (document.get("format"), document.get("version"), document.get("colorSpace")) != (
        "subcubes-material-palette", 1, "sRGB"
    ):
        raise ValueError("expected subcubes-material-palette v1 in sRGB")
    entries = document.get("materials", [])
    if len(entries) != 6 or sorted(m.get("id", "") for m in entries) != sorted(MATERIAL_IDS):
        raise ValueError("expected exactly red/orange/yellow/green/blue/violet materials")
    materials = {m["id"]: m for m in entries}
    ordered = [materials[id] for id in MATERIAL_IDS]
    for m in ordered:
        values = [m["rgb"][a] for a in "rgb"] + [m["roughness"], m["metallic"]]
        if any(type(v) not in (int, float) or not math.isfinite(v) or not 0 <= v <= 1 for v in values):
            raise ValueError(f"{m['id']}: RGB, roughness and metallic must be finite in [0,1]")
    return raw, ordered


def srgb_to_linear(value):
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def palette_shader(materials):
    cases = []
    for i, m in enumerate(materials):
        color = ",".join(format(srgb_to_linear(m["rgb"][a]), ".9f") for a in "rgb")
        cases.append(f"case {i}u: baseColor=vec3({color}); roughness={float(m['roughness'])}; "
                     f"metallic={float(m['metallic'])}; break;")
    return '''// Imported sRGB palette is converted to linear light for shading.
void paletteMaterial(uint material, out vec3 baseColor, out float roughness, out float metallic) {
    baseColor=vec3(0.5); roughness=1.0; metallic=0.0;
    switch (material) {
''' + "\n".join(cases) + '''
    }
}
'''


def geometry(path: Path):
    raw = path.read_bytes()
    if len(raw) < 12:
        raise ValueError("truncated GLB header")
    magic, version, length = struct.unpack_from("<III", raw)
    if (magic, version, length) != (0x46546C67, 2, len(raw)):
        raise ValueError("invalid GLB header")
    chunks = {}
    cursor = 12
    while cursor < len(raw):
        if len(raw) - cursor < 8:
            raise ValueError("truncated GLB chunk header")
        size, kind = struct.unpack_from("<II", raw, cursor)
        cursor += 8
        if cursor + size > len(raw):
            raise ValueError("truncated GLB chunk")
        if kind in chunks:
            raise ValueError("duplicate GLB chunk")
        chunks[kind] = raw[cursor:cursor + size]
        cursor += size
    document = json.loads(chunks[0x4E4F534A])
    binary = chunks[0x004E4942]
    if len(document["buffers"]) != 1 or "uri" in document["buffers"][0]:
        raise ValueError("one embedded GLB buffer required")

    def accessor(index):
        a = document["accessors"][index]
        if "sparse" in a:
            raise ValueError("sparse accessor unsupported in this reference bake")
        view = document["bufferViews"][a["bufferView"]]
        if view["buffer"] != 0:
            raise ValueError("external buffer unsupported")
        count = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[a["type"]]
        fmt = "<" + {5126: "f", 5125: "I", 5123: "H", 5121: "B"}[a["componentType"]] * count
        size = struct.calcsize(fmt)
        if a["count"] < 1 or a.get("normalized", False):
            raise ValueError("nonempty unnormalized accessor required")
        offset = a.get("byteOffset", 0)
        stride = view.get("byteStride", size)
        if stride < size or offset + (a["count"] - 1) * stride + size > view["byteLength"]:
            raise ValueError("accessor exceeds its buffer view")
        start = view.get("byteOffset", 0) + offset
        if view.get("byteOffset", 0) + view["byteLength"] > len(binary):
            raise ValueError("buffer view exceeds GLB binary")
        return [struct.unpack_from(fmt, binary, start + i * stride) for i in range(a["count"])]

    # Match Cubes/build.rs: mesh-local coordinates, not the GLB node transform.
    triangles = []
    for mesh in document["meshes"]:
        for primitive in mesh["primitives"]:
            if primitive.get("mode", 4) != 4:
                raise ValueError("reference must contain triangle lists")
            positions = accessor(primitive["attributes"]["POSITION"])
            normals = accessor(primitive["attributes"]["NORMAL"])
            indices = [v[0] for v in accessor(primitive["indices"])]
            if len(indices) % 3 or len(positions) != len(normals):
                raise ValueError("invalid reference geometry")
            if any(not isinstance(j, int) or j < 0 or j >= len(positions) for j in indices):
                raise ValueError("invalid reference index")
            for i in range(0, len(indices), 3):
                triangles.append(tuple((positions[j], normals[j]) for j in indices[i:i + 3]))
    if not triangles:
        raise ValueError("empty reference")
    return raw, triangles


def carousel_colors():
    colors = {0x7fff}
    for path in sorted((ROOT / "Cube/Assets").glob("*.cubes")):
        data = path.read_bytes()
        for i in range(data[10]):
            rgb = data[16+i*4:19+i*4]
            colors.add(sum(((v*31+127)//255) << (a*5) for a,v in enumerate(rgb)))
    if len(colors) > 512:
        raise ValueError("carousel palette exceeds 512 colors")
    return sorted(colors)


def carousel_shader(colors):
    # Immediate selections keep this relocation-free: an indirectly indexed
    # constant array creates an unsupported native constant-data section.
    choices = "\n".join(f" if (index == {i}u) rgb = {color}u;" for i,color in enumerate(colors[1:],1))
    return f"vec3 carouselColor(uint index) {{\n uint rgb = {colors[0]}u;\n{choices}\n return vec3(rgb&31u,(rgb>>5u)&31u,(rgb>>10u)&31u)/31.0;\n}}\n"


def write_sources(source: Path, out: Path, palette: Path = PALETTE):
    raw, triangles = geometry(source)
    palette_raw, materials = load_palette(palette)
    asset_colors = carousel_colors()
    out.mkdir(parents=True, exist_ok=True)
    # uintBitsToFloat preserves every reference float bit, including signed
    # zero. Constants belong to shader code, never a runtime vertex mesh.
    def vec(v):
        words = struct.unpack("<3I", struct.pack("<3f", *v))
        return "uvec3(" + ",".join(f"0x{x:08x}u" for x in words) + ")"

    positions = [v[0] for triangle in triangles for v in triangle]
    normals = [v[1] for triangle in triangles for v in triangle]
    # The patch list still has 44 × 3 corner occurrences, but geometric
    # positions are first-class canonical identities. Use float bits, not
    # Python float equality, so signed zero remains a distinct shader value.
    # Insertion order makes the canonical table stable without a runtime
    # vertex buffer or a shader constant-data allocation.
    canonical_positions = []
    canonical_position_ids = {}
    position_ids = []
    for position in positions:
        key = struct.pack("<3f", *position)
        position_id = canonical_position_ids.get(key)
        if position_id is None:
            position_id = len(canonical_positions)
            canonical_position_ids[key] = position_id
            canonical_positions.append(position)
        position_ids.append(position_id)
    if len(canonical_positions) != len(canonical_position_ids):
        raise ValueError("canonical position table lost a reference position")

    # Dynamic arrays become shader-constant A64 loads in ANV. Explicit cases
    # keep this prototype's values in instruction immediates and preserve the
    # relocation-free HS contract.
    position_cases = "".join(
        f"case {i}: p={vec(position)}; break;\n"
        for i, position in enumerate(canonical_positions)
    )
    corner_position_cases = "".join(
        f"case {corner}: return {position_id};\n"
        for corner, position_id in enumerate(position_ids)
    )
    normal_cases = "".join(
        f"case {corner}: n={vec(normal)}; break;\n"
        for corner, normal in enumerate(normals)
    )
    (out / "cube.vert").write_text('''#version 450
layout(location=0) in vec3 seed;
layout(location=0) out float instanceID;
layout(std430, set=0, binding=0) readonly buffer Camera {
    mat4 view; mat4 projection; mat4 viewProjection;
} camera;
// The existing retained-transform ABI: 208 bytes / 13 vec4s per instance.
layout(std430, set=0, binding=1) readonly buffer Instances { vec4 rows[]; } instances;
layout(std430, set=0, binding=2) readonly buffer Compacted { uint ids[]; } compacted;
void main() {
    uint id = compacted.ids[gl_InstanceIndex];
    instanceID = float(id + 1u);
    uint base = id * 13u;
    mat4 model = mat4(instances.rows[base], instances.rows[base+1u],
                      instances.rows[base+2u], instances.rows[base+3u]);
    vec4 center = model * vec4(seed, 1.0);
    vec4 clip = camera.viewProjection * center;
    // Before tessellation, Position.w is the positive uniform cube scale.
    // Tiny positive scales encode flat seed markers; zero culls behind the eye.
    gl_Position = vec4(center.xyz, clip.w > 0.0 ? length(model[0].xyz) : 0.0);
}
''')
    (out / "cube.tesc").write_text('''#version 450
layout(vertices=3) out;
layout(location=0) in float instanceID[];
layout(location=0) out vec4 controlNormal[];
''' + '''
int triangleCornerToPositionID(int primitiveID, int invocationID) {
    int corner = primitiveID * 3 + invocationID;
    switch (corner) {
''' + corner_position_cases + '''
    }
    return 0;
}
vec3 cubePosition(int positionID) {
    uvec3 p = uvec3(0);
    switch (positionID) {
''' + position_cases + '''
    }
    return uintBitsToFloat(p);
}
vec3 triangleCornerNormal(int primitiveID, int invocationID) {
    int corner = primitiveID * 3 + invocationID;
    uvec3 n = uvec3(0);
    switch (corner) {
''' + normal_cases + '''
    }
    return uintBitsToFloat(n);
}
void main() {
    float scale = gl_in[0].gl_Position.w;
    if (scale < 0.001) {
        // Indicator only: two triangles form a small XY square. No cube
        // corners are evaluated for inactive seeds. CPU encodes half-size / 1000.
        int c = gl_PrimitiveID * 3 + gl_InvocationID;
        vec2 offset = vec2(-1,-1);
        if (c == 1 || c == 4) offset = vec2(1,-1);
        if (c == 2 || c == 3) offset = vec2(-1,1);
        if (c == 5) offset = vec2(1,1);
        gl_out[gl_InvocationID].gl_Position = vec4(
            gl_in[0].gl_Position.xyz, 1);
        // Preserve the seed ID for DS colour selection while keeping a
        // negative W distinct from ordinary cube normals.
        controlNormal[gl_InvocationID] = vec4(offset, scale * 1000.0, -instanceID[0]);
        if (gl_InvocationID == 0) {
            float level = scale > 0.0 && gl_PrimitiveID < 2 ? 1.0 : 0.0;
            gl_TessLevelOuter[0] = level;
            gl_TessLevelOuter[1] = level;
            gl_TessLevelOuter[2] = level;
            gl_TessLevelInner[0] = level;
        }
        return;
    }
    int positionID = triangleCornerToPositionID(gl_PrimitiveID, gl_InvocationID);
    vec3 p = cubePosition(positionID);
    vec3 n = triangleCornerNormal(gl_PrimitiveID, gl_InvocationID);
    gl_out[gl_InvocationID].gl_Position =
        vec4(p, 1.0);
    controlNormal[gl_InvocationID] = vec4(n, instanceID[0]);
    if (gl_InvocationID == 0) {
        gl_TessLevelOuter[0] = 1.0;
        gl_TessLevelOuter[1] = 1.0;
        gl_TessLevelOuter[2] = 1.0;
        gl_TessLevelInner[0] = 1.0;
    }
}
''')
    (out / "cube.tese").write_text('''#version 450
layout(triangles, equal_spacing, ccw) in;
layout(location=0) in vec4 controlNormal[];
layout(location=0) out vec4 surfaceColor;
layout(location=1) out vec4 surfaceNormal;
layout(location=2) out vec4 surfaceView;
layout(location=3) out vec4 surfaceLight;
// Retained camera ABI: view-projection starts at byte 128.
layout(std430, set=0, binding=0) readonly buffer Camera {
    mat4 view;
    mat4 projection;
    mat4 viewProjection;
    vec4 position_near;
} camera;
layout(std430, set=0, binding=1) readonly buffer Instances { vec4 rows[]; } instances;
''' + palette_shader(materials) + carousel_shader(asset_colors) + '''
void main() {
    vec3 b = gl_TessCoord;
    vec4 p = b.x * gl_in[0].gl_Position
           + b.y * gl_in[1].gl_Position
           + b.z * gl_in[2].gl_Position;
    vec3 normal = normalize(b.x * controlNormal[0].xyz
                         + b.y * controlNormal[1].xyz
                         + b.z * controlNormal[2].xyz);
    // The ordinary (Key-2) material stays fully opaque neutral mid-gray.
    vec3 baseColor = vec3(0.5);
    float alpha = 1.0;
    // Neutral bevels and legacy world/room colors keep the diffuse shader.
    // Key 2 square faces and Key 7 use the imported material response.
    float roughness = -1.0;
    float metallic = 0.0;
    bool hidden = false;
    bool marker = controlNormal[0].w < 0.0;
    if (marker) {
        vec3 marker = b.x*controlNormal[0].xyz + b.y*controlNormal[1].xyz + b.z*controlNormal[2].xyz;
        vec3 right=vec3(camera.view[0][0],camera.view[1][0],camera.view[2][0]);
        vec3 up=vec3(camera.view[0][1],camera.view[1][1],camera.view[2][1]);
        p.xyz += (right*marker.x + up*marker.y)*marker.z;
        normal=vec3(0,1,0);
    }
    if (controlNormal[0].w != 0.0) {
        uint id = uint(abs(controlNormal[0].w)) - 1u;
        uint base = id * 13u;
        uint flags = floatBitsToUint(instances.rows[base+12u].z);
        uint cubie = flags & 31u;
        int sticker = -1;
        int material = -1;
        mat4 model = mat4(instances.rows[base], instances.rows[base+1u],
                          instances.rows[base+2u], instances.rows[base+3u]);
        // The Key 1 sphere colours marker dots and expanded cubes by position
        // on the containing sphere.
        bool carousel = (flags & 57856u) == 25088u; // no RGB555 bit; both showcase bits plus group-1 bit
        if (carousel) {
            if ((flags & 4096u) != 0u) material = int(flags & 7u);
            else baseColor = carouselColor(flags & 511u);
            uint opacity = (flags >> 10u) & 3u;
            alpha = opacity == 1u ? 0.5 : opacity == 2u ? 0.25 : opacity == 3u ? 0.35 : 1.0;
        } else if ((flags & 32768u) != 0u) {
            baseColor = vec3(flags & 31u, (flags >> 5u) & 31u, (flags >> 10u) & 31u) / 31.0;
        // Key 7 combines the otherwise-exclusive room and sphere flags. Each
        // cube uses one palette color and one data-only surface finish.
        } else if ((flags & 24576u) == 24576u) {
            // Material identity survives visibility compaction and mining.
            material = int(flags & 7u);
        } else if ((flags & 16384u) != 0u) {
            baseColor = 0.5 + 0.5 * normalize(model[3].xyz);
        // Key 1 retains its legacy six-wall palette. Whole cubes remain opaque.
        } else if ((flags & 8192u) != 0u && id < 600u) {
            uint wall = id / 100u;
            if (wall == 0u) baseColor = vec3(1,0.025,0.015);
            if (wall == 1u) baseColor = vec3(1,0.28,0.015);
            if (wall == 2u) baseColor = vec3(1,1,1);
            if (wall == 3u) baseColor = vec3(1,0.85,0.015);
            if (wall == 4u) baseColor = vec3(0.02,0.8,0.08);
            if (wall == 5u) baseColor = vec3(0.02,0.08,1);
        // Face colors follow local cube orientation through every turn.
        // Key 2 colors all square faces; the world companion keeps outer stickers.
        } else if ((flags & 256u) != 0u && cubie < 27u) {
            // Only the six sticker faces are translucent; bevels and the
            // baseline cube material remain fully opaque.
            uvec3 cell = uvec3(cubie % 3u, (cubie / 3u) % 3u, cubie / 9u);
            bool allFaces = (flags & 128u) != 0u;
            if (normal.x > 0.9999 && (allFaces || cell.x == 2u)) sticker=0;
            if (normal.x < -0.9999 && (allFaces || cell.x == 0u)) sticker=1;
            if (normal.y > 0.9999 && (allFaces || cell.y == 2u)) sticker=2;
            if (normal.y < -0.9999 && (allFaces || cell.y == 0u)) sticker=3;
            if (normal.z > 0.9999 && (allFaces || cell.z == 2u)) sticker=4;
            if (normal.z < -0.9999 && (allFaces || cell.z == 0u)) sticker=5;
            material = sticker;
        }
        if (material >= 0) paletteMaterial(uint(material), baseColor, roughness, metallic);
        bool transparentPass = (flags & 32768u) == 0u && (flags & 512u) != 0u;
        hidden = !carousel && (transparentPass ? (sticker < 0 || sticker != int((flags >> 10u) & 7u)) : sticker >= 0);
        if (sticker >= 0) alpha=0.35;
        if (!marker) {
            p = model * p;
            normal = normalize(mat3(model) * normal); // positive uniform scale only
        }
    }
    // The single sun follows the camera orientation: above, behind, and
    // slightly to the player's right. Rows of the world-to-view rotation
    // are the camera's world-space right/up/back axes (GLSL indexes columns
    // first). Ignore translation so this remains a directional light.
    vec3 cameraRight = vec3(camera.view[0][0],camera.view[1][0],camera.view[2][0]);
    vec3 cameraUp = vec3(camera.view[0][1],camera.view[1][1],camera.view[2][1]);
    vec3 cameraBack = vec3(camera.view[0][2],camera.view[1][2],camera.view[2][2]);
    surfaceLight = vec4(cameraRight*0.35 + cameraUp*0.80 + cameraBack*0.45, 0.0);
    surfaceColor = vec4(baseColor, roughness);
    surfaceNormal = vec4(normal, metallic);
    surfaceView = vec4(camera.position_near.xyz - p.xyz, alpha);
    gl_Position = camera.viewProjection * p;
    // Every vertex of the excluded planar triangle clips together. No PS
    // discard/early-depth side effects and no opaque depth written for glass.
    if (hidden) gl_Position=vec4(0,0,2,1);
}
''')
    (out / "cube.frag").write_text('''#version 450
layout(location=0) in vec4 surfaceColor;
layout(location=1) in vec4 surfaceNormal;
layout(location=2) in vec4 surfaceView;
layout(location=3) in vec4 surfaceLight;
layout(location=0) out vec4 color;
void main() {
    vec3 baseColor = surfaceColor.rgb;
    vec3 normal = normalize(surfaceNormal.xyz);
    vec3 light = normalize(surfaceLight.xyz);
    float nDotL = max(dot(normal, light), 0.0);
    float sky = 0.18 + 0.12 * max(normal.y, 0.0);
    if (surfaceColor.a < 0.0) {
        // Same diffuse response in modes 1-5; only the sun direction changed.
        color = vec4(baseColor * (sky + nDotL * 0.82), surfaceView.a);
        return;
    }
    float roughness = clamp(surfaceColor.a, 0.045, 1.0);
    float metallic = clamp(surfaceNormal.a, 0.0, 1.0);
    vec3 viewDelta = surfaceView.xyz;
    vec3 view = viewDelta*inversesqrt(max(dot(viewDelta,viewDelta), 0.00000001));
    vec3 halfDelta = light + view;
    vec3 halfVector = halfDelta*inversesqrt(max(dot(halfDelta,halfDelta), 0.00000001));
    float nDotV = max(dot(normal, view), 0.0001);
    float nDotH = max(dot(normal, halfVector), 0.0);
    float vDotH = max(dot(view, halfVector), 0.0);
    float alpha = roughness * roughness;
    float alphaSquared = alpha * alpha;
    float denominator = nDotH*nDotH*(alphaSquared-1.0)+1.0;
    float distribution = alphaSquared / max(3.14159265*denominator*denominator, 0.0001);
    float k = (roughness+1.0)*(roughness+1.0)/8.0;
    float visibilityV = nDotV / (nDotV*(1.0-k)+k);
    float visibilityL = nDotL / (nDotL*(1.0-k)+k);
    vec3 f0 = mix(vec3(0.04), baseColor, metallic);
    vec3 fresnel = f0 + (1.0-f0)*pow(1.0-vDotH, 5.0);
    vec3 specular = distribution*visibilityV*visibilityL*fresnel
                  / max(4.0*nDotV*nDotL, 0.0001);
    vec3 diffuse = (1.0-fresnel)*(1.0-metallic)*baseColor/3.14159265;
    vec3 ambient = baseColor*sky*(1.0-metallic) + f0*0.04;
    vec3 linearColor = max(ambient + (diffuse+specular)*nDotL*2.6, vec3(0.0));
    // Retained color targets are UNORM; encode the imported material path to sRGB.
    vec3 srgb = mix(12.92*linearColor, 1.055*pow(linearColor,vec3(1.0/2.4))-0.055,
                    greaterThan(linearColor,vec3(0.0031308)));
    color = vec4(srgb, surfaceView.a);
}
''')
    (out / "seed.f32le").write_bytes(struct.pack("<3f", 0, 0, 0))
    (out / "patches.u32le").write_bytes(bytes(len(triangles) * 4))
    manifest = {
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "palette_sha256": hashlib.sha256(palette_raw).hexdigest(),
        "stored_seed_vertices": 1, "patches": len(triangles),
        "input_control_points": 1, "output_control_points": 3,
        "domain": "triangles", "tessellation_level": 1,
        "reference_triangle_count": len(triangles),
        "canonical_position_count": len(canonical_positions),
        "triangle_corner_count": len(positions),
        "unique_positions": len(canonical_positions), "unique_normals": len(set(normals)),
        "coordinate_space": "mesh-local, matching Cubes/build.rs",
        "carousel_colors_rgb555": asset_colors,
        "runtime_integrated": False, "host_render_verified": False,
        "baremetal_verified": False,
    }
    return manifest


def replace(text, old, new):
    if text.count(old) != 1:
        raise ValueError(f"capture tool contract drift: {old[:80]}")
    return text.replace(old, new, 1)


def make_dumper(out, patches):
    spec = importlib.util.spec_from_file_location("baker", TRUEOS / "tools/helio-intel-bake/bake.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    path = out / "patch_pipeline_dump.c"
    module.make_compile_only_dumper(path)
    c = path.read_text()
    c = replace(c, "argc != 3 && argc != 4", "argc != 5")
    c = replace(c, "const int geometry_enabled = argc == 4;", "const int geometry_enabled = 0;")
    c = replace(c, '.pName = "vs_main",', '.pName = "main",')
    c = replace(c, '.pName = "fs_main",', '.pName = "main",')
    c = replace(c, "if (geometry_enabled) {\n        VkPhysicalDeviceFeatures supported_features;", "if (1) {\n        VkPhysicalDeviceFeatures supported_features;")
    c = replace(c, "supported_features.geometryShader != VK_TRUE", "supported_features.tessellationShader != VK_TRUE")
    c = replace(c, "enabled_features.geometryShader = VK_TRUE;", "enabled_features.tessellationShader = VK_TRUE;")
    c = replace(c, ".pEnabledFeatures = geometry_enabled ? &enabled_features : NULL,", ".pEnabledFeatures = &enabled_features,")
    c = replace(c, "const VkPipelineShaderStageCreateInfo stages[3] = {", '''FileData hs = read_spirv(argv[3]);
    FileData ds = read_spirv(argv[4]);
    VkShaderModule hs_module, ds_module;
    VkShaderModuleCreateInfo hs_info = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = hs.word_count * 4, .pCode = hs.words,
    };
    VkShaderModuleCreateInfo ds_info = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = ds.word_count * 4, .pCode = ds.words,
    };
    CHECK_VK(vkCreateShaderModule(device, &hs_info, NULL, &hs_module));
    CHECK_VK(vkCreateShaderModule(device, &ds_info, NULL, &ds_module));
    const VkPipelineShaderStageCreateInfo stages[4] = {''')
    c = replace(c, '''.stage = VK_SHADER_STAGE_GEOMETRY_BIT,
            .module = gs_module,
            .pName = "main",''', '''.stage = VK_SHADER_STAGE_TESSELLATION_CONTROL_BIT,
            .module = hs_module,
            .pName = "main",
        },
        {
            .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT,
            .module = ds_module,
            .pName = "main",''')
    c = replace(c, ".stageCount = geometry_enabled ? 3u : 2u,", ".stageCount = 4,")
    start = c.index("        .topology = line_adjacency")
    end = c.index("    };", start)
    c = c[:start] + "        .topology = VK_PRIMITIVE_TOPOLOGY_PATCH_LIST,\n" + c[end:]
    c = replace(c, "const VkGraphicsPipelineCreateInfo pipeline_info = {", '''const VkPipelineTessellationStateCreateInfo tessellation = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_TESSELLATION_STATE_CREATE_INFO,
        .patchControlPoints = 1,
    };
    const VkGraphicsPipelineCreateInfo pipeline_info = {''')
    c = replace(c, ".pInputAssemblyState = &input_assembly,", ".pInputAssemblyState = &input_assembly,\n        .pTessellationState = &tessellation,")
    c = replace(c, ".stageFlags = VK_SHADER_STAGE_VERTEX_BIT,", ".stageFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT,")
    c = replace(c, "    const VkDescriptorSetLayoutCreateInfo set_layout_info = {", '''
    VkDescriptorSetLayoutBinding cube_bindings[3] = {camera_binding, camera_binding, camera_binding};
    cube_bindings[1].binding = 1;
    cube_bindings[2].binding = 2;
    cube_bindings[1].stageFlags = VK_SHADER_STAGE_VERTEX_BIT | VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT;
    cube_bindings[2].stageFlags = VK_SHADER_STAGE_VERTEX_BIT;
    const VkDescriptorSetLayoutCreateInfo set_layout_info = {''')
    c = replace(c, '.bindingCount = 1,', '.bindingCount = 3,')
    c = replace(c, '.pBindings = &camera_binding,', '.pBindings = cube_bindings,')
    c = replace(c, 'case VK_SHADER_STAGE_GEOMETRY_BIT:', '''case VK_SHADER_STAGE_TESSELLATION_CONTROL_BIT: return "tess_control";
        case VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT: return "tess_eval";
        case VK_SHADER_STAGE_GEOMETRY_BIT:''')
    c = replace(c, 'helio_pipeline_dump: compiled_only=1', 'cube_patch_pipeline: compiled_only=1')
    c = replace(c, 'printf("cube_patch_pipeline: compiled_only=1\\n");\n    return 0;',
                'printf("cube_patch_pipeline: compiled_only=1\\n");')
    # Record a complete draw through ANV to capture dynamic TE and URB state,
    # but return before vkQueueSubmit. The no-op shim never executes EU code.
    start = c.index("    const VkVertexInputBindingDescription binding = {")
    end = c.index("    const VkPipelineInputAssemblyStateCreateInfo", start)
    c = c[:start] + '''    const VkVertexInputBindingDescription binding = {
        .binding = 0, .stride = 12, .inputRate = VK_VERTEX_INPUT_RATE_VERTEX,
    };
    const VkVertexInputAttributeDescription attribute = {
        .location = 0, .binding = 0, .format = VK_FORMAT_R32G32B32_SFLOAT, .offset = 0,
    };
    const VkPipelineVertexInputStateCreateInfo vertex_input = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
        .vertexBindingDescriptionCount = 1, .pVertexBindingDescriptions = &binding,
        .vertexAttributeDescriptionCount = 1, .pVertexAttributeDescriptions = &attribute,
    };
''' + c[end:]
    # One position at byte 0, zero indices at byte 12, and a camera at an
    # aligned offset. They share one allocation, not one vertex attribute.
    camera_offset = ((12 + patches * 4 + 255) // 256) * 256
    size = camera_offset + 208
    start = c.index("    const float *vertices = line_adjacency")
    end = c.index("    const VkBufferCreateInfo buffer_info", start)
    c = c[:start] + f'''    float seed_and_camera[{size // 4}] = {{0}};
    seed_and_camera[{camera_offset // 4 + 32}] = 0.4f;
    seed_and_camera[{camera_offset // 4 + 37}] = 0.4f;
    seed_and_camera[{camera_offset // 4 + 42}] = 0.2f;
    seed_and_camera[{camera_offset // 4 + 46}] = 0.5f;
    seed_and_camera[{camera_offset // 4 + 47}] = 1.0f;
    const float *vertices = seed_and_camera;
    const size_t vertex_bytes = sizeof(seed_and_camera);
''' + c[end:]
    c = replace(c, ".usage = VK_BUFFER_USAGE_VERTEX_BUFFER_BIT,",
                ".usage = VK_BUFFER_USAGE_VERTEX_BUFFER_BIT | VK_BUFFER_USAGE_INDEX_BUFFER_BIT | VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,")
    c = replace(c, "    const VkCommandBufferBeginInfo begin_info = {", f'''    VkDescriptorPoolSize pool_size = {{VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 3}};
    VkDescriptorPoolCreateInfo descriptor_pool_info = {{
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets = 1, .poolSizeCount = 1, .pPoolSizes = &pool_size,
    }};
    VkDescriptorPool pool;
    CHECK_VK(vkCreateDescriptorPool(device, &descriptor_pool_info, NULL, &pool));
    VkDescriptorSetAllocateInfo set_alloc = {{
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool = pool, .descriptorSetCount = 1, .pSetLayouts = &set_layout,
    }};
    VkDescriptorSet camera_set;
    CHECK_VK(vkAllocateDescriptorSets(device, &set_alloc, &camera_set));
    VkDescriptorBufferInfo camera_buffer = {{vertex_buffer, {camera_offset}, 208}};
    VkWriteDescriptorSet camera_write = {{
        .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = camera_set, .dstBinding = 0, .descriptorCount = 1,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &camera_buffer,
    }};
    vkUpdateDescriptorSets(device, 1, &camera_write, 0, NULL);
    // Valid backing ranges for recording only; no EU code is submitted.
    VkDescriptorBufferInfo instance_buffer = {{vertex_buffer, 0, 208}};
    VkDescriptorBufferInfo compacted_buffer = {{vertex_buffer, 0, 4}};
    camera_write.dstBinding = 1;
    camera_write.pBufferInfo = &instance_buffer;
    vkUpdateDescriptorSets(device, 1, &camera_write, 0, NULL);
    camera_write.dstBinding = 2;
    camera_write.pBufferInfo = &compacted_buffer;
    vkUpdateDescriptorSets(device, 1, &camera_write, 0, NULL);
    const VkCommandBufferBeginInfo begin_info = {{''')
    c = replace(c, "    vkCmdDraw(command_buffer, line_adjacency ? 4u : (geometry_enabled ? 6u : 3u), 1, 0, 0);", f'''
    vkCmdBindDescriptorSets(command_buffer, VK_PIPELINE_BIND_POINT_GRAPHICS,
                            pipeline_layout, 0, 1, &camera_set, 0, NULL);
    vkCmdBindIndexBuffer(command_buffer, vertex_buffer, 12, VK_INDEX_TYPE_UINT32);
    vkCmdDrawIndexed(command_buffer, {patches}, 1, 0, 0, 0);''')
    c = replace(c, "    const VkSubmitInfo submit_info = {", '''    printf("cube_patch_pipeline: commands_recorded=1 submitted=0\\n");
    return 0;
    const VkSubmitInfo submit_info = {''')
    path.write_text(c)
    subprocess.run(["cc", str(path), "-o", str(out / "patch_pipeline_dump"), *module.vulkan_compile_flags()], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / "Cube/cube.glb")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--palette", type=Path, default=PALETTE)
    parser.add_argument("--source-only", action="store_true")
    parser.add_argument("--device-id", choices=["a780", "4680"], default="a780")
    args = parser.parse_args()
    out = args.out.resolve()
    manifest = write_sources(args.source, out, args.palette)
    # Invalidate a previous success before attempting another compile.
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if not args.source_only:
        lane = TRUEOS / ".codex_tmp/trueos-adj-instrumented-rpls"
        compiler = lane / "bootstrap-root/usr/bin/glslang"
        mesa = lane / "mesa-build"
        for stage in ["vert", "frag", "tesc", "tese"]:
            subprocess.run([str(compiler), "-V", "--target-env", "vulkan1.1", str(out / f"cube.{stage}"), "-o", str(out / f"cube.{stage}.spv")], check=True)
        make_dumper(out, manifest["patches"])
        driver = mesa / "src/intel/vulkan/libvulkan_intel.so"
        shim = mesa / "src/intel/tools/libintel_noop_drm_shim.so"
        if not driver.is_file() or not shim.is_file():
            raise ValueError("pinned ANV compiler and no-op DRM shim required")
        icd = out / "icd.json"
        icd.write_text(json.dumps({"file_format_version": "1.0.1", "ICD": {"api_version": "1.4.346", "library_path": str(driver)}}))
        # Never let a missing capture silently reuse a previous bake's file.
        native = Path(tempfile.mkdtemp(prefix="native-", dir=out))
        env = dict(os.environ, LD_PRELOAD=str(shim), VK_DRIVER_FILES=str(icd), VK_ICD_FILENAMES=str(icd),
                   INTEL_STUB_GPU_DEVICE_ID=args.device_id, TRUEOS_VK_DEVICE_ID="0x" + args.device_id,
                   MESA_SHADER_CACHE_DISABLE="true", TRUEOS_EXECUTABLE_DUMP_DIR=str(native))
        result = subprocess.run([str(out / "patch_pipeline_dump"), *(str(out / f"cube.{stage}.spv") for stage in ["vert", "frag", "tesc", "tese"])], env=env, capture_output=True, text=True)
        (out / "compile.log").write_text(result.stdout + result.stderr)
        result.check_returncode()
        if "cube_patch_pipeline: compiled_only=1" not in result.stdout:
            raise ValueError("missing compile-only completion")
        if "cube_patch_pipeline: commands_recorded=1 submitted=0" not in result.stdout:
            raise ValueError("missing command-recording proof")
        captures = ["vertex_TRUEOS_VS_state_v1.txt", "fragment_TRUEOS_PS_state_v1.txt",
                    "tess_control_TRUEOS_HS_state_v1.txt", "tess_eval_TRUEOS_DS_state_v1.txt",
                    "tessellation_TRUEOS_URB_state_v1.txt", "tessellation_TRUEOS_TE_state_v1.txt"]
        for name in captures:
            if not (native / name).is_file():
                raise ValueError(f"missing compiler state capture: {name}; apply the tessellation capture patch")
        for stage in ["tess_control", "tess_eval"]:
            kind = "HS" if stage == "tess_control" else "DS"
            if "scratch_bytes=0" not in (native / f"{stage}_TRUEOS_{kind}_state_v1.txt").read_text():
                raise ValueError(f"{stage} requires unsupported scratch allocation")
        hs_isa = list(native.glob("*_tess_control_*_GEN_Assembly.txt"))
        if len(hs_isa) != 1 or "A64" in hs_isa[0].read_text():
            raise ValueError("HS instruction-only corner contract not satisfied")
        manifest["native_directory"] = native.name
        manifest["commands_recorded"] = True
        manifest["gpu_submitted"] = False
        manifest["capture_sha256"] = {n: hashlib.sha256((native / n).read_bytes()).hexdigest() for n in captures}
        manifest["compiled_device_id"] = "0x" + args.device_id
        manifest["spirv_sha256"] = {s: hashlib.sha256((out / f"cube.{s}.spv").read_bytes()).hexdigest() for s in ["vert", "tesc", "tese", "frag"]}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
