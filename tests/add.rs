//! Integration tests for `ggg add`.
//!
//! Covers the non-interactive add paths: archive, git, asset-library (via
//! --id/plain ID), and asset-store (spec with --yes/-y). Interactive
//! disambiguation (2-5 search results -> picker) is out of scope - driving
//! dialoguer from a test harness is fragile; see TASK-5.9.

mod common;

use predicates::str::contains;

use ggg::config::{AssetStoreRef, Source};

use common::TestProject;
use common::archive::zip_bytes;
use common::git_fixtures::BareRepo;
use common::wiremock::{
    AssetDetailBody, AssetSearchBody, MockApi, StoreAssetSummary, StoreRelease, StoreSearchBody,
};

// ---------------------------------------------------------------------------
// AC #1: add archive writes the dep with no network
// ---------------------------------------------------------------------------

#[test]
fn add_archive_writes_dep() {
    // `ggg add archive` must write a new dependency entry to ggg.toml
    // without contacting any remote.  The URL and name are stored exactly
    // as given; downloading and SHA verification happen later during sync.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/addon.zip",
            "--name",
            "my-addon",
        ])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    let Source::Archive { url, .. } = &config.dependency[0].source else {
        panic!("expected Archive source");
    };
    assert_eq!(url, "http://example.com/addon.zip");
}

// ---------------------------------------------------------------------------
// AC #2: add archive rejects a non-.zip/.tar.gz/.tgz URL
// ---------------------------------------------------------------------------

#[test]
fn add_archive_rejects_bad_extension() {
    // Only .zip, .tar.gz, and .tgz archives are supported.  Any other
    // extension must be rejected up front with a clear error rather than
    // silently storing a URL that sync cannot handle.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/file.rar",
            "--name",
            "my-rar",
        ])
        .assert()
        .failure()
        .stderr(contains("unrecognised archive format"));
}

// ---------------------------------------------------------------------------
// AC #3: bare add <url>.zip routes to the archive path
// ---------------------------------------------------------------------------

#[test]
fn bare_add_zip_routes_to_archive() {
    // The convenience form `ggg add <url>` auto-detects the dependency type
    // from the input.  A URL ending in .zip must be routed to the archive
    // handler, producing the same result as `ggg add archive <url>`.
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["add", "http://example.com/addon.zip", "--name", "my-addon"])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    let Source::Archive { url, .. } = &config.dependency[0].source else {
        panic!("expected Archive source");
    };
    assert_eq!(url, "http://example.com/addon.zip");
}

// ---------------------------------------------------------------------------
// AC #4: fails with 'no ggg.toml found'
// ---------------------------------------------------------------------------

#[test]
fn add_fails_without_ggg_toml() {
    // Without a ggg.toml there is no Godot version or dependency list to
    // work with, so the command must fail immediately with a hint to run
    // `ggg init` rather than guess or create a default config.
    let project = TestProject::new();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/addon.zip",
            "--name",
            "x",
        ])
        .assert()
        .failure()
        .stderr(contains("no ggg.toml found"));
}

// ---------------------------------------------------------------------------
// AC #5: duplicate name rejected
// ---------------------------------------------------------------------------

#[test]
fn add_rejects_duplicate_name() {
    // Every dependency name must be unique within ggg.toml.  Adding a
    // dependency whose name collides with an existing one must fail with
    // a clear error rather than silently overwriting or creating a
    // duplicate entry.
    let project = TestProject::new();
    project
        .config()
        .archive("existing", "http://example.com/first.zip")
        .write();

    project
        .cmd()
        .args([
            "add",
            "archive",
            "http://example.com/second.zip",
            "--name",
            "existing",
        ])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

// ---------------------------------------------------------------------------
// AC #6: add with no arguments fails
// ---------------------------------------------------------------------------

#[test]
fn add_fails_with_no_args() {
    // Running `ggg add` with no arguments is ambiguous — the tool cannot
    // know whether the user intends a git, archive, or asset dependency.
    // It must fail with an error explaining the expected usage.
    let project = TestProject::new();
    project.config().write();

    project.cmd().arg("add").assert().failure();
}

// ---------------------------------------------------------------------------
// AC #7: add git <file://url>@rev --name -y resolves and writes the dep
// ---------------------------------------------------------------------------

#[test]
fn add_git_resolves_and_writes() {
    // `ggg add git` must resolve the given rev (branch, tag, or SHA)
    // against the remote and write both the dependency to ggg.toml.
    // Using a local file:// bare repo keeps the test hermetic while
    // exercising the real git resolution path through gix.  The -y flag
    // skips interactive prompts, and --name overrides the inferred name.
    let repo = BareRepo::builder()
        .with("plugin.gd", "# hello from the fixture")
        .build();

    let project = TestProject::new();
    project.config().write();

    let url_with_rev = format!("{}@main", repo.file_url());
    project
        .cmd()
        .args(["add", "git", &url_with_rev, "--name", "my-addon", "-y"])
        .assert()
        .success()
        .stdout(contains("Added \"my-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "my-addon");
    let Source::Git { git, rev } = &config.dependency[0].source else {
        panic!("expected Git source");
    };
    assert_eq!(git, repo.file_url().as_str());
    assert_eq!(rev, "main");
}

// ---------------------------------------------------------------------------
// AC #8: add asset-library --id N --name -y fetches and writes the dep via wiremock
// ---------------------------------------------------------------------------

#[tokio::test]
async fn add_asset_library_via_id_writes_dep() {
    // `ggg add asset-library --id N` fetches the asset metadata from the Godot
    // Asset Library API and writes the dependency to ggg.toml.  The test
    // wires the API endpoint to a wiremock server so no real network is
    // needed.  The -y flag skips the confirmation prompt, and --name
    // overrides the inferred name.
    let api = MockApi::start().await;
    api.mount_asset_detail(
        42,
        AssetDetailBody::new_with_file(
            42,
            "Test Addon",
            "Author",
            "MIT",
            1,
            "1.0.0",
            "https://example.com/test-addon",
            zip_bytes(&[("addon/addon-file.txt", "addon content")]),
        ),
    )
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args([
            "add",
            "asset-library",
            "--id",
            "42",
            "--name",
            "test-addon",
            "-y",
        ])
        .assert()
        .success()
        .stdout(contains("Added \"test-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    assert_eq!(config.dependency[0].name, "test-addon");
    let Source::AssetLib {
        asset_library_id, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetLib source");
    };
    assert_eq!(*asset_library_id, 42);
}

// ---------------------------------------------------------------------------
// AC #9: a bare numeric add routes to the Asset Library by ID
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bare_numeric_add_routes_to_asset_library() {
    // `ggg add <number>` and `ggg add asset-library <number>` both resolve the
    // number as a direct Asset Library ID, keeping the convenience form that
    // `ggg add asset <n>` used to provide (asset is now the store keyword).
    let api = MockApi::start().await;
    api.mount_asset_detail(
        54,
        AssetDetailBody::new_with_file(
            54,
            "Test Addon",
            "Author",
            "MIT",
            1,
            "1.0.0",
            "https://example.com/test-addon",
            zip_bytes(&[("addon/addon-file.txt", "addon content")]),
        ),
    )
    .await;

    let mut project = TestProject::new();
    project.env_api(&api).config().write();

    project
        .cmd()
        .args(["add", "54", "--name", "test-addon", "-y"])
        .assert()
        .success()
        .stdout(contains("Added \"test-addon\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    let Source::AssetLib {
        asset_library_id, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetLib source");
    };
    assert_eq!(*asset_library_id, 54);
}

// ---------------------------------------------------------------------------
// AC #10: a bare asset-store reference routes to the store and pins the latest
// stable release compatible with the project's Godot version
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bare_store_spec_adds_latest_stable_compatible() {
    // `ggg add souleat/godot-xoshiro256-plus-plus` must route to the Asset
    // Store and pin the largest release id among the stable, compatible
    // releases: here id 11 ("1.1.0") wins over the newer-but-incompatible
    // ("2.1.0", needs Godot 4.4) and the unstable ("2.0.0") releases.
    let api = MockApi::start().await;
    api.mount_store_asset(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &StoreAssetSummary::new(
            "godot-xoshiro256-plus-plus",
            "souleat",
            "souleat",
            "GodotXoshiro256++",
            "MIT",
            "https://example.com/store/godot-xoshiro256-plus-plus",
        ),
    )
    .await;
    api.mount_store_releases(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &[
            StoreRelease::new(
                10,
                "1.0.0",
                true,
                "https://example.com/dl/1.0.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                11,
                "1.1.0",
                true,
                "https://example.com/dl/1.1.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                12,
                "2.0.0",
                false,
                "https://example.com/dl/2.0.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                13,
                "2.1.0",
                true,
                "https://example.com/dl/2.1.0.zip",
                "4.4",
                None,
            ),
        ],
    )
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args(["add", "souleat/godot-xoshiro256-plus-plus", "--yes"])
        .assert()
        .success()
        .stdout(contains("Added \"godotxoshiro256\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    let Source::AssetStore {
        asset_store_asset, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetStore source");
    };
    assert_eq!(
        *asset_store_asset,
        AssetStoreRef {
            publisher: "souleat".to_string(),
            asset: "godot-xoshiro256-plus-plus".to_string(),
            version: "1.1.0".to_string(),
        }
    );
}

// ---------------------------------------------------------------------------
// AC #11: a :version suffix pins exactly that release
// ---------------------------------------------------------------------------

#[tokio::test]
async fn store_spec_version_suffix_pins_exact_release() {
    let api = MockApi::start().await;
    api.mount_store_asset(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &StoreAssetSummary::new(
            "godot-xoshiro256-plus-plus",
            "souleat",
            "souleat",
            "GodotXoshiro256++",
            "MIT",
            "https://example.com/store/godot-xoshiro256-plus-plus",
        ),
    )
    .await;
    api.mount_store_releases(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &[
            StoreRelease::new(
                10,
                "1.0.0",
                true,
                "https://example.com/dl/1.0.0.zip",
                "4.0",
                None,
            ),
            StoreRelease::new(
                11,
                "1.1.0",
                true,
                "https://example.com/dl/1.1.0.zip",
                "4.0",
                None,
            ),
        ],
    )
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args([
            "add",
            "souleat/godot-xoshiro256-plus-plus:1.0.0",
            "--yes",
            "--name",
            "xoshiro",
        ])
        .assert()
        .success()
        .stdout(contains("Added \"xoshiro\""));

    let config = project.read_ggg_toml();
    let Source::AssetStore {
        asset_store_asset, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetStore source");
    };
    assert_eq!(asset_store_asset.version, "1.0.0");
}

// ---------------------------------------------------------------------------
// AC #13: the pinned release may be non-interactively pinning an incompatible
// one warns but still adds
// ---------------------------------------------------------------------------

#[tokio::test]
async fn store_pinned_incompatible_release_warns_but_adds() {
    // Pin "2.1.0" explicitly even though it needs Godot 4.4 and the project
    // uses 4.3: ggg must add it anyway with a warning on stderr.
    let api = MockApi::start().await;
    api.mount_store_asset(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &StoreAssetSummary::new(
            "godot-xoshiro256-plus-plus",
            "souleat",
            "souleat",
            "GodotXoshiro256++",
            "MIT",
            "https://example.com/store/godot-xoshiro256-plus-plus",
        ),
    )
    .await;
    api.mount_store_releases(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &[StoreRelease::new(
            13,
            "2.1.0",
            true,
            "https://example.com/dl/2.1.0.zip",
            "4.4",
            None,
        )],
    )
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args([
            "add",
            "souleat/godot-xoshiro256-plus-plus:2.1.0",
            "--yes",
            "--name",
            "xoshiro",
        ])
        .assert()
        .success()
        .stderr(contains("warning"))
        .stdout(contains("Added \"xoshiro\""));

    let config = project.read_ggg_toml();
    let Source::AssetStore {
        asset_store_asset, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetStore source");
    };
    assert_eq!(asset_store_asset.version, "2.1.0");
}

// ---------------------------------------------------------------------------
// AC #14: `asset` is an alias for `asset-store`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn asset_alias_routes_to_store() {
    let api = MockApi::start().await;
    api.mount_store_asset(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &StoreAssetSummary::new(
            "godot-xoshiro256-plus-plus",
            "souleat",
            "souleat",
            "GodotXoshiro256++",
            "MIT",
            "https://example.com/store/godot-xoshiro256-plus-plus",
        ),
    )
    .await;
    api.mount_store_releases(
        "souleat",
        "godot-xoshiro256-plus-plus",
        &[StoreRelease::new(
            10,
            "1.0.0",
            true,
            "https://example.com/dl/1.0.0.zip",
            "4.0",
            None,
        )],
    )
    .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).config().write();

    project
        .cmd()
        .args([
            "add",
            "asset",
            "souleat/godot-xoshiro256-plus-plus",
            "--yes",
            "--name",
            "xoshiro",
        ])
        .assert()
        .success()
        .stdout(contains("Added \"xoshiro\""));

    let config = project.read_ggg_toml();
    let Source::AssetStore {
        asset_store_asset, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetStore source");
    };
    assert_eq!(asset_store_asset.version, "1.0.0");
}

// ---------------------------------------------------------------------------
// AC #15: asset-store rejects numeric IDs and the --id flag with guidance
// ---------------------------------------------------------------------------

#[test]
fn asset_keyword_numeric_is_rejected() {
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["add", "asset", "42", "--yes"])
        .assert()
        .failure()
        .stderr(contains("asset-library --id 42"));
}

#[test]
fn asset_store_rejects_id_flag() {
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["add", "asset-store", "--id", "42", "--yes"])
        .assert()
        .failure()
        .stderr(contains("publisher/slug[:version]"));
}

// ---------------------------------------------------------------------------
// AC #16: a combined search with a single total hit auto-adds (no picker)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn combined_search_single_store_hit_auto_adds() {
    // Bare plain-query add: the store returns exactly one hit and the library
    // none, so disambiguation auto-selects the store hit without a picker.
    let api = MockApi::start().await;
    api.mount_store_search(&StoreSearchBody::new(
        1,
        StoreAssetSummary::new(
            "decal-co",
            "Bitwes",
            "bitwes",
            "Decal Co",
            "MIT",
            "https://example.com/store/decal-co",
        ),
    ))
    .await;
    api.mount_store_asset(
        "bitwes",
        "decal-co",
        &StoreAssetSummary::new(
            "decal-co",
            "Bitwes",
            "bitwes",
            "Decal Co",
            "MIT",
            "https://example.com/store/decal-co",
        ),
    )
    .await;
    api.mount_store_releases(
        "bitwes",
        "decal-co",
        &[StoreRelease::new(
            3,
            "1.0.0",
            true,
            "https://example.com/dl/1.0.0.zip",
            "4.0",
            None,
        )],
    )
    .await;
    api.mount_asset_search(&AssetSearchBody::new(0, vec![]))
        .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).env_api(&api).config().write();

    project
        .cmd()
        .args(["add", "decal-co", "--yes", "--name", "decal"])
        .assert()
        .success()
        .stdout(contains("Added \"decal\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency.len(), 1);
    let Source::AssetStore {
        asset_store_asset, ..
    } = &config.dependency[0].source
    else {
        panic!("expected AssetStore source");
    };
    assert_eq!(asset_store_asset.asset, "decal-co");
    assert_eq!(asset_store_asset.version, "1.0.0");
}

// ---------------------------------------------------------------------------
// AC #17: a combined search with 6+ total hits errors suggesting `ggg search`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn combined_search_with_many_results_errors() {
    let api = MockApi::start().await;
    api.mount_store_search(&StoreSearchBody::new(
        6,
        StoreAssetSummary::new(
            "gut",
            "Bitwes",
            "bitwes",
            "GUT",
            "MIT",
            "https://example.com/store/gut",
        ),
    ))
    .await;
    api.mount_asset_search(&AssetSearchBody::new(0, vec![]))
        .await;

    let mut project = TestProject::new();
    project.env_store_api(&api).env_api(&api).config().write();

    project
        .cmd()
        .args(["add", "gut", "--yes"])
        .assert()
        .failure()
        .stderr(contains("found 6 results"))
        .stderr(contains("ggg search gut"));
}

// ---------------------------------------------------------------------------
// AC #18: bare archive add infers the name from the URL filename with --yes
// ---------------------------------------------------------------------------

#[test]
fn bare_archive_infers_name_from_url() {
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args(["add", "http://example.com/debug_draw_3d.zip", "--yes"])
        .assert()
        .success()
        .stdout(contains("Added \"debug-draw-3d\""));

    let config = project.read_ggg_toml();
    assert_eq!(config.dependency[0].name, "debug-draw-3d");
}

// ---------------------------------------------------------------------------
// AC #19: archive-specific flags are rejected on non-archive routes
// ---------------------------------------------------------------------------

#[test]
fn bare_git_rejects_sha256_flag() {
    let project = TestProject::new();
    project.config().write();

    project
        .cmd()
        .args([
            "add",
            "https://example.com/repo.git@main",
            "--sha256",
            "x",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(contains("--sha256 is only valid with `ggg add archive`"));
}
