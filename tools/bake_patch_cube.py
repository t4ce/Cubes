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
import os
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TRUEOS = ROOT.parent / "TRUEOS"


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


def write_sources(source: Path, out: Path):
    raw, triangles = geometry(source)
    out.mkdir(parents=True, exist_ok=True)
    # uintBitsToFloat preserves every reference float bit, including signed
    # zero. Constants belong to shader code, never a runtime vertex mesh.
    def vec(v):
        words = struct.unpack("<3I", struct.pack("<3f", *v))
        return "uvec3(" + ",".join(f"0x{x:08x}u" for x in words) + ")"

    positions = [v[0] for triangle in triangles for v in triangle]
    normals = [v[1] for triangle in triangles for v in triangle]
    # Dynamic arrays become shader-constant A64 loads in ANV. Explicit cases
    # keep this prototype's geometry in instruction immediates and avoid a
    # hidden constant-data allocation/relocation contract in the HS.
    cases = ""
    for i, (position, normal) in enumerate(zip(positions, normals)):
        cases += f"case {i}: p={vec(position)}; n={vec(normal)}; break;\n"
    (out / "cube.vert").write_text('''#version 450
layout(location=0) in vec3 seed;
void main() { gl_Position = vec4(seed, 1.0); }
''')
    (out / "cube.tesc").write_text('''#version 450
layout(vertices=3) out;
layout(location=0) out vec3 controlNormal[];
''' + '''
void main() {
    int corner = gl_PrimitiveID * 3 + gl_InvocationID;
    uvec3 p = uvec3(0), n = uvec3(0);
    switch (corner) {
''' + cases + '''
    }
    gl_out[gl_InvocationID].gl_Position =
        vec4(gl_in[0].gl_Position.xyz + uintBitsToFloat(p), 1.0);
    controlNormal[gl_InvocationID] = uintBitsToFloat(n);
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
layout(location=0) in vec3 controlNormal[];
layout(location=0) out vec3 worldNormal;
// Retained camera ABI: view-projection starts at byte 128.
layout(std430, set=0, binding=0) readonly buffer Camera {
    mat4 view;
    mat4 projection;
    mat4 viewProjection;
} camera;
void main() {
    vec3 b = gl_TessCoord;
    vec4 p = b.x * gl_in[0].gl_Position
           + b.y * gl_in[1].gl_Position
           + b.z * gl_in[2].gl_Position;
    worldNormal = b.x * controlNormal[0]
                + b.y * controlNormal[1]
                + b.z * controlNormal[2];
    gl_Position = camera.viewProjection * p;
}
''')
    (out / "cube.frag").write_text('''#version 450
layout(location=0) in vec3 worldNormal;
layout(location=0) out vec4 color;
// Same material-0 lighting as the imported Cubes retained forward path.
// Cubes uses an identity object transform and material_id=0.
void main() {
    vec3 normal = normalize(worldNormal);
    vec3 lightDirection = normalize(vec3(0.35, 0.80, 0.45));
    float diffuse = max(dot(normal, lightDirection), 0.0);
    float sky = 0.18 + 0.12 * max(normal.y, 0.0);
    color = vec4(vec3(0.25, 0.70, 1.0) * (sky + diffuse * 0.82), 1.0);
}
''')
    (out / "seed.f32le").write_bytes(struct.pack("<3f", 0, 0, 0))
    (out / "patches.u32le").write_bytes(bytes(len(triangles) * 4))
    manifest = {
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "stored_seed_vertices": 1, "patches": len(triangles),
        "input_control_points": 1, "output_control_points": 3,
        "domain": "triangles", "tessellation_level": 1,
        "reference_triangle_count": len(triangles),
        "unique_positions": len(set(positions)), "unique_normals": len(set(normals)),
        "coordinate_space": "mesh-local, matching Cubes/build.rs",
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
    c = replace(c, ".stageFlags = VK_SHADER_STAGE_VERTEX_BIT,", ".stageFlags = VK_SHADER_STAGE_TESSELLATION_EVALUATION_BIT,")
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
    size = camera_offset + 192
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
    c = replace(c, "    const VkCommandBufferBeginInfo begin_info = {", f'''    VkDescriptorPoolSize pool_size = {{VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1}};
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
    VkDescriptorBufferInfo camera_buffer = {{vertex_buffer, {camera_offset}, 192}};
    VkWriteDescriptorSet camera_write = {{
        .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = camera_set, .dstBinding = 0, .descriptorCount = 1,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &camera_buffer,
    }};
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
    parser.add_argument("--source-only", action="store_true")
    parser.add_argument("--device-id", choices=["a780", "4680"], default="a780")
    args = parser.parse_args()
    out = args.out.resolve()
    manifest = write_sources(args.source, out)
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
