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
