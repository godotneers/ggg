pub mod archive;

use std::path::Path;

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
