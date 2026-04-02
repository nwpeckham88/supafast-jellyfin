use std::fs;

use supafast_jellyfin::safety::lock_key::lock_key_for_base;
use tempfile::tempdir;

#[test]
fn canonicalizes_lock_key_for_equivalent_paths() {
    let temp = tempdir().expect("tempdir");
    let base = temp.path().join("library");
    fs::create_dir_all(&base).expect("base dir");

    let key_a = lock_key_for_base(&base).expect("key a");
    let key_b = lock_key_for_base(&base.join(".")).expect("key b");

    assert_eq!(key_a, key_b);
}
