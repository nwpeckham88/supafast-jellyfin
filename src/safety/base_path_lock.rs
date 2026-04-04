use std::collections::HashSet;

use tokio::sync::Mutex;

#[derive(Default)]
pub struct BasePathLockManager {
    active_paths: Mutex<HashSet<String>>,
}

impl BasePathLockManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn try_acquire(&self, canonical_base_path: &str) -> bool {
        let mut active = self.active_paths.lock().await;
        active.insert(canonical_base_path.to_string())
    }

    pub async fn release(&self, canonical_base_path: &str) {
        let mut active = self.active_paths.lock().await;
        active.remove(canonical_base_path);
    }
}
