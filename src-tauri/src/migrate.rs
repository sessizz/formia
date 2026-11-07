//! One-time carry-over of user data from the app's previous identifier.
//!
//! Tauri keys the config and cache folders by the bundle identifier, so renaming
//! the app (com.modelgezgini.app -> com.sessizz.formia) would otherwise start with
//! empty settings, no favourites and a cold thumbnail cache.

use std::fs;
use std::path::Path;

/// Identifier used by builds released as "Model Gezgini".
pub const LEGACY_IDENTIFIER: &str = "com.modelgezgini.app";

/// Copies the top-level files of the old config folder (settings.json with the
/// favourites, window state) and moves the old thumbnail cache, but only when the
/// new location has none yet. Failures are ignored: the worst case is a fresh start.
pub fn carry_over(config_dir: &Path, thumbs_dir: &Path) {
    if let Some(old) = sibling(config_dir) {
        copy_config(&old, config_dir);
    }
    if let (Some(cache_dir), Some(name)) = (thumbs_dir.parent(), thumbs_dir.file_name()) {
        if let Some(old_cache) = sibling(cache_dir) {
            move_thumbs(&old_cache.join(name), thumbs_dir);
        }
    }
}

fn sibling(dir: &Path) -> Option<std::path::PathBuf> {
    Some(dir.parent()?.join(LEGACY_IDENTIFIER)).filter(|p| p != dir)
}

fn copy_config(old: &Path, new: &Path) {
    if new.join("settings.json").exists() || !old.join("settings.json").is_file() {
        return;
    }
    let Ok(entries) = fs::read_dir(old) else { return };
    if fs::create_dir_all(new).is_err() {
        return;
    }
    for entry in entries.flatten() {
        let src = entry.path();
        if src.is_file() {
            let _ = fs::copy(&src, new.join(entry.file_name()));
        }
    }
}

fn move_thumbs(old: &Path, new: &Path) {
    if new.exists() || !old.is_dir() {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::rename(old, new);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Fresh scratch root laid out like the real config/cache parents.
    fn root(tag: &str) -> PathBuf {
        let r = std::env::temp_dir().join(format!("formia-migrate-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&r);
        fs::create_dir_all(&r).unwrap();
        r
    }

    #[test]
    fn copies_settings_and_window_state_and_moves_thumbs() {
        let r = root("full");
        let (cfg, cache) = (r.join("config"), r.join("cache"));
        let old_cfg = cfg.join(LEGACY_IDENTIFIER);
        fs::create_dir_all(&old_cfg).unwrap();
        fs::write(old_cfg.join("settings.json"), r#"{"favorites":["D:\\Modeller"]}"#).unwrap();
        fs::write(old_cfg.join(".window-state.json"), "{}").unwrap();
        let old_thumbs = cache.join(LEGACY_IDENTIFIER).join("thumbs");
        fs::create_dir_all(&old_thumbs).unwrap();
        fs::write(old_thumbs.join("abc.png"), b"png").unwrap();

        let new_cfg = cfg.join("com.sessizz.formia");
        let new_thumbs = cache.join("com.sessizz.formia").join("thumbs");
        carry_over(&new_cfg, &new_thumbs);

        assert_eq!(fs::read_to_string(new_cfg.join("settings.json")).unwrap(), r#"{"favorites":["D:\\Modeller"]}"#);
        assert!(new_cfg.join(".window-state.json").is_file());
        assert!(old_cfg.join("settings.json").is_file(), "old settings stay as a backup");
        assert_eq!(fs::read(new_thumbs.join("abc.png")).unwrap(), b"png");
        assert!(!old_thumbs.exists(), "thumbnail cache is moved, not duplicated");
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn never_overwrites_existing_new_data() {
        let r = root("keep");
        let (cfg, cache) = (r.join("config"), r.join("cache"));
        let old_cfg = cfg.join(LEGACY_IDENTIFIER);
        fs::create_dir_all(&old_cfg).unwrap();
        fs::write(old_cfg.join("settings.json"), "old").unwrap();
        let old_thumbs = cache.join(LEGACY_IDENTIFIER).join("thumbs");
        fs::create_dir_all(&old_thumbs).unwrap();

        let new_cfg = cfg.join("com.sessizz.formia");
        fs::create_dir_all(&new_cfg).unwrap();
        fs::write(new_cfg.join("settings.json"), "new").unwrap();
        let new_thumbs = cache.join("com.sessizz.formia").join("thumbs");
        fs::create_dir_all(&new_thumbs).unwrap();
        carry_over(&new_cfg, &new_thumbs);

        assert_eq!(fs::read_to_string(new_cfg.join("settings.json")).unwrap(), "new");
        assert!(old_thumbs.is_dir(), "old cache left alone when the new one exists");
        let _ = fs::remove_dir_all(&r);
    }

    #[test]
    fn no_legacy_data_is_a_no_op() {
        let r = root("none");
        let new_cfg = r.join("config").join("com.sessizz.formia");
        let new_thumbs = r.join("cache").join("com.sessizz.formia").join("thumbs");
        carry_over(&new_cfg, &new_thumbs);
        assert!(!new_cfg.exists());
        assert!(!new_thumbs.exists());
        let _ = fs::remove_dir_all(&r);
    }
}
