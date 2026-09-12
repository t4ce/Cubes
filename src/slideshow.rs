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
/// One image plane for the whole slab: +X is its front, image-right is -Z,
/// image-up is +Y. Normals affect lighting only, never pixel addressing.
/// This also projects the adjacent edge pixels onto bevels and slab sides.
fn surface_uv(p: [f32;3]) -> [f32;2] {
    [0.5-p[2]*0.5, 0.5-p[1]*0.5]
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
                        let uv = layout.uv(face, surface_uv(p));
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
        assert_eq!(contract::TIERS.map(|t|(t.source,t.blocks,t.pixels)), [(64,8,8),(128,10,21),(256,14,31),(512,28,31)]);
        assert_eq!(contract::TIERS.map(|t|t.grid()), [8,20,28,28]);
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
        assert_eq!(surface_uv([0.,1.,1.]),[0.,0.]);
        assert_eq!(surface_uv([0.,-1.,-1.]),[1.,1.]);
    }
    #[test]
    fn bevel_and_front_vertices_use_the_same_image_plane_on_all_six_faces() {
        let layout = Layout { tiers: [1,2,3,4,3,4] };
        let mesh = geometry(layout);
        for v in &mesh.vertices {
            let face = (0..6).min_by(|&a,&b| {
                let distance = |face: usize| {
                    let c = center(face);
                    (0..3).map(|k| (v[k]-c[k])*BASES[face][2][k]).sum::<f32>().abs()
                };
                distance(a).partial_cmp(&distance(b)).unwrap()
            }).unwrap();
            let c = center(face);
            let right: f32 = (0..3).map(|k|(v[k]-c[k])*BASES[face][0][k]).sum();
            let up: f32 = (0..3).map(|k|(v[k]-c[k])*BASES[face][1][k]).sum();
            let expected = layout.uv(face, [0.5+right/(2.*HALF_EXTENT), 0.5-up/(2.*HALF_EXTENT)]);
            assert!((v[6]-expected[0]).abs()<0.000001);
            assert!((v[7]-expected[1]).abs()<0.000001);
        }
    }
    #[test]
    fn one_pixel_per_block_tiers_keep_bevel_samples_inside_their_own_pixel() {
        for tier in [1,4] {
            let layout=Layout {tiers:[tier;6]};
            let n=layout.tier(0).blocks;
            assert_eq!(n,layout.tier(0).grid());
            for row in 0..n { for column in 0..n {
                // Front centers, inset face corners and points just inside the
                // outer bevel all sample the same cell. Depth cannot change UV.
                for local_y in [-0.999,-0.8,0.,0.8,0.999] {
                    for local_z in [-0.999,-0.8,0.,0.8,0.999] {
                        for depth in [-1.,0.,1.] {
                            let p=[depth/n as f32,(2.*row as f32+1.+local_y)/n as f32-1.,
                                (2.*column as f32+1.+local_z)/n as f32-1.];
                            let uv=surface_uv(p);
                            assert_eq!((uv[0]*n as f32) as u32,n-1-column);
                            assert_eq!((uv[1]*n as f32) as u32,n-1-row);
                        }
                    }
                }
            } }
        }
    }

}
