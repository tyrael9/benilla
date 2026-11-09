//! Where the WoW install is: the one resolver for the `Data` folder. The first candidate that
//! exists wins:
//!
//! 1. `$WOW_DATA`. Set and empty (`WOW_DATA=`) means no install and ends the search, so the
//!    no-install path runs on a machine that has one.
//! 2. The writable iOS app-container copy, seeded once from the bundle.
//! 3. `<project folder>/WoW/Data`, `dev` builds only ([`project_folder`]).
//! 4. `<exe dir>/Data`, then `<exe dir>/WoW/Data`, the release layouts.
//!
//! `dev` is a default feature here, so every dependent declares `default-features = false` and
//! re-exports it; otherwise feature unification puts the source-tree path in a player build.
//! `scripts/gates.sh`'s player build and `tests::a_player_build_carries_no_source_tree_path`
//! catch it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The launcher's project folder, handed in by its entry point ([`set_project_folder`]).
static LAUNCHER_FOLDER: OnceLock<PathBuf> = OnceLock::new();

/// Record the launcher's project folder, as its build stamp names it (`BuildId::project_dir`); an
/// empty stamp, a player build's, records nothing. The entry point calls this once, before
/// anything resolves from the folder, and a later call changes nothing.
pub fn set_project_folder(dir: &str) {
    if !dir.is_empty() {
        let _ = LAUNCHER_FOLDER.set(PathBuf::from(dir));
    }
}

/// The folder a dev build keeps its `WoW` link, `benilla-config/` and `.probe-identity` in: the
/// launcher's, when its entry point recorded one, else benilla's own checkout, so a crate built on
/// top of benilla resolves from its folder and not from cargo's checkout of benilla. `None` in a
/// player build, which looks in no source tree.
pub fn project_folder() -> Option<&'static Path> {
    #[cfg(feature = "dev")]
    {
        LAUNCHER_FOLDER
            .get()
            .map(PathBuf::as_path)
            .or_else(benilla_checkout)
    }
    #[cfg(not(feature = "dev"))]
    {
        None
    }
}

/// benilla's own checkout: this crate's manifest dir, two levels up. The addon corpus always
/// resolves here, since its tests are benilla's.
#[cfg(feature = "dev")]
fn benilla_checkout() -> Option<&'static Path> {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2)
}

#[cfg(target_os = "ios")]
pub fn prepare_ios_data() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let Some(bundle) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let bundled = bundle.join("Data");
    if !bundled.is_dir() {
        return;
    }
    let persistent = ios_data_dir(&home);
    if let Err(error) = seed_ios_data(&bundled, &persistent) {
        eprintln!(
            "benilla: could not prepare persistent WoW Data at {}: {error}",
            persistent.display()
        );
    }
}

#[cfg(any(target_os = "ios", test))]
fn ios_data_dir(home: &Path) -> PathBuf {
    ios_support_root(home).join("Data")
}

#[cfg(any(target_os = "ios", test))]
fn ios_addons_dir(home: &Path) -> PathBuf {
    ios_config_dir(home).join("AddOns")
}

#[cfg(any(target_os = "ios", test))]
fn ios_addons_dir_with_override(home: &Path, config_dir: Option<PathBuf>) -> PathBuf {
    config_dir
        .unwrap_or_else(|| ios_config_dir(home))
        .join("AddOns")
}

#[cfg(any(target_os = "ios", test))]
fn ios_support_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Benilla")
}

#[cfg(any(target_os = "ios", test))]
fn ios_config_dir(home: &Path) -> PathBuf {
    ios_support_root(home).join("benilla-config")
}

// Seed once. The ready marker is written only after the copied MPQ chain opens.
// An existing unmarked directory is adopted if valid, otherwise preserved untouched.
#[cfg(target_os = "ios")]
fn seed_ios_data(bundled: &Path, persistent: &Path) -> std::io::Result<()> {
    let marker = persistent.join(".benilla-ready");
    if marker.is_file() {
        return Ok(());
    }
    if persistent.is_dir() {
        if crate::Chain::open(persistent).is_ok() {
            return std::fs::write(marker, b"ready\n");
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "an existing unmarked Data directory is not a readable patch chain; leaving it untouched",
        ));
    }
    let parent = persistent.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "Data path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join("Data.installing");
    remove_path_if_exists(&staging)?;
    if let Err(error) = copy_dir_recursive(bundled, &staging) {
        let _ = remove_path_if_exists(&staging);
        return Err(error);
    }
    if let Err(error) = crate::Chain::open(&staging) {
        let _ = remove_path_if_exists(&staging);
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("the bundled Data directory is not a readable patch chain: {error:#}"),
        ));
    }
    if let Err(error) = std::fs::rename(&staging, persistent) {
        let _ = remove_path_if_exists(&staging);
        return Err(error);
    }
    // If this write fails, the next launch validates the promoted directory and writes it.
    std::fs::write(marker, b"ready\n")
}

#[cfg(target_os = "ios")]
pub fn prepare_ios_addons() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let Some(bundle) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let bundled = bundle.join("Interface/AddOns");
    if !bundled.is_dir() {
        return;
    }
    let persistent = ios_addons_dir_with_override(
        &home,
        std::env::var_os("BENILLA_HOME").map(PathBuf::from),
    );
    if let Err(error) = refresh_ios_addons(&bundled, &persistent) {
        eprintln!(
            "benilla: could not refresh AddOns at {}: {error}",
            persistent.display()
        );
    }
}

// Only AddOns is replaced. SavedVariables, settings, bindings, macros and other state stay put.
#[cfg(any(target_os = "ios", test))]
fn refresh_ios_addons(bundled: &Path, persistent: &Path) -> std::io::Result<()> {
    let parent = persistent.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "AddOns path has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join("AddOns.installing");
    let backup = parent.join("AddOns.previous");
    remove_path_if_exists(&staging)?;
    copy_dir_recursive(bundled, &staging)?;
    // Recover an interrupted earlier swap before starting another one.
    if backup.exists() && !persistent.exists() {
        std::fs::rename(&backup, persistent)?;
    } else if backup.exists() && persistent.exists() {
        remove_path_if_exists(&backup)?;
    }
    let had_previous = persistent.exists();
    if had_previous {
        std::fs::rename(persistent, &backup)?;
    }
    if let Err(error) = std::fs::rename(&staging, persistent) {
        if had_previous {
            let _ = std::fs::rename(&backup, persistent);
        }
        let _ = remove_path_if_exists(&staging);
        return Err(error);
    }
    if had_previous {
        remove_path_if_exists(&backup)?;
    }
    Ok(())
}

#[cfg(any(target_os = "ios", test))]
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(any(target_os = "ios", test))]
fn remove_path_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
/// The vanilla `Data` directory, or `None` when there is no install. Uncached, so a `$WOW_DATA`
/// change is seen.
pub fn wow_data() -> Option<PathBuf> {
    candidates().into_iter().find(|c| c.is_dir())
}

/// Every place [`wow_data`] looks, in order, whether or not it exists, for a "looked in" message.
pub fn candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "ios")]
    let ios_data = std::env::var_os("HOME").map(|home| ios_data_dir(&PathBuf::from(home)));
    #[cfg(not(target_os = "ios"))]
    let ios_data = None;

    candidates_from(
        std::env::var_os("WOW_DATA").map(PathBuf::from),
        ios_data,
        project_folder(),
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf)),
    )
}

/// [`candidates`] with the environment passed in: a test that set `$WOW_DATA` would change the
/// answer for every test running beside it. `tests/wow_data_env.rs` covers the real read.
fn candidates_from(
    override_dir: Option<PathBuf>,
    ios_data: Option<PathBuf>,
    project: Option<&Path>,
    exe_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(5);

    // 1. The override; set and empty is no install at all, even in a dev build
    // (`WOW_DATA= cargo play` runs as a player without data).
    if let Some(over) = override_dir {
        if over.as_os_str().is_empty() {
            return Vec::new();
        }
        out.push(over);
    }

    // iOS's writable app-container copy is preferred over the read-only app bundle.
    if let Some(data) = ios_data {
        out.push(data);
    }

    // 3. The project folder, dev builds only.
    if let Some(root) = project {
        out.push(root.join("WoW/Data"));
    }

    // 4. Beside the binary; a failed `current_exe` adds nothing.
    if let Some(dir) = exe_dir {
        out.push(dir.join("Data"));
        out.push(dir.join("WoW/Data"));
    }

    out
}

/// The install, or skip this test: `wow_data_or_skip!()` in a `-> ()` test,
/// `wow_data_or_skip!(None)` in a helper returning `Option`. The skip names every path tried, and
/// under `BENILLA_REQUIRE_DATA=1` it fails instead.
#[macro_export]
macro_rules! wow_data_or_skip {
    () => {
        $crate::wow_data_or_skip!(())
    };
    ($ret:expr) => {
        match $crate::wow_data() {
            Some(data) => data,
            None => {
                $crate::skipped("no WoW install found", &$crate::candidates());
                return $ret;
            }
        }
    };
}

/// A data-gated test's skip, printed; a panic under `BENILLA_REQUIRE_DATA=1`, which the gate sets
/// where the data is, since libtest hides a passing test's stderr and a lost install reads green.
#[doc(hidden)]
pub fn skipped(what: &str, looked_in: &[PathBuf]) {
    skipped_under(
        std::env::var_os("BENILLA_REQUIRE_DATA").is_some_and(|v| !v.is_empty() && v != "0"),
        what,
        looked_in,
    );
    // `$BENILLA_SKIP_LOG`: libtest hides the line above, so the gate counts skips from this file.
    if let Some(log) = std::env::var_os("BENILLA_SKIP_LOG").filter(|p| !p.is_empty()) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
        {
            // One write per line: parallel tests append here, and a formatted write can interleave.
            let _ = f.write_all(format!("{what}\n").as_bytes());
        }
    }
}

/// [`skipped`] with the switch passed in, testable without the process environment.
fn skipped_under(required: bool, what: &str, looked_in: &[PathBuf]) {
    let msg = format!("skipping: {what} — looked in {looked_in:?}");
    assert!(
        !required,
        "{msg} — and BENILLA_REQUIRE_DATA is set: the gate resolved this data on this machine, \
         so a skip here is a broken resolver or a missing link, not a missing install"
    );
    eprintln!("{msg}");
}

/// The third-party addon corpus the UI engine's real-addon tests run against, or `None`:
/// `$BENILLA_ADDON_CORPUS`, then `<benilla's checkout>/wow-addons-vanilla` (`dev` only), a
/// gitignored link beside `WoW`. Never a folder beside the checkout, which depends on where the
/// checkout sits.
pub fn addon_corpus() -> Option<PathBuf> {
    addon_corpus_candidates().into_iter().find(|c| c.is_dir())
}

/// Every place [`addon_corpus`] looks, in order, whether or not it exists, for the skip message.
pub fn addon_corpus_candidates() -> Vec<PathBuf> {
    addon_corpus_candidates_from(std::env::var_os("BENILLA_ADDON_CORPUS").map(PathBuf::from))
}

/// [`addon_corpus_candidates`] with the environment passed in, like [`candidates_from`].
fn addon_corpus_candidates_from(override_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(2);
    if let Some(over) = override_dir {
        out.push(over);
    }
    // benilla's checkout, dev builds only, whoever the launcher is: the corpus tests are benilla's.
    #[cfg(feature = "dev")]
    if let Some(root) = benilla_checkout() {
        out.push(root.join("wow-addons-vanilla"));
    }
    out
}

/// The addon corpus, or skip this test, spelled and refused as [`wow_data_or_skip`] is.
#[macro_export]
macro_rules! addon_corpus_or_skip {
    () => {
        $crate::addon_corpus_or_skip!(())
    };
    ($ret:expr) => {
        match $crate::addon_corpus() {
            Some(root) => root,
            None => {
                $crate::skipped(
                    "no vanilla addon corpus (set $BENILLA_ADDON_CORPUS)",
                    &$crate::addon_corpus_candidates(),
                );
                return $ret;
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No test here touches the process environment (see [`candidates_from`]), or records a
    /// launcher's folder, which is process-wide (`tests/project_folder.rs` has a process of its own).
    fn probe(over: Option<&str>, exe: Option<&str>) -> Vec<PathBuf> {
        candidates_from(
            over.map(PathBuf::from),
            None,
            project_folder(),
            exe.map(PathBuf::from),
        )
    }

    #[test]
    fn the_ladder_is_override_then_project_folder_then_beside_the_binary() {
        let c = probe(Some("/opt/wow/Data"), Some("/games/benilla"));
        assert_eq!(c.first(), Some(&PathBuf::from("/opt/wow/Data")), "{c:?}");
        assert!(c.contains(&PathBuf::from("/games/benilla/Data")), "{c:?}");
        assert!(
            c.contains(&PathBuf::from("/games/benilla/WoW/Data")),
            "{c:?}"
        );

        // Either input may be absent without shifting the rest.
        assert_eq!(
            probe(None, Some("/games/benilla")).last(),
            Some(&PathBuf::from("/games/benilla/WoW/Data"))
        );
        assert!(!probe(Some("/opt/wow/Data"), None)
            .iter()
            .any(|p| p.starts_with("/games")));
    }

    #[test]
    fn a_candidate_that_does_not_exist_is_never_chosen() {
        let tmp = std::env::temp_dir().join(format!("benilla-wd-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("Data")).unwrap();
        let ghost = tmp.join("nope");

        // The winner depends on the build (dev puts the project folder first), so assert only
        // that it is not the ghost and exists.
        let chosen = candidates_from(
            Some(ghost.clone()),
            None,
            project_folder(),
            Some(tmp.clone()),
        )
            .into_iter()
            .find(|c| c.is_dir());
        assert_ne!(
            chosen,
            Some(ghost),
            "a $WOW_DATA that does not exist must never be chosen"
        );
        assert!(
            chosen.is_some_and(|c| c.is_dir()),
            "a stale override must fall through to a real install"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn ios_data_and_addons_live_in_the_persistent_application_support_container() {
        let home = Path::new("/var/mobile/Containers/Data/Application/example");
        assert_eq!(
            ios_data_dir(home),
            home.join("Library/Application Support/Benilla/Data")
        );
        assert_eq!(
            ios_addons_dir(home),
            home.join("Library/Application Support/Benilla/benilla-config/AddOns")
        );
        assert_eq!(
            ios_addons_dir_with_override(home, Some(PathBuf::from("/custom/config"))),
            PathBuf::from("/custom/config/AddOns")
        );
    }

    #[test]
    fn an_ios_data_candidate_follows_an_explicit_override_and_precedes_the_bundle() {
        let persistent = PathBuf::from("/container/Library/Application Support/Benilla/Data");
        let candidates = candidates_from(
            Some(PathBuf::from("/explicit/Data")),
            Some(persistent.clone()),
            None,
            Some(PathBuf::from("/app/Benilla.app")),
        );
        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/explicit/Data"),
                persistent,
                PathBuf::from("/app/Benilla.app/Data"),
                PathBuf::from("/app/Benilla.app/WoW/Data"),
            ]
        );
    }

    #[test]
    fn bootstrap_copy_recurses_without_touching_its_source() {
        let tmp = std::env::temp_dir().join(format!("benilla-ios-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let source = tmp.join("bundle/Data");
        let target = tmp.join("container/Data.installing");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/patch.MPQ"), b"patch-bytes").unwrap();
        copy_dir_recursive(&source, &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("nested/patch.MPQ")).unwrap(),
            b"patch-bytes"
        );
        assert!(source.join("nested/patch.MPQ").is_file());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn refreshing_ios_addons_replaces_only_the_addon_tree() {
        let tmp = std::env::temp_dir().join(format!("benilla-ios-addons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.join("Library/Application Support/Benilla/benilla-config");
        let persistent = home.join("AddOns");
        let saved = home.join("saved/Example.lua");
        let bundled = tmp.join("bundle/Interface/AddOns");
        std::fs::create_dir_all(persistent.join("OldAddon")).unwrap();
        std::fs::write(persistent.join("OldAddon/OldAddon.toc"), b"old").unwrap();
        std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
        std::fs::write(&saved, b"player data").unwrap();
        std::fs::create_dir_all(bundled.join("NewAddon")).unwrap();
        std::fs::write(bundled.join("NewAddon/NewAddon.toc"), b"new").unwrap();
        refresh_ios_addons(&bundled, &persistent).unwrap();
        assert!(!persistent.join("OldAddon").exists());
        assert!(persistent.join("NewAddon/NewAddon.toc").is_file());
        assert_eq!(std::fs::read(&saved).unwrap(), b"player data");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn an_empty_override_means_there_is_no_install() {
        assert_eq!(
            probe(Some(""), Some("/games/benilla")),
            Vec::<PathBuf>::new(),
            "`WOW_DATA=` must leave the ladder with no rungs at all"
        );
    }

    /// A failure here means feature unification put the project-folder rung in a player build.
    #[cfg(not(feature = "dev"))]
    #[test]
    fn a_player_build_carries_no_source_tree_path() {
        assert_eq!(
            project_folder(),
            None,
            "a player build has no project folder, whatever its launcher stamped"
        );
        let root = env!("CARGO_MANIFEST_DIR");
        for c in probe(None, Some("/games/benilla")) {
            assert!(
                !c.starts_with(root),
                "a player build looked inside the source tree at {}",
                c.display()
            );
        }
    }

    /// With no launcher folder recorded (a test, an example, a tool) the rung is benilla's own
    /// checkout; with one, it is that folder's, and benilla's checkout is not looked in.
    #[cfg(feature = "dev")]
    #[test]
    fn a_dev_build_looks_in_the_project_folder() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        assert_eq!(
            project_folder(),
            Some(root),
            "nothing recorded: the checkout"
        );
        let c = probe(None, Some("/games/benilla"));
        assert!(
            c.contains(&root.join("WoW/Data")),
            "the dev build lost its project-folder candidate ({}): {c:?}",
            root.display()
        );

        let launcher = Path::new("/home/player/hello-mod");
        let c = candidates_from(None, None, Some(launcher), Some(PathBuf::from("/games/benilla")));
        assert_eq!(
            c,
            vec![
                launcher.join("WoW/Data"),
                PathBuf::from("/games/benilla/Data"),
                PathBuf::from("/games/benilla/WoW/Data"),
            ],
            "a launcher's folder takes the rung, and nothing else looks in benilla's checkout"
        );
    }

    #[cfg(feature = "dev")]
    #[test]
    fn the_corpus_ladder_is_override_then_benillas_checkout_and_nothing_outside_it() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let c = addon_corpus_candidates_from(Some(PathBuf::from("/opt/corpus")));
        assert_eq!(
            c,
            vec![
                PathBuf::from("/opt/corpus"),
                root.join("wow-addons-vanilla")
            ],
            "{c:?}"
        );
        for c in addon_corpus_candidates_from(None) {
            assert!(
                c.starts_with(root),
                "a corpus candidate outside the checkout ({}) — that is the resolver whose answer \
                 moved when the pool did",
                c.display()
            );
        }
    }

    #[cfg(not(feature = "dev"))]
    #[test]
    fn a_player_build_has_no_corpus_rung_of_its_own() {
        assert_eq!(addon_corpus_candidates_from(None), Vec::<PathBuf>::new());
    }

    #[test]
    fn a_skip_is_a_skip_where_nothing_requires_the_data() {
        skipped_under(false, "no WoW install found", &[PathBuf::from("/nowhere")]);
    }

    #[test]
    #[should_panic(expected = "BENILLA_REQUIRE_DATA is set")]
    fn a_skip_is_refused_where_the_gate_says_the_data_is_present() {
        skipped_under(true, "no WoW install found", &[PathBuf::from("/nowhere")]);
    }
}
