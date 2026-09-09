#!/usr/bin/env python3
"""Port the supplied Chroma geometry/materials, with a resident cubemap view ABI."""
from pathlib import Path
import re
import sys

APP = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(APP.parent / "TRUEOS/tools/shadertoy-cpp-offline"))
from adapter import adapt


def glsl_source():
    html = (APP / "Cube/Mandelbox.html").read_text()
    license_text = re.search(r"<!--\s*(MIT License.*?)-->", html, re.S).group(1).strip()
    bake = re.search(r'<script id="bake-shader"[^>]*>(.*?)</script>', html, re.S).group(1)
    bake = bake[bake.index("const vec3 CAMERA"):]
    bake = bake.replace("uMode", "int(iDate.w)")
    bake = bake.replace("vec3 cubeRay(vec2 uv)", "vec3 cubeRay(vec2 uv, int face)").replace("uFace", "face")
    bake = bake.replace("void main() {\n    vec3 rd=cubeRay(gl_FragCoord.xy/uResolution*2.0-1.0);", "vec4 bakeMaterial(vec3 rd) {")
    bake = bake.replace("{ gl_FragColor=vec4(0.0); return; }", "{ return vec4(0.0); }")
    bake = bake.replace("gl_FragColor=", "return ")
    # Keep the licensed reference intact. Thread one bake-only shape value
    # through distance, normal and AO evaluation; sampling never sees it.
    shape = (APP / "Cube/mandelbox/theme_shape.glsl").read_text()
    replacements = {
        "vec2 cathedral(vec3 p)": "vec2 cathedral(vec3 p, vec3 shape)",
        "mod(p*scale,2.0)": "mod(themeFold(p,shape,float(i))*scale,2.0)",
        "vec2 map(vec3 p)": "vec2 map(vec3 p, vec3 shape)",
        "cathedral(p)": "cathedral(p,shape)",
        "vec3 normalAt(vec3 p,float eps)": "vec3 normalAt(vec3 p,float eps,vec3 shape)",
        "float occlusion(vec3 p,vec3 n)": "float occlusion(vec3 p,vec3 n,vec3 shape)",
        "vec4 bakeMaterial(vec3 rd)": "vec4 bakeMaterial(vec3 rd,vec3 shape)",
        "map(p+e.xyy)": "map(p+e.xyy,shape)",
        "map(p+e.yyx)": "map(p+e.yyx,shape)",
        "map(p+e.yxy)": "map(p+e.yxy,shape)",
        "map(p+e.xxx)": "map(p+e.xxx,shape)",
        "map(p+n*h)": "map(p+n*h,shape)",
        "map(CAMERA+rd*travel)": "map(CAMERA+rd*travel,shape)",
        "normalAt(p,max(0.0012,epsilon*1.1))": "normalAt(p,max(0.0012,epsilon*1.1),shape)",
        "occlusion(p,n)": "occlusion(p,n,shape)",
    }
    for old, new in replacements.items():
        assert old in bake, f"reference changed: {old}"
        bake = bake.replace(old, new)
    bake = shape + "\n" + bake
    view = re.search(r'<script id="view-shader"[^>]*>(.*?)</script>', html, re.S).group(1)
    tint = view[view.index("    float light="):view.index("    vec2 screen=")]
    for name, value in [("uColorA", "linearColor(iDate.x)"), ("uColorB", "linearColor(iDate.y)"),
                        ("uColorC", "linearColor(iDate.z)"), ("uColorCount", "iSampleRate")]:
        tint = re.sub(r"\b" + name + r"\b", value, tint)
    tint = tint.replace("// Palette uniforms are converted from sRGB into linear light by JavaScript.",
                        "// Authored sRGB colors are converted once, as part of the bake.")
    return "/*\n" + license_text + "\n*/\n" + """// Generated port of Mandelbox.html; see tools/export_chroma.py.
// Both complete spherical presets retain the reference's 120 steps / 3 AO taps.
// iDate.xyz = packed authored sRGB; .w = 0 Folded Core, 1 Box Cathedral.
// iSampleRate = 1..3 colors. Pixels form a 3x2 cubemap with a one-texel gutter.
""" + bake + """
float linearChannel(float s) {
    return s<=0.04045 ? s/12.92 : pow((s+0.055)/1.055,2.4);
}
vec3 linearColor(float rgb) {
    vec3 s=vec3(floor(rgb/65536.0),mod(floor(rgb/256.0),256.0),mod(rgb,256.0))/255.0;
    return vec3(linearChannel(s.x),linearChannel(s.y),linearChannel(s.z));
}
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    float stride=iResolution.x/3.0;
    float faceSize=stride-2.0;
    vec2 tile=floor(fragCoord/stride);
    int face=int(tile.x+tile.y*3.0);
    vec2 uv=(mod(fragCoord,stride)-1.0)/faceSize*2.0-1.0;
    vec4 material=bakeMaterial(cubeRay(uv,face),themeShape());
""" + tint + "    fragColor=vec4(color,1.0);\n}\n"


def kernel_source(glsl):
    # Reuse the reviewed GLSL translator and packing, with the same third
    # pointer layout as the existing source-atlas ShaderToy ABI. Only this
    # app-owned epilogue implements cubemap lookup; no generic adapter changes.
    generated = adapt(glsl, kernel_name="shadertoy_mandelbox")
    generated = generated.replace("    float4 timing;", "    float4 timing;\n    uint4 render_control;\n    float4 focus_control;")
    marker = "kernel TRUEOS_REQD_SUB_GROUP_SIZE_16 void shadertoy_mandelbox("
    assert generated.count(marker) == 1
    epilogue = (APP / "Cube/mandelbox/environment.clcpp").read_text()
    if "struct ShaderToyInvocation {" in generated:
        epilogue = epilogue.replace("        mainImage(", "        ShaderToyInvocation invocation = {};\n        invocation.mainImage(")
    return generated[:generated.index(marker)] + epilogue
