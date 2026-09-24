pub mod archive;
pub mod de;
pub mod output;
pub mod validation;

use std::path::Path;

/// Clear the read-only flag so a file can be overwritten.
///
/// On Windows this toggles the read-only attribute only. On Unix,
/// `Permissions::set_readonly(false)` sets a broad, effectively world-writable
/// mode, so instead we grant just the owner write bit while preserving the
/// file's existing mode bits.
pub(crate) fn set_writable(perms: &mut std::fs::Permissions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(perms.mode() | 0o200);
    }

    #[cfg(not(unix))]
    {
        perms.set_readonly(false);
    }
}

/// Convert a relative [`Path`] to a forward-slash string for cross-platform
/// consistency when stored in `.ggg.state` and displayed to users.
pub fn path_key(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
