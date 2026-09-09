/*
MIT License

Copyright (c) 2026 INSIDE / BOX contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/
// Generated port of Mandelbox.html; see tools/export_chroma.py.
// Both complete spherical presets retain the reference's 120 steps / 3 AO taps.
// iDate.xyz = packed authored sRGB; .w = 0 Folded Core, 1 Box Cathedral.
// iSampleRate = 1..3 colors. Pixels form a 3x2 cubemap with a one-texel gutter.
// World identity also changes the Cathedral's recursive geometry. These six
// fixed directions belong to the authored themes, not the output tint. Blends
// use every active theme once; padded palette slots have no influence.
vec3 themeShapeColor(float rgb) {
    if (rgb==6539250.0) return vec3( 0.65, 0.15,-0.40); // sky 63c7f2
    if (rgb==8014640.0) return vec3(-0.50, 0.80, 0.20); // underground 7a4b30
    if (rgb==2430269.0) return vec3( 0.30,-0.65, 0.90); // black-hole 25153d
    if (rgb==16050342.0) return vec3(-0.20,-0.50,-0.70); // white-hole f4e8a6
    if (rgb==5156712.0) return vec3( 0.80, 0.60,-0.10); // island 4eaf68
    if (rgb==14116199.0) return vec3(-0.70, 0.20, 0.60); // city d76567
    return vec3(0.0);
}
vec3 themeShape() {
    if (int(iDate.w)==0) return vec3(0.0); // Void stays the original Folded Core.
    vec3 shape=themeShapeColor(iDate.x);
    if (iSampleRate>1.0) shape+=themeShapeColor(iDate.y);
    if (iSampleRate>2.0) shape+=themeShapeColor(iDate.z);
    return shape/clamp(iSampleRate,1.0,3.0);
}
// One rigid rotation at each recursive scale, increasing toward fine detail.
// Rotate before repetition: apertures/solid intersections change, not merely
// the camera or pigment. A unit quaternion preserves the distance bound, so
// the marcher, normals and AO all see the same conservative geometry.
vec3 themeFold(vec3 p, vec3 shape, float level) {
    vec3 turn=shape*(0.28+0.11*level);
    vec4 q=vec4(turn,1.0)/sqrt(1.0+dot(turn,turn));
    return p+2.0*cross(q.xyz,cross(q.xyz,p)+q.w*p);
}

const vec3 CAMERA = vec3(0.08,-0.12,0.05);
const float FAR = 44.0;
float max3(vec3 v) { return max(v.x,max(v.y,v.z)); }

// Exact zero isosurface of the uploaded mesh, in local coordinates:
// 24 unique vertices = signed permutations of (1,0.8,0.8).
// Six face planes, twelve edge-bevel planes, eight corner planes.
// This is a conservative distance BOUND, not an exact Euclidean SDF outside.
float oneCubeUnit(vec3 p) {
    vec3 a=abs(p);
    float faces=max3(a)-1.0;
    float edges=max3(vec3(a.x+a.y,a.x+a.z,a.y+a.z))-1.8;
    float corners=a.x+a.y+a.z-2.6;
    return max(faces,max(edges*0.70710678,corners*0.57735027));
}
float oneCube(vec3 p,float size) { return oneCubeUnit(p/size)*size; }
float octagon(vec2 q) {
    // Projection of the same 20%-inset seed onto an axis-aligned face.
    return max(max(q.x,q.y)-1.0,(q.x+q.y-1.8)*0.70710678);
}

// Original cathedral's four recursive scales and factor-three repetition.
// Each level is now bounded by the 24-vertex seed; apertures have flat bevels.
// Thus the uploaded silhouette recurs throughout, rather than only enclosing it.
vec2 cathedral(vec3 p, vec3 shape) {
    p/=7.2;
    float d=-100.0, scale=1.0, detail=0.0;
    for (int i=0;i<4;++i) {
        vec3 a=mod(themeFold(p,shape,float(i))*scale,2.0)-1.0;
        float seed=oneCubeUnit(a)/scale;
        vec3 r=abs(1.0-3.0*abs(a));
        float cut=min(octagon(r.xy),min(octagon(r.yz),octagon(r.zx)))/(3.0*scale);
        float level=max(seed,cut);
        if (level>d) { d=level; detail=float(i); }
        scale*=3.0;
    }
    return vec2(d*7.2,detail);
}

// Exact Euclidean projection onto the convex 24-vertex seed, then reflect.
// The seed is [-0.8,0.8]^3 plus an L1 ball of radius 0.2. Projecting therefore
// reduces to soft-thresholding the three positive "excess beyond 0.8" values.
// 2*projection(z)-z is the beveled equivalent of the original box fold. Unlike
// sequential plane reflections, it preserves escaping orbits far from the seed.
vec3 seedFold(vec3 z) {
    vec3 a=abs(z);
    vec3 excess=max(a-0.8,0.0);
    float largest=max3(excess);
    float smallest=min(excess.x,min(excess.y,excess.z));
    float sum=excess.x+excess.y+excess.z;
    float threshold=max(0.0,max(largest-0.2,
                    max((sum-smallest-0.2)*0.5,(sum-0.2)/3.0)));
    vec3 projected=sign(z)*(min(a,0.8)+max(excess-threshold,0.0));
    return projected*2.0-z;
}

// Original Folded core base: scale 3, sphere-fold amplification 1..4,
// nine recurrences, symmetric world-space aspect ratio. Bevel-seeded adaptation,
// not a claim that the original mathematical Mandelbox remains unchanged.
vec2 foldedCore(vec3 p) {
    vec3 q=mod(p+vec3(0.0,0.0,3.525)+9.4,18.8)-9.4;
    vec3 c=q/2.35, z=c;
    float derivative=1.0, orbit=10.0;
    for (int i=0;i<9;++i) {
        z=seedFold(z);
        float k=clamp(1.0/max(dot(z,z),0.00001),1.0,4.0);
        z*=k; derivative*=k;
        z=3.0*z+c; derivative=3.0*derivative+1.0;
        orbit=min(orbit,abs(oneCubeUnit(z)));
    }
    float d=length(z)/derivative*2.35-0.0035;
    // A small chamfered safety pocket keeps the fixed observer in empty space.
    d=max(d,-oneCube(p,0.62));
    return vec2(d,clamp(orbit*3.0,0.0,3.0));
}
vec2 map(vec3 p, vec3 shape) {
    vec2 inner=int(iDate.w)==0 ? foldedCore(p) : cathedral(p,shape);
    float shell=-oneCube(p,17.5);
    if (shell<inner.x) return vec2(shell,4.0);
    return inner;
}
vec3 cubeRay(vec2 uv, int face) {
    if (face==0) return normalize(vec3(1.0,-uv.y,-uv.x));
    if (face==1) return normalize(vec3(-1.0,-uv.y,uv.x));
    if (face==2) return normalize(vec3(uv.x,1.0,uv.y));
    if (face==3) return normalize(vec3(uv.x,-1.0,-uv.y));
    if (face==4) return normalize(vec3(uv.x,-uv.y,1.0));
    return normalize(vec3(-uv.x,-uv.y,-1.0));
}
vec3 normalAt(vec3 p,float eps,vec3 shape) {
    vec2 e=vec2(1.0,-1.0)*eps;
    return normalize(e.xyy*map(p+e.xyy,shape).x+e.yyx*map(p+e.yyx,shape).x+
                     e.yxy*map(p+e.yxy,shape).x+e.xxx*map(p+e.xxx,shape).x);
}
float occlusion(vec3 p,vec3 n,vec3 shape) {
    float a=0.0,weight=1.0;
    for (int i=1;i<=3;++i) {
        float h=0.10+float(i)*0.18;
        a+=(h-map(p+n*h,shape).x)*weight; weight*=0.52;
    }
    return clamp(1.0-1.75*a,0.18,1.0);
}
vec4 bakeMaterial(vec3 rd,vec3 shape) {
    float travel=0.0, hit=0.0, epsilon=0.002;
    vec2 value=vec2(1.0);
    for (int i=0;i<120;++i) {
        value=map(CAMERA+rd*travel,shape);
        epsilon=max(0.0015,travel*0.0005);
        if (value.x<epsilon) { hit=1.0; break; }
        travel+=max(value.x*0.68,0.0009);
        if (travel>FAR) break;
    }
    if (hit<0.5) { return vec4(0.0); }
    vec3 p=CAMERA+rd*travel;
    vec3 n=normalAt(p,max(0.0012,epsilon*1.1),shape);
    float ao=occlusion(p,n,shape);
    float facing=max(dot(n,-rd),0.0);
    float key=max(dot(n,normalize(vec3(-0.45,0.70,-0.55))),0.0);
    float fill=max(dot(n,normalize(vec3(0.8,-0.2,0.4))),0.0);
    float tint=0.85+0.15*cos(value.y*1.8);
    float light=(0.075+0.30*facing+0.65*key+0.14*fill)*ao*tint;
    vec3 lightDirection=normalize(vec3(0.5,1.8,-0.2)-p);
    light+=max(dot(n,lightDirection),0.0)/(1.0+0.025*travel*travel)*0.30;
    light+=pow(1.0-facing,3.0)*0.055*ao;
    // Surface marks stay fixed in world space and survive every palette change.
    vec3 nearest=abs(fract(p*0.42+0.5)-0.5), absN=abs(n);
    float seam=absN.x>absN.y && absN.x>absN.z ? min(nearest.y,nearest.z)
               : absN.y>absN.z ? min(nearest.x,nearest.z) : min(nearest.x,nearest.y);
    light*=1.0-(1.0-smoothstep(0.008,0.024,seam))*0.13;
    float band=1.0-smoothstep(0.016,0.033,abs(fract(p.y*0.23+0.11)-0.5));
    light+=band*0.11*ao;
    // Geometry-aware material fields: broad faces, bevels, recursive recesses.
    // Their weights are baked, never tied to the screen or camera rotation.
    float bevel=1.0-smoothstep(0.86,0.995,max3(absN));
    float story=0.5+0.5*sin(value.y*2.1+p.y*0.45+p.z*0.21);
    float third=smoothstep(0.36,0.88,story)*0.82;
    float second=(1.0-third)*mix(0.04,0.91,bevel);
    return vec4(sqrt(clamp(light/1.6,0.0,1.0)),second,third,exp(-travel*0.039));
}


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
    float light=material.r*material.r*1.6;
    float wb=material.g,wc=material.b,wa=max(0.0,1.0-wb-wc);
    vec3 a=linearColor(iDate.x);
    vec3 b=iSampleRate>1 ? linearColor(iDate.y) : a;
    vec3 c=iSampleRate>2 ? linearColor(iDate.z) : (iSampleRate>1 ? mix(a,b,0.70) : a);
    // Authored sRGB colors are converted once, as part of the bake.
    // A modest neutral component retains material readability and bright facets.
    vec3 pigment=wa*a+wb*b+wc*c;
    vec3 color=(pigment*0.67+0.028)*light;
    color+=vec3(pow(max(light-0.72,0.0),2.0)*0.16);
    vec3 fog=0.0025+mix(a,c,0.35)*0.012;
    color=mix(fog,color,material.a);
    color=1.0-exp(-color*1.65);
    color=pow(max(color,0.0),vec3(1.0/2.2));
    fragColor=vec4(color,1.0);
}
