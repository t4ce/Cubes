// Only the filesystem boundary is replaced; profile transactions use real redb.
pub mod async_fs {
    use std::sync::atomic::{AtomicBool, Ordering};
    pub static FAIL_WRITES: AtomicBool = AtomicBool::new(false);
    pub const ERR_NOT_FOUND: i32 = -8;
    pub enum ContentTypeId { BLOB, UTF8_TEXT }
    pub struct Metadata { pub len: u64, file: bool }
    impl Metadata { pub fn is_file(&self) -> bool { self.file } }
    fn path(bytes: &[u8]) -> &std::path::Path {
        std::path::Path::new(std::str::from_utf8(bytes).unwrap())
    }
    fn error(e: std::io::Error) -> i32 { if e.kind() == std::io::ErrorKind::NotFound { -8 } else { -2 } }
    pub async fn metadata(bytes: &[u8]) -> Result<Metadata, i32> {
        std::fs::metadata(path(bytes)).map(|m| Metadata { len: m.len(), file: m.is_file() }).map_err(error)
    }
    pub async fn read_file(bytes: &[u8]) -> Result<Vec<u8>, i32> { std::fs::read(path(bytes)).map_err(error) }
    pub async fn create_dir_all(bytes: &[u8]) -> Result<(), i32> { std::fs::create_dir_all(path(bytes)).map_err(error) }
    pub async fn write_file_typed(bytes: &[u8], contents: &[u8], _: ContentTypeId) -> Result<(), i32> {
        if FAIL_WRITES.load(Ordering::Relaxed) { return Err(-2); }
        std::fs::write(path(bytes), contents).map_err(error)
    }
}
