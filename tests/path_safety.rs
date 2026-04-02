use std::{fs, path::Path};

use supafast_jellyfin::safety::path_guard::{resolve_under_base, PathGuardError};
use tempfile::tempdir;

#[test]
fn rejects_parent_traversal_escape() {
    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    fs::create_dir_all(&base).expect("base dir");

    let result = resolve_under_base(&base, Path::new("../outside/secret.mkv"));

    assert!(matches!(result, Err(PathGuardError::InvalidCandidate(_)) | Err(PathGuardError::OutsideBase)));
}

#[test]
fn resolves_file_inside_base() {
    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    let episode = base.join("Show").join("Season 1").join("episode.mkv");

    fs::create_dir_all(episode.parent().expect("parent")).expect("episode dir");
    fs::write(&episode, b"video").expect("episode file");

    let resolved = resolve_under_base(&base, Path::new("Show/Season 1/episode.mkv")).expect("resolve");

    assert!(resolved.starts_with(base.canonicalize().expect("canonical base")));
}

#[test]
fn resolves_nonexistent_target_with_existing_parent() {
    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    let season_dir = base.join("Show").join("Season 1");

    fs::create_dir_all(&season_dir).expect("season dir");

    let resolved = resolve_under_base(&base, Path::new("Show/Season 1/new-episode.mkv")).expect("resolve");

    assert_eq!(resolved, season_dir.join("new-episode.mkv"));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    let outside = temp.path().join("outside");

    fs::create_dir_all(&base).expect("base dir");
    fs::create_dir_all(&outside).expect("outside dir");
    fs::write(outside.join("secret.txt"), b"secret").expect("secret file");

    symlink(&outside, base.join("linked-outside")).expect("symlink");

    let result = resolve_under_base(&base, Path::new("linked-outside/secret.txt"));

    assert!(matches!(result, Err(PathGuardError::OutsideBase)));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_leaf_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    let outside = temp.path().join("outside");

    fs::create_dir_all(&base).expect("base dir");
    fs::create_dir_all(&outside).expect("outside dir");
    fs::write(outside.join("secret.txt"), b"secret").expect("secret file");

    symlink(outside.join("secret.txt"), base.join("escaped-leaf.mkv")).expect("symlink");

    let result = resolve_under_base(&base, Path::new("escaped-leaf.mkv"));

    assert!(matches!(result, Err(PathGuardError::OutsideBase)));
}
