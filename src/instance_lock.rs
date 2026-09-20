use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum InstanceLockError {
    Busy,
    Io(std::io::Error),
}

pub struct InstanceLock {
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    pub fn acquire() -> Result<Self, InstanceLockError> {
        let base = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("workbuddy-auto-signin");
        Self::acquire_at(base.join("run.lock"))
    }

    pub fn acquire_at(path: impl AsRef<Path>) -> Result<Self, InstanceLockError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(InstanceLockError::Io)?;
        }

        // Windows 的 File::try_lock 要求文件以可写方式打开；read+write 在三平台一致。
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(InstanceLockError::Io)?;

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file, path }),
            Err(TryLockError::WouldBlock) => Err(InstanceLockError::Busy),
            Err(TryLockError::Error(error)) => Err(InstanceLockError::Io(error)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
