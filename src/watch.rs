//! # Live Stylesheet File Watcher
//!
//! Provides [`StyleWatcher`] and [`load_style_fallback`] for watching `.gss` files on disk
//! and automatically hot-reloading [`Gss`] stylesheets at runtime when modified.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{Gss, load_gss_from_file};

/// Loads a [`Gss`] stylesheet from `style_file`, returning `fallback` if loading or parsing fails.
pub fn load_style_fallback<P: AsRef<Path>>(style_file: Option<P>, fallback: Gss) -> Gss {
    if let Some(style_file) = style_file.as_ref() {
        match load_gss_from_file(style_file) {
            Ok(ok) => return ok,
            Err(err) => {
                eprintln!(
                    "[gss] warning: cannot load file '{}' because of: {}",
                    style_file.as_ref().display(),
                    err
                );
            }
        }
    }
    fallback
}

/// Watches a `.gss` file on disk and reloads it whenever the file is modified or recreated.
pub struct StyleWatcher {
    path: PathBuf,
    rx: Receiver<()>,
    _watcher: Option<RecommendedWatcher>,
}

impl StyleWatcher {
    /// Creates a new [`StyleWatcher`] monitoring `path` for changes.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (tx, rx) = mpsc::channel();
        let mut watcher_handle = None;

        let abs_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if let Some(parent) = abs_path.parent() {
            let abs_path_clone = abs_path.clone();
            if let Ok(mut w) =
                notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res
                        && event.paths.iter().any(|p| p == &abs_path_clone)
                        && (event.kind.is_modify() || event.kind.is_create())
                    {
                        let _ = tx.send(());
                    }
                })
                && w.watch(parent, RecursiveMode::NonRecursive).is_ok()
            {
                watcher_handle = Some(w);
            }
        }

        Self {
            path,
            rx,
            _watcher: watcher_handle,
        }
    }

    /// Returns the watched file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the watched stylesheet immediately, returning `fallback` if the file is missing or invalid.
    pub fn load_or(&self, fallback: Gss) -> Gss {
        load_style_fallback(Some(&self.path), fallback)
    }

    /// Polls for filesystem modification events and reloads `style` in-place if the file changed.
    ///
    /// Returns `true` if a reload occurred.
    pub fn poll_reload(&self, style: &mut Gss) -> bool {
        let mut changed = false;
        while self.rx.try_recv().is_ok() {
            changed = true;
        }
        if changed {
            let prev = std::mem::take(style);
            *style = load_style_fallback(Some(&self.path), prev);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_str;

    #[test]
    fn test_load_style_fallback_behavior() {
        let fallback = parse_str("key = 42,").unwrap();
        let loaded = load_style_fallback(Some("non_existent_gss_12345.gss"), fallback);
        assert_eq!(loaded.get::<u32>(&["key"]), Some(&42));

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_gss_watch_{}.gss", std::process::id()));
        std::fs::write(&file_path, "button = { width = 120, height = 40 },").unwrap();

        let watcher = StyleWatcher::new(&file_path);
        let style = watcher.load_or(Gss::new());
        assert_eq!(style.get::<u32>(&["button", "width"]), Some(&120));

        let _ = std::fs::remove_file(file_path);
    }
}
