use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

/// Rename a file or directory, falling back to copy+delete on EXDEV (cross-filesystem).
/// For directories, this recursively copies then removes the source.
pub fn rename_with_exdev_fallback(source: &Path, target: &Path) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(err) if is_exdev(&err) => {
            tracing::info!(
                source = %source.display(),
                target = %target.display(),
                "EXDEV detected, falling back to copy+delete"
            );
            if source.is_dir() {
                copy_dir_recursive(source, target)?;
            } else {
                copy_file_with_fsync(source, target)?;
            }
            remove_source(source)?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn is_exdev(err: &io::Error) -> bool {
    // EXDEV = errno 18 on Linux, reported as CrossesDevices on nightly
    // or raw_os_error 18 on stable
    if let Some(os_err) = err.raw_os_error() {
        return os_err == 18; // libc::EXDEV
    }
    // Fallback: check ErrorKind if available on this Rust version
    matches!(err.kind(), ErrorKind::Other)
        && err.to_string().to_lowercase().contains("cross-device")
}

fn copy_file_with_fsync(source: &Path, target: &Path) -> io::Result<()> {
    fs::copy(source, target)?;
    // fsync the target file
    let file = fs::File::open(target)?;
    file.sync_all()?;
    // fsync the parent directory
    if let Some(parent) = target.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = target.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            copy_file_with_fsync(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn remove_source(source: &Path) -> io::Result<()> {
    if source.is_dir() {
        fs::remove_dir_all(source)
    } else {
        fs::remove_file(source)
    }
}
