use std::path::Path;

pub trait Filesystem: Send + 'static {
    fn remove_file(&self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
}

pub struct RealFs;

impl Filesystem for RealFs {
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
}

/// No-op filesystem for unit tests — no disk access.
pub struct NullFs;

impl Filesystem for NullFs {
    fn remove_file(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn remove_dir_all(&self, _path: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn exists(&self, _path: &Path) -> bool {
        true
    }
    fn is_dir(&self, _path: &Path) -> bool {
        true
    }
}
