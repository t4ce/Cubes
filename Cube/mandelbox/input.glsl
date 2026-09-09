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
// Cubes background Image port. Geometry/shading from Cube/mandelbox.html.
// Fixed fractal origin; yaw/pitch in iMouse.xy, six-theme mask in iDate.x.
vec3 themeColor(int i) {
    if(i==0) return vec3(99.0,199.0,242.0)/255.0;
    if(i==1) return vec3(122.0,75.0,48.0)/255.0;
    if(i==2) return vec3(37.0,21.0,61.0)/255.0;
    if(i==3) return vec3(244.0,232.0,166.0)/255.0;
    if(i==4) return vec3(78.0,175.0,104.0)/255.0;
    if(i==5) return vec3(215.0,101.0,103.0)/255.0;
    return vec3(0.0);
}
const vec3 CAMERA = vec3(0.08, -0.12, 0.05);
const float FAR = 44.0;

float max3(vec3 v) { return max(v.x, max(v.y, v.z)); }
float min3(vec3 v) { return min(v.x, min(v.y, v.z)); }
float box(vec3 p, vec3 b) {
    vec3 q = abs(p) - b;
    return length(max(q, 0.0)) + min(max3(q), 0.0);
}
float crossSection(vec3 q) {
    return min(max(q.x,q.y), min(max(q.y,q.z), max(q.z,q.x)));
}

// A periodic solid carved with square tunnels at four scales.
// This is Menger-like geometry, not the exact Mandelbox set.
vec2 cathedral(vec3 p) {
    p /= 7.2;
    float d = -100.0;
    float scale = 1.0;
    float detail = 0.0;
    for (int i=0; i<4; ++i) {
        vec3 a = mod(p*scale, 2.0)-1.0;
        vec3 r = abs(1.0-3.0*abs(a));
        float cut = (crossSection(r)-1.0)/(3.0*scale);
        if (cut > d) { d=cut; detail=float(i); }
        scale *= 3.0;
    }
    return vec2(d*7.2, detail);
}

// Union of three differently scaled, infinite cubic beam networks.
vec2 lattice(vec3 p) {
    float d=100.0;
    float detail=0.0;
    float period=9.6;
    for (int i=0; i<3; ++i) {
        // Grid lines fall on cell boundaries, leaving the origin clear.
        vec3 a = abs(mod(p + period*0.5, period)-period*0.5);
        vec3 edge = period*0.5-a;
        float thickness = period*(i==0 ? 0.052 : 0.036);
        float beam = crossSection(edge)-thickness;
        if (beam < d) { d=beam; detail=float(i); }
        period /= 2.4;
    }
    // Open an axis-aligned observation chamber. It is a design choice, not
    // a collision system: the actual camera remains the constant above.
    float chamber=-box(p,vec3(1.65));
    if (chamber>d) { d=chamber; detail=3.0; }
    return vec2(d,detail);
}

vec2 foldedCore(vec3 p) {
    // Repeat a true, finite Mandelbox distance estimate to surround the view.
    vec3 q=mod(p+vec3(0.0,0.0,3.525)+9.4,18.8)-9.4;
    vec3 c=q/2.35;
    vec3 z=c;
    float derivative=1.0;
    float orbit=10.0;
    for (int i=0; i<9; ++i) {
        z=clamp(z,-1.0,1.0)*2.0-z; // box fold
        float r2=dot(z,z);
        float k=clamp(1.0/max(r2,0.00001),1.0,4.0); // sphere fold
        z*=k;
        derivative*=k;
        z=3.0*z+c;
        derivative=3.0*derivative+1.0;
        orbit=min(orbit,abs(length(z)-1.2));
    }
    float d=length(z)/abs(derivative)*2.35-0.0035;
    // The fixed origin lies in a natural void; this small safety cavity avoids
    // a near-plane collision if the estimator is adjusted later.
    d=max(d,-box(p,vec3(0.42)));
    return vec2(d,clamp(orbit*3.0,0.0,3.0));
}

vec2 map(vec3 p) {
    if (2==0) return cathedral(p);
    if (2==1) return lattice(p);
    return foldedCore(p);
}

vec3 normalAt(vec3 p,float eps) {
    vec2 e=vec2(1.0,-1.0)*eps;
    return normalize(e.xyy*map(p+e.xyy).x + e.yyx*map(p+e.yyx).x
                   + e.yxy*map(p+e.yxy).x + e.xxx*map(p+e.xxx).x);
}
float occlusion(vec3 p,vec3 n) {
    float a=0.0;
    float weight=1.0;
    for (int i=1;i<=3;++i) {
        float h=0.10+float(i)*0.18;
        a+=(h-map(p+n*h).x)*weight;
        weight*=0.52;
    }
    return clamp(1.0-1.75*a,0.14,1.0);
}

void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    if(iDate.y<0.5) { fragColor=vec4(0.0); return; }
    vec2 uv=fragCoord/iResolution.xy*2.0-1.0;
    vec3 rd=normalize(vec3(uv.x*iResolution.x/iResolution.y*iMouse.z, -uv.y*iMouse.z, -1.0));
    float pitch=iMouse.y;
    float yaw=iMouse.x;
    rd=vec3(rd.x,rd.y*cos(pitch)-rd.z*sin(pitch),rd.y*sin(pitch)+rd.z*cos(pitch));
    rd=vec3(rd.x*cos(yaw)-rd.z*sin(yaw),rd.y,rd.x*sin(yaw)+rd.z*cos(yaw));
    float travel=0.0;
    float hit=0.0;
    float epsilon=0.002;
    vec2 sampleValue=vec2(1.0);
    for (int i=0;i<112;++i) {
        vec3 p=CAMERA+rd*travel;
        sampleValue=map(p);
        epsilon=max(0.0015,travel*0.00050);
        if (sampleValue.x<epsilon) { hit=1.0; break; }
        travel+=max(sampleValue.x*0.78,0.001);
        if (travel>FAR) break;
    }
    // iDate.x is the closed six-theme bit mask. Keep territorial colours
    // distinct in dual/triple worlds instead of averaging them into grey.
    float mask=iDate.x;
    vec3 accent=vec3(216.0,60.0,255.0)/255.0;
    float count=0.0;
    for(int i=0;i<6;i++) if(mod(floor(mask/exp2(float(i))),2.0)>0.5) count+=1.0;
    float selected=floor(fract(atan(rd.z,rd.x)/6.2831853+0.5+0.10*rd.y)*max(count,1.0));
    float rank=0.0;
    for(int i=0;i<6;i++) {
        if(mod(floor(mask/exp2(float(i))),2.0)>0.5) {
            if(rank==selected) accent=themeColor(i);
            rank+=1.0;
        }
    }
    accent=pow(accent,vec3(2.2));
    vec3 fog=accent*0.025;
    vec3 base=accent*0.43;
    vec3 color=fog;
    if (hit>0.5) {
        vec3 p=CAMERA+rd*travel;
        vec3 n=normalAt(p,max(0.0012,epsilon*1.1));
        float ao=occlusion(p,n);
        float facing=max(dot(n,-rd),0.0);
        float key=max(dot(n,normalize(vec3(-0.45,0.70,-0.55))),0.0);
        float fill=max(dot(n,normalize(vec3(0.8,-0.2,0.4))),0.0);
        float detail=sampleValue.y;
        float tint=0.85+0.15*cos(detail*1.8);
        vec3 albedo=base*tint;
        albedo=mix(albedo,accent*0.58,clamp(detail*0.105,0.0,0.35));
        color=albedo*(0.060+0.29*facing+0.60*key+0.10*fill)*ao;
        // A fixed point light close to the observation chamber: no shadow ray.
        vec3 lightDirection=normalize(vec3(0.5,1.8,-0.2)-p);
        float point=max(dot(n,lightDirection),0.0)/(1.0+0.025*travel*travel);
        color+=accent*point*0.30;
        float fresnel=pow(1.0-facing,3.0);
        color+=accent*fresnel*0.055*ao;
        // Subtle repeated seams, evaluated only at the surface.
        vec3 nearest=abs(fract(p*0.42+0.5)-0.5);
        vec3 absN=abs(n);
        float seam;
        if (absN.x>absN.y && absN.x>absN.z) seam=min(nearest.y,nearest.z);
        else if (absN.y>absN.z) seam=min(nearest.x,nearest.z);
        else seam=min(nearest.x,nearest.y);
        float ink=1.0-smoothstep(0.008,0.024,seam);
        color*=1.0-ink*0.16;
        // Local luminous inlays. No bloom / multipass postprocessing.
        float band=1.0-smoothstep(0.016,0.033,abs(fract(p.y*0.23+0.11)-0.5));
        color+=accent*band*0.14*ao;
        float fogAmount=1.0-exp(-travel*(2==1 ? 0.046 : 0.043));
        color=mix(color,fog,fogAmount);
    }
    // Tone map and encode once, during baking, not during mouse movement.
    color=vec3(1.0)-exp(-color*1.45);
    color=pow(max(color,0.0),vec3(1.0/2.2));
    fragColor=vec4(color,1.0);
}

