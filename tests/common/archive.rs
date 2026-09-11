//! In-memory archive fixtures for integration tests.
//!
//! Archive dependencies are HTTP downloads (a `.zip`, `.tar.gz`, or `.tgz`)
//! fetched by the binary via `reqwest::blocking`. Tests serve these bytes
//! through [`super::wiremock::MockApi::mount_file`]; [`zip_bytes`] builds the
//! bytes for a simple zip with no nesting (entries are written at the archive
//! root, so no `strip_components` is needed).
//!
//! Fake Godot engine and export-template archives are built here too, so the
//! wiremock fixtures can serve them without duplicating zip logic.

use std::io::Write;

use ggg::godot::release::GodotRelease;

/// Build an in-memory `.zip` archive containing `entries` at the archive root.
///
/// Each entry is `(path, contents)`. The resulting bytes can be served by
/// wiremock as the download for an archive dependency.
pub fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let mut w = zip::ZipWriter::new(&mut buf);
    for (path, contents) in entries {
        w.start_file::<_, ()>(*path, Default::default()).unwrap();
        w.write_all(contents.as_bytes()).unwrap();
    }
    w.finish().unwrap();
    buf.into_inner()
}

/// Build a fake Godot engine zip for `release` for the current platform.
///
/// Returns `(asset_name, zip_bytes)`. `asset_name` is the zip filename the
/// GitHub builds API reports for this platform (e.g.
/// `Godot_v4.3-stable_win64.exe.zip` on Windows), and `zip_bytes` is a valid
/// zip containing a single executable-looking file so `GodotCache::install`
/// can extract it and `find_executable` can locate a binary. The
/// `browser_download_url` the mock reports for `asset_name` should be served
/// from the same mock server as the archive itself.
pub fn fake_godot_asset(release: &GodotRelease) -> (String, Vec<u8>) {
    let prefix = if release.mono {
        format!("Godot_v{}-{}_mono_", release.version, release.flavor)
    } else {
        format!("Godot_v{}-{}_", release.version, release.flavor)
    };

    if cfg!(target_os = "linux") {
        let stem = format!("{prefix}linux.x86_64");
        (format!("{stem}.zip"), zip_bytes(&[(stem.as_str(), "")]))
    } else if cfg!(target_os = "macos") {
        let stem = format!("{prefix}macos");
        (
            format!("{stem}.universal.zip"),
            zip_bytes(&[("Godot.app/Contents/MacOS/Godot", "")]),
        )
    } else {
        // Windows: standard zips are named `.exe.zip`, mono zips plain
        // `.zip`; the extracted executable always ends in `.exe` so
        // `find_executable` recognises it.
        let stem = format!("{prefix}win64");
        let asset_name = if release.mono {
            format!("{stem}.zip")
        } else {
            format!("{stem}.exe.zip")
        };
        let exe = format!("{stem}.exe");
        (asset_name, zip_bytes(&[(&exe, "")]))
    }
}

/// Build a fake Godot export-templates `.tpz` for `release`.
///
/// A `.tpz` is Godot's name for a zip holding export templates. This archive
/// contains only `templates/version.txt` — the file ggg's export-template
/// installer (`ExportTemplateCache` in `src/godot/export_templates.rs`) treats
/// as the completion marker for an install. The bytes are served as the
/// download the Godot downloads host returns for export templates.
pub fn fake_template_tpz(release: &GodotRelease) -> Vec<u8> {
    let version = format!("{}.{}", release.version, release.flavor);
    zip_bytes(&[("templates/version.txt", &version)])
}
