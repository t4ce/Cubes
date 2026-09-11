#[cfg(test)]
mod plateau_tests {
    use super::*;
    use plateau::{Profile, Save, PlacedCube};
    use axum::http::StatusCode;
    use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
    static ID: AtomicU64 = AtomicU64::new(0);
    fn path() -> String {
        std::env::temp_dir().join(format!("cubeusers-test-{}-{}", std::process::id(), ID.fetch_add(1, Ordering::Relaxed)))
            .join("cubeusers.db").to_str().unwrap().into()
    }
    fn placed() -> PlacedCube { PlacedCube { center: [10., -16.8, 0.], scale: 0.799, flags: 0x8000 | 31 } }
    fn save(p: &Profile, placed: Vec<PlacedCube>) -> Save { Save { generation: p.generation, revision: p.revision, placed } }
    #[test]
    fn terrace_is_one_small_chunk_with_editor_geometry_and_center_walk_spawn() {
        for theme in 1..=6 {
            let bytes = plateau::generate(theme).unwrap();
            let records: Vec<_> = cube_format::cubes(&bytes).collect();
            assert_eq!(records.len(), 428);
            assert!(records.iter().all(|r| r.side == 8 && r.tier == 8 && r.part == 0));
            let authored: Vec<[i32;3]> = serde_json::from_str(EDITOR_TERRACE).unwrap();
            assert_eq!(records.iter().map(|r|r.origin).collect::<Vec<_>>(), authored);
            assert!(records.iter().all(|r| r.origin.iter().all(|v| *v >= -256 && *v + 8 <= 256)));
            let mut asset = orchard::decode("plateau", &bytes).unwrap();
            for c in &mut asset.cubes { c.center = orchard::world_from_demo(c.center); }
            let mut cam = walker_camera::CubesWalkerCam::plateau(&bytes, &[]).unwrap();
            assert!(!cam.is_flying());
            let first = cam.pose().0;
            assert!(first[0].abs() < 0.001 && first[2].abs() < 0.001);
            assert!(first[1] > -17.6 && first[1] < -16.);
            let cube = placed();
            assert!(cam.add_placed(&[(cube.center, cube.scale)]));
            assert!(walker_camera::CubesWalkerCam::plateau(&bytes, &[(cube.center, cube.scale)]).is_some());
            assert!(!cam.add_placed(&[([51.2, 0., 0.], 0.799)]));
            cam.update(walker_camera::Input { forward: 1., ..Default::default() }, 0.02);
            assert_ne!(cam.pose().0, first);
            assert_eq!(cam.crossed_portal(first), None);
        }
    }
    #[test]
    fn terrace_tab_path_can_preview_and_travel_to_an_aimed_cube() {
        let bytes = plateau::generate(3).unwrap();
        let mut cam = walker_camera::CubesWalkerCam::plateau(&bytes, &[]).unwrap();
        cam.look(-180., 150.);
        for frame in 0..120 {
            cam.update(walker_camera::Input { path_tab: frame == 0, ..Default::default() }, 0.016);
            if cam.path_status() == "ready-Space-to-travel" { break; }
        }
        assert_eq!(cam.path_status(), "ready-Space-to-travel");
        assert!(cam.path_target().is_some());
        let cubes = cam.path_cubes(0x7e02, 128);
        assert!(!cubes.is_empty());
        assert!(cubes.iter().all(|c| c.scale >= 0.001));
        let first = cam.pose().0;
        cam.update(walker_camera::Input { space: true, ..Default::default() }, 0.016);
        assert_eq!(cam.path_status(), "travelling");
        for _ in 0..300 {
            cam.update(walker_camera::Input::default(), 0.016);
            assert!(!cam.is_flying());
            if cam.path_status() != "travelling" { break; }
        }
        assert_ne!(cam.pose().0, first);
        assert_ne!(cam.path_status(), "travelling");
    }
    #[tokio::test]
    async fn two_users_create_save_reopen_delete_and_recreate_in_one_database() {
        let path = path();
        let store = profiles::Store::new(&path);
        assert_eq!(store.load("t4ce").await.unwrap_err(), StatusCode::NOT_FOUND);
        let a = store.create("t4ce", 5).await.unwrap();
        let b = store.create("other-user", 2).await.unwrap();
        assert_eq!(store.create("t4ce", 1).await.unwrap(), a); // Theme chosen once.
        let updated = store.save("t4ce", save(&a, vec![placed()])).await.unwrap();
        assert_eq!(store.save("t4ce", save(&a, vec![placed()])).await.unwrap(), updated); // Lost reply.
        assert_eq!(store.save("other-user", save(&a, vec![placed()])).await.unwrap_err(), StatusCode::CONFLICT);
        drop(store);
        let store = profiles::Store::new(&path);
        assert_eq!(store.load("t4ce").await.unwrap(), updated);
        assert_eq!(store.load("other-user").await.unwrap(), b);
        store.delete("t4ce").await.unwrap();
        drop(store);
        let store = profiles::Store::new(&path);
        assert_eq!(store.load("t4ce").await.unwrap_err(), StatusCode::NOT_FOUND);
        assert_eq!(store.load("other-user").await.unwrap(), b);
        let recreated = store.create("t4ce", 6).await.unwrap();
        assert!(recreated.generation > a.generation);
        assert!(recreated.placed.is_empty());
        assert_eq!(store.save("t4ce", save(&updated, updated.placed.clone())).await.unwrap_err(), StatusCode::CONFLICT);
        assert_eq!(std::fs::read_dir(std::path::Path::new(&path).parent().unwrap()).unwrap().count(), 1);
        drop(store); std::fs::remove_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn failed_persistence_keeps_last_durable_profile_and_corruption_is_not_reset() {
        let path = path();
        let store = profiles::Store::new(&path);
        let p = store.create("t4ce", 1).await.unwrap();
        async_fs::FAIL_WRITES.store(true, Ordering::Relaxed);
        assert!(store.save("t4ce", save(&p, vec![placed()])).await.is_err());
        async_fs::FAIL_WRITES.store(false, Ordering::Relaxed);
        assert_eq!(store.load("t4ce").await.unwrap(), p);
        drop(store);
        let store = profiles::Store::new(&path);
        assert_eq!(store.load("t4ce").await.unwrap(), p);
        drop(store);
        std::fs::write(&path, b"broken existing database").unwrap();
        assert!(profiles::Store::new(&path).create("t4ce", 2).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken existing database");
        std::fs::remove_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn malformed_names_nonfinite_bounds_and_stale_revision_cannot_change_profile() {
        let path = path(); let store = profiles::Store::new(&path);
        for name in ["", "../t4ce", "a/b", "a\\b", ".", "t4ce?", "x%2Fy"] { assert!(store.create(name, 1).await.is_err()); }
        let a = store.create("t4ce", 1).await.unwrap();
        let mut bad = placed(); bad.center[0] = f32::NAN;
        assert_eq!(store.save("t4ce", save(&a, vec![bad])).await.unwrap_err(), StatusCode::BAD_REQUEST);
        let mut bad = placed(); bad.center[0] = 512.;
        assert!(store.save("t4ce", save(&a, vec![bad])).await.is_err());
        let saved = store.save("t4ce", save(&a, vec![placed()])).await.unwrap();
        assert_eq!(store.save("t4ce", save(&a, vec![])).await.unwrap_err(), StatusCode::CONFLICT);
        assert_eq!(store.save("t4ce", save(&saved, vec![])).await.unwrap_err(), StatusCode::BAD_REQUEST);
        assert_eq!(store.load("t4ce").await.unwrap(), saved);
        drop(store); std::fs::remove_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn real_client_and_http_routes_round_trip_the_persisted_profile() {
        use plateau_client::{Command, exchange_at};
        let path = path();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/plateau/t4ce", listener.local_addr().unwrap());
        let router = profiles::router(Arc::new(profiles::Store::new(&path)));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        assert!(exchange_at("t4ce", Command::Load, &url).await.unwrap().is_none());
        let p = exchange_at("t4ce", Command::Create(3), &url).await.unwrap().unwrap();
        let saved = exchange_at("t4ce", Command::Save(save(&p, vec![placed()])), &url).await.unwrap().unwrap();
        assert_eq!(exchange_at("t4ce", Command::Load, &url).await.unwrap().unwrap(), saved);
        assert!(exchange_at("t4ce", Command::Delete, &url).await.unwrap().is_none());
        assert!(exchange_at("t4ce", Command::Load, &url).await.unwrap().is_none());
        server.abort(); let _ = server.await;
        std::fs::remove_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    }
}
