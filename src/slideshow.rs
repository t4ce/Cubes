//! CubeImage's 1×N×N beveled slab, projected across six inward world faces.
use alloc::vec::Vec;
pub use cubes_protocol::gallery as contract;
#[path = "image_cube.rs"]
#[allow(dead_code)]
mod reference;
use contract::Layout;
// WorldShowcase: 4 chunks × 512 c1 × 0.2 renderer units, centered on origin.
pub const WORLD_HALF: f32 = 204.8;
pub const HALF_EXTENT: f32 = 72.;
pub const DISTANCE: f32 = WORLD_HALF;
/// Right, up, inward. Right × up = inward; top/bottom have deterministic north.
pub const BASES: [[[f32; 3]; 3]; 6] = [
    [[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]],
    [[0.,0.,1.],[0.,1.,0.],[-1.,0.,0.]],
    [[-1.,0.,0.],[0.,1.,0.],[0.,0.,-1.]],
    [[0.,0.,-1.],[0.,1.,0.],[1.,0.,0.]],
    [[1.,0.,0.],[0.,0.,-1.],[0.,1.,0.]],
    [[1.,0.,0.],[0.,0.,1.],[0.,-1.,0.]],
];
pub fn rotate(face: usize, v: [f32;3]) -> [f32;3] {
    core::array::from_fn(|a| (0..3).map(|k| BASES[face][k][a]*v[k]).sum())
}
pub fn center(face: usize) -> [f32;3] { BASES[face][2].map(|v| -v*WORLD_HALF) }
/// Preserve the HTML's dominant-normal box projection, including bevel tie rules.
fn box_uv(p: [f32;3], n: [f32;3]) -> [f32;2] {
    let q = p.map(|v| v*0.5+0.5);
    let a = n.map(f32::abs);
    if a[0] > a[1] && a[0] > a[2] {
        [if n[0]>0. {1.-q[2]} else {q[2]}, 1.-q[1]]
    } else if a[1] > a[2] {
        [q[0], if n[1]>0. {q[2]} else {1.-q[2]}]
    } else {
        [if n[2]>0. {q[0]} else {1.-q[0]}, 1.-q[1]]
    }
}
fn hidden(n: [f32;3], row: u32, column: u32, count: u32) -> bool {
    (n == [0.,1.,0.] && row+1<count) || (n == [0.,-1.,0.] && row>0)
        || (n == [0.,0.,1.] && column+1<count) || (n == [0.,0.,-1.] && column>0)
}
pub struct Geometry { pub vertices: Vec<[f32;12]>, pub indices: Vec<u32> }
pub fn geometry(layout: Layout) -> Geometry {
    let blocks: usize = layout.tiers.iter().enumerate().map(|(i,_)| layout.tier(i).blocks.pow(2) as usize).sum();
    let mut mesh = Geometry { vertices: Vec::with_capacity(blocks*80+4096), indices: Vec::with_capacity(blocks*108+4096) };
    for face in 0..6 {
        let count = layout.tier(face).blocks;
        let scale = 1./count as f32;
        for row in 0..count { for column in 0..count {
            let mut mapping = [u32::MAX; reference::VERTICES.len()];
            for tri in reference::TRIANGLES {
                let v = reference::VERTICES[tri[0]];
                if hidden([v[3],v[4],v[5]], row, column, count) { continue; }
                for id in tri {
                    if mapping[id] == u32::MAX {
                        let v = reference::VERTICES[id];
                        let p = [v[0]*scale, (v[1]+2.*row as f32+1.)*scale-1., (v[2]+2.*column as f32+1.)*scale-1.];
                        let n = [v[3],v[4],v[5]];
                        let uv = layout.uv(face, box_uv(p,n));
                        let offset = rotate(face, [-p[2]*HALF_EXTENT,p[1]*HALF_EXTENT,p[0]*HALF_EXTENT]);
                        let position: [f32;3] = core::array::from_fn(|a| center(face)[a]+offset[a]);
                        let normal = rotate(face, [-n[2],n[1],n[0]]);
                        // Normal maps are absent; still supply an orthogonal tangent for PBR.
                        let axis = if normal[1].abs()<0.9 { [0.,1.,0.] } else { [1.,0.,0.] };
                        let t = [axis[1]*normal[2]-axis[2]*normal[1],axis[2]*normal[0]-axis[0]*normal[2],axis[0]*normal[1]-axis[1]*normal[0]];
                        let length = libm::sqrtf(t.iter().map(|v|v*v).sum());
                        mapping[id] = mesh.vertices.len() as u32;
                        mesh.vertices.push([position[0],position[1],position[2],normal[0],normal[1],normal[2],uv[0],uv[1],t[0]/length,t[1]/length,t[2]/length,1.]);
                    }
                    mesh.indices.push(mapping[id]);
                }
            }
        } }
    }
    mesh
}
impl Geometry {
    pub fn vertex_bytes(&self) -> &[u8] {
        const { assert!(cfg!(target_endian = "little")); }
        // Arrays of initialized f32 have no padding. Borrow until upload completes;
        // avoid allocating a second full geometry copy on the scene thread.
        unsafe { core::slice::from_raw_parts(self.vertices.as_ptr().cast(), self.vertices.len()*48) }
    }
    pub fn index_bytes(&self) -> &[u8] {
        const { assert!(cfg!(target_endian = "little")); }
        // u32 indices are initialized, contiguous and little endian on TRUEOS.
        unsafe { core::slice::from_raw_parts(self.indices.as_ptr().cast(), self.indices.len()*4) }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiers_are_exact_and_faces_are_centered_with_inward_right_handed_bases() {
        assert_eq!(contract::TIERS.map(|t|(t.source,t.blocks,t.pixels)), [(64,12,60),(128,18,90),(256,24,120),(512,32,320)]);
        for face in 0..6 {
            let [u,v,n] = BASES[face];
            assert_eq!([u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]], n);
            assert_eq!(center(face), n.map(|x|-x*WORLD_HALF));
        }
    }
    #[test]
    fn geometry_is_bounded_wound_outward_and_has_no_touching_flat_faces() {
        for tier in 1..=4 {
            let layout = Layout {tiers:[tier;6]};
            let mesh = geometry(layout);
            let n = layout.tier(0).blocks as usize;
            assert_eq!(mesh.vertex_bytes().len(), mesh.vertices.len()*48);
            assert_eq!(mesh.index_bytes().len(), mesh.indices.len()*4);
            assert_eq!(&mesh.vertex_bytes()[..4], &mesh.vertices[0][0].to_le_bytes());
            assert_eq!(&mesh.index_bytes()[..4], &mesh.indices[0].to_le_bytes());
            assert!(mesh.vertex_bytes().len()+mesh.index_bytes().len() < 27*1024*1024);
            assert_eq!(mesh.indices.len()/3, 6*(44*n*n-8*n*(n-1)));
            for v in &mesh.vertices {
                assert!(v.iter().all(|f|f.is_finite()));
                assert!((0. ..=1.).contains(&v[6]) && (0. ..=1.).contains(&v[7]));
            }
            for tri in mesh.indices.chunks_exact(3) {
                let a=mesh.vertices[tri[0] as usize];let b=mesh.vertices[tri[1] as usize];let c=mesh.vertices[tri[2] as usize];
                let u:[f32;3]=core::array::from_fn(|i|b[i]-a[i]);let v:[f32;3]=core::array::from_fn(|i|c[i]-a[i]);
                let cross=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
                assert!((0..3).map(|i|cross[i]*a[3+i]).sum::<f32>()>0.);
            }
        }
    }
    #[test]
    fn front_projection_is_upright_and_spans_the_whole_slab() {
        assert_eq!(box_uv([0.,1.,1.],[1.,0.,0.]),[0.,0.]);
        assert_eq!(box_uv([0.,-1.,-1.],[1.,0.,0.]),[1.,1.]);
    }
}
