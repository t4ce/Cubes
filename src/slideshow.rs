//! Six nearby CubeImage slabs around one central 3×3×3 c4 landmark.
use alloc::vec::Vec;
pub use cubes_protocol::gallery as contract;
#[path = "image_cube.rs"]
#[allow(dead_code)]
mod reference;
use contract::Layout;
// WorldShowcase: 4 chunks × 512 c1 × 0.2 renderer units, centered on origin.
pub const WORLD_HALF: f32 = contract::WORLD_HALF_C1 as f32 * contract::C1;
pub const DISTANCE: f32 = contract::GALLERY_DISTANCE_TWICE_C1 as f32 * contract::C1 * 0.5;
pub const CENTER_CUBE_SIDE: f32 = contract::CENTER_CUBE_SIDE_C1 as f32 * contract::C1;
pub const CENTER_HALF_EXTENT: f32 =
    CENTER_CUBE_SIDE * contract::CENTER_CUBES_PER_AXIS as f32 * 0.5;
pub const RADIUS: f32 = DISTANCE + contract::TIERS[2].blocks as f32*contract::C1;
pub const BASES: [[[f32; 3]; 3]; 6] = {
    let mut bases = [[[0.;3];3];6];
    let mut face = 0;
    while face < 6 {
        let mut k = 0;
        while k < 3 {
            let mut a = 0;
            while a < 3 { bases[face][k][a] = contract::BASES[face][k][a] as f32; a += 1; }
            k += 1;
        }
        face += 1;
    }
    bases
};
#[cfg(test)]
fn half_extent(layout: Layout, face: usize) -> f32 {
    layout.tier(face).blocks as f32 * contract::C1 * 0.5
}
pub fn rotate(face: usize, v: [f32;3]) -> [f32;3] {
    core::array::from_fn(|a| (0..3).map(|k| BASES[face][k][a]*v[k]).sum())
}
pub fn center(face: usize) -> [f32;3] { BASES[face][2].map(|v| -v*DISTANCE) }
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
    geometry_with_holy(layout, &[])
}
pub fn geometry_with_holy(layout: Layout, holy: &[cubes_protocol::holy::Pixel]) -> Geometry {
    let blocks: usize = layout.tiers.iter().enumerate().map(|(i,_)| layout.tier(i).blocks.pow(2) as usize).sum();
    let center_blocks = contract::CENTER_CUBES_PER_AXIS.pow(3) as usize;
    let mut mesh = Geometry { vertices: Vec::with_capacity((blocks+center_blocks+holy.len())*80+4096), indices: Vec::with_capacity((blocks+center_blocks+holy.len())*108+4096) };
    for face in 0..6 {
        let count = layout.tier(face).blocks;
        let scale = 1./count as f32;
        for row in 0..count { for column in 0..count {
            let cube_center = layout.cube_min(face, row, column).map(|v| (v as f32+0.5)*contract::C1);
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
                        let offset = rotate(face, [-v[2],v[1],v[0]]).map(|v| v*contract::C1*0.5);
                        let position: [f32;3] = core::array::from_fn(|a| cube_center[a]+offset[a]);
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
    let side = contract::CENTER_CUBES_PER_AXIS;
    let half = CENTER_CUBE_SIDE*0.5;
    let white = layout.white_uv();
    for x in 0..side { for y in 0..side { for z in 0..side {
        let cube_center = [x-side/2,y-side/2,z-side/2].map(|v| v as f32*CENTER_CUBE_SIDE);
        let mut mapping = [u32::MAX; reference::VERTICES.len()];
        for tri in reference::TRIANGLES {
            let v = reference::VERTICES[tri[0]];
            let n = [v[3],v[4],v[5]];
            let interior = (n == [1.,0.,0.] && x+1<side) || (n == [-1.,0.,0.] && x>0)
                || (n == [0.,1.,0.] && y+1<side) || (n == [0.,-1.,0.] && y>0)
                || (n == [0.,0.,1.] && z+1<side) || (n == [0.,0.,-1.] && z>0);
            if interior { continue; }
            for id in tri {
                if mapping[id] == u32::MAX {
                    let v = reference::VERTICES[id];
                    let position: [f32;3] = core::array::from_fn(|a| cube_center[a]+v[a]*half);
                    let normal = [v[3],v[4],v[5]];
                    let axis = if normal[1].abs()<0.9 { [0.,1.,0.] } else { [1.,0.,0.] };
                    let tangent = [axis[1]*normal[2]-axis[2]*normal[1],
                        axis[2]*normal[0]-axis[0]*normal[2], axis[0]*normal[1]-axis[1]*normal[0]];
                    let length = libm::sqrtf(tangent.iter().map(|v|v*v).sum());
                    mapping[id] = mesh.vertices.len() as u32;
                    mesh.vertices.push([position[0],position[1],position[2],normal[0],normal[1],normal[2],
                        white[0],white[1],tangent[0]/length,tangent[1]/length,tangent[2]/length,1.]);
                }
                mesh.indices.push(mapping[id]);
            }
        }
    } } }
    // The sparse 48px asset is an upright c1 cube grid. Its bottom edge rests
    // on the landmark and it faces the player's +Z-side spawn.
    let occupied: alloc::collections::BTreeSet<_> = holy.iter().map(|pixel| (pixel.x, pixel.y)).collect();
    for pixel in holy {
        let cube_center = [
            (pixel.x as f32 + 0.5 - cubes_protocol::holy::WIDTH as f32 * 0.5) * contract::C1,
            CENTER_HALF_EXTENT + (cubes_protocol::holy::HEIGHT as f32 - pixel.y as f32 - 0.5) * contract::C1,
            0.,
        ];
        let half = contract::C1 * 0.5;
        let uv = layout.holy_uv(pixel.palette);
        let mut mapping = [u32::MAX; reference::VERTICES.len()];
        for tri in reference::TRIANGLES {
            let v = reference::VERTICES[tri[0]];
            let n = [v[3],v[4],v[5]];
            let touching = (n == [1.,0.,0.] && occupied.contains(&(pixel.x+1,pixel.y)))
                || (n == [-1.,0.,0.] && pixel.x>0 && occupied.contains(&(pixel.x-1,pixel.y)))
                || (n == [0.,1.,0.] && pixel.y>0 && occupied.contains(&(pixel.x,pixel.y-1)))
                || (n == [0.,-1.,0.] && pixel.y+1<cubes_protocol::holy::HEIGHT
                    && occupied.contains(&(pixel.x,pixel.y+1)));
            if touching { continue; }
            for id in tri {
                if mapping[id] == u32::MAX {
                    let v = reference::VERTICES[id];
                    let position: [f32;3] = core::array::from_fn(|axis| cube_center[axis]+v[axis]*half);
                    let normal = [v[3],v[4],v[5]];
                    let axis = if normal[1].abs()<0.9 { [0.,1.,0.] } else { [1.,0.,0.] };
                    let tangent = [axis[1]*normal[2]-axis[2]*normal[1],
                        axis[2]*normal[0]-axis[0]*normal[2], axis[0]*normal[1]-axis[1]*normal[0]];
                    let length = libm::sqrtf(tangent.iter().map(|value|value*value).sum());
                    mapping[id] = mesh.vertices.len() as u32;
                    mesh.vertices.push([position[0],position[1],position[2],normal[0],normal[1],normal[2],
                        uv[0],uv[1],tangent[0]/length,tangent[1]/length,tangent[2]/length,1.]);
                }
                mesh.indices.push(mapping[id]);
            }
        }
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
        assert_eq!(contract::TIERS.map(|t|(t.source,t.blocks,t.pixels)), [(48,6,6),(128,8,16),(256,16,128)]);
        assert_eq!(contract::TIERS.map(|t|t.grid()), [6,16,128]);
        for face in 0..6 {
            let [u,v,n] = BASES[face];
            assert_eq!([u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]], n);
            assert_eq!(center(face), n.map(|x|-x*DISTANCE));
        }
    }
    #[test]
    fn wire_cube_grids_are_unique_unit_cells_centered_at_ten_percent_radius() {
        use alloc::collections::BTreeSet;
        // Parse the actual server package, then exercise every preset on every face.
        let bytes = include_bytes!("../../TRUEOS-Blueprints/apps/cubesrv/slides/gallery.cga");
        let (published, _) = Layout::parse(bytes).unwrap();
        for layout in [published, Layout {tiers:[1;6]}, Layout {tiers:[2;6]}, Layout {tiers:[3;6]}] {
            for face in 0..6 {
                let count = layout.tier(face).blocks;
                let mut cells = BTreeSet::new();
                let mut min = [i32::MAX;3];
                let mut max = [i32::MIN;3];
                for row in 0..count { for column in 0..count {
                    let cell = layout.cube_min(face,row,column);
                    assert!(cells.insert(cell));
                    for a in 0..3 {
                        min[a] = min[a].min(cell[a]); max[a] = max[a].max(cell[a]+1);
                        assert!(cell[a]>=-104 && cell[a]+1<=104);
                    }
                } }
                assert_eq!(cells.len(), count.pow(2) as usize);
                for a in 0..3 {
                    if contract::BASES[face][2][a] == 0 {
                        assert_eq!((min[a],max[a]), (-(count as i32)/2,count as i32/2));
                    } else {
                        assert_eq!(max[a]-min[a],1);
                        assert_eq!(min[a]+max[a],
                            -contract::BASES[face][2][a]*contract::GALLERY_DISTANCE_TWICE_C1);
                        assert!(min[a].abs()==102 || min[a].abs()==103);
                    }
                }
                let mesh = geometry(layout);
                let mut actual_min = [f32::INFINITY;3];
                let mut actual_max = [f32::NEG_INFINITY;3];
                for v in &mesh.vertices {
                    let normal = BASES[face][2];
                    let depth: f32 = (0..3).map(|a|(v[a]-center(face)[a])*normal[a]).sum();
                    if depth.abs()>contract::C1 { continue; }
                    for a in 0..3 {
                        actual_min[a]=actual_min[a].min(v[a]); actual_max[a]=actual_max[a].max(v[a]);
                    }
                }
                for a in 0..3 {
                    assert!((actual_min[a]-min[a] as f32*contract::C1).abs()<0.00003);
                    assert!((actual_max[a]-max[a] as f32*contract::C1).abs()<0.00003);
                }
            }
        }
    }
    #[test]
    fn geometry_is_bounded_wound_outward_and_has_no_touching_flat_faces() {
        for tier in 1..=3 {
            let layout = Layout {tiers:[tier;6]};
            let mesh = geometry(layout);
            let n = layout.tier(0).blocks as usize;
            assert_eq!(mesh.vertex_bytes().len(), mesh.vertices.len()*48);
            assert_eq!(mesh.index_bytes().len(), mesh.indices.len()*4);
            assert_eq!(&mesh.vertex_bytes()[..4], &mesh.vertices[0][0].to_le_bytes());
            assert_eq!(&mesh.index_bytes()[..4], &mesh.indices[0].to_le_bytes());
            assert!(mesh.vertex_bytes().len()+mesh.index_bytes().len() < 27*1024*1024);
            let center = contract::CENTER_CUBES_PER_AXIS as usize;
            let center_triangles = 44*center.pow(3)-12*center.pow(2)*(center-1);
            assert_eq!(mesh.indices.len()/3, 6*(44*n*n-8*n*(n-1))+center_triangles);
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
    fn center_landmark_is_three_c4_cubes_per_axis_and_uses_the_white_texel() {
        let layout = Layout {tiers:[1;6]};
        let mesh = geometry(layout);
        let white = layout.white_uv();
        let central: Vec<_> = mesh.vertices.iter().filter(|v| v[..3].iter().all(|x| x.abs()<=CENTER_HALF_EXTENT+0.0001)).collect();
        assert!(!central.is_empty());
        assert!(central.iter().all(|v| [v[6],v[7]]==white));
        for axis in 0..3 {
            assert!(central.iter().any(|v| (v[axis]+CENTER_HALF_EXTENT).abs()<0.0001));
            assert!(central.iter().any(|v| (v[axis]-CENTER_HALF_EXTENT).abs()<0.0001));
        }
        assert_eq!(CENTER_CUBE_SIDE, 8.*contract::C1);
    }
    #[test]
    fn holy_frame_is_an_upright_sparse_c1_grid_resting_on_the_landmark() {
        let layout = Layout {tiers:[1;6]};
        let base = geometry(layout);
        let pixels = [
            cubes_protocol::holy::Pixel {x:0,y:47,palette:0},
            cubes_protocol::holy::Pixel {x:47,y:0,palette:4},
        ];
        let animated = geometry_with_holy(layout, &pixels);
        let added = &animated.vertices[base.vertices.len()..];
        assert!(!added.is_empty());
        for palette in [0,4] {
            assert!(added.iter().any(|vertex| [vertex[6],vertex[7]] == layout.holy_uv(palette)));
        }
        for axis in 0..2 {
            let values = added.iter().map(|vertex| vertex[axis]).collect::<Vec<_>>();
            let (expected_min, expected_max) = if axis == 0 {
                (-4.8,4.8)
            } else {
                (CENTER_HALF_EXTENT,CENTER_HALF_EXTENT+9.6)
            };
            assert!((values.iter().copied().fold(f32::INFINITY,f32::min)-expected_min).abs()<0.0001);
            assert!((values.iter().copied().fold(f32::NEG_INFINITY,f32::max)-expected_max).abs()<0.0001);
        }
        assert!(added.iter().all(|vertex| vertex[2].abs()<=contract::C1*0.5+0.0001));
    }
    #[test]
    fn bevel_and_front_vertices_use_the_same_image_plane_on_all_six_faces() {
        let layout = Layout { tiers: [1,2,3,1,2,3] };
        let mesh = geometry(layout);
        for v in &mesh.vertices {
            if v[..3].iter().all(|x| x.abs()<=CENTER_HALF_EXTENT+0.0001) { continue; }
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
            let expected = layout.uv(face, [0.5+right/(2.*half_extent(layout, face)), 0.5-up/(2.*half_extent(layout, face))]);
            assert!((v[6]-expected[0]).abs()<0.000001);
            assert!((v[7]-expected[1]).abs()<0.000001);
        }
    }
    #[test]
    fn one_pixel_per_block_tiers_keep_bevel_samples_inside_their_own_pixel() {
        for tier in [1] {
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
