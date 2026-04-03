use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathGuardError {
    #[error("base path is invalid: {0}")]
    InvalidBase(String),
    #[error("candidate path is invalid: {0}")]
    InvalidCandidate(String),
    #[error("target resolves outside base path")]
    OutsideBase,
}

pub fn resolve_under_base(
    base_path: &Path,
    candidate_relative: &Path,
) -> Result<PathBuf, PathGuardError> {
    if candidate_relative.is_absolute() {
        return Err(PathGuardError::OutsideBase);
    }

    if candidate_relative
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(PathGuardError::OutsideBase);
    }

    let canonical_base = base_path
        .canonicalize()
        .map_err(|err| PathGuardError::InvalidBase(err.to_string()))?;

    let joined = canonical_base.join(candidate_relative);
    let (parent, file_name) = split_parent_and_leaf(&joined)?;

    let canonical_parent = parent
        .canonicalize()
        .map_err(|err| PathGuardError::InvalidCandidate(err.to_string()))?;

    if !canonical_parent.starts_with(&canonical_base) {
        return Err(PathGuardError::OutsideBase);
    }

    let target = canonical_parent.join(file_name);

    if target.exists() {
        let canonical_target = target
            .canonicalize()
            .map_err(|err| PathGuardError::InvalidCandidate(err.to_string()))?;
        if !canonical_target.starts_with(&canonical_base) {
            return Err(PathGuardError::OutsideBase);
        }
        return Ok(canonical_target);
    }

    Ok(target)
}

fn split_parent_and_leaf(path: &Path) -> Result<(&Path, &std::ffi::OsStr), PathGuardError> {
    let parent = path
        .parent()
        .ok_or_else(|| PathGuardError::InvalidCandidate("missing parent".to_string()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| PathGuardError::InvalidCandidate("missing path leaf".to_string()))?;
    Ok((parent, file_name))
}
