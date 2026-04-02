use std::path::{Path, PathBuf};

pub fn lock_key_for_base(base_path: &Path) -> std::io::Result<PathBuf> {
    base_path.canonicalize()
}
