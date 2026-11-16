//! Watches the skills folders shown, and tells the window when they change.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

/// The folders watched: `dirs` with everything in them, `roots` for their own entries only.
/// Nothing is watched until the window gives a waker, so tests and headless runs watch nothing.
#[derive(Default)]
pub struct Watcher {
    waker: Option<rune::Waker>,
    changed: Arc<AtomicBool>,
    watching: Option<RecommendedWatcher>,
    key: (Vec<PathBuf>, Vec<PathBuf>),
}

impl Watcher {
    pub fn set_waker(&mut self, waker: rune::Waker) {
        self.waker = Some(waker);
    }

    /// Watches these folders instead of the ones before; the same folders keep their watch.
    pub fn watch(&mut self, mut dirs: Vec<PathBuf>, mut roots: Vec<PathBuf>) {
        let Some(waker) = self.waker.clone() else {
            return;
        };
        for list in [&mut dirs, &mut roots] {
            list.sort();
            list.dedup();
        }
        let key = (dirs, roots);
        if self.watching.is_some() && key == self.key {
            return;
        }
        let changed = self.changed.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // Reading a file is no change to it.
            if event.is_ok_and(|e| !e.kind.is_access()) && !changed.swap(true, Ordering::Relaxed) {
                waker();
            }
        });
        self.watching = watcher.ok().map(|mut w| {
            for (list, mode) in [
                (&key.0, RecursiveMode::Recursive),
                (&key.1, RecursiveMode::NonRecursive),
            ] {
                for path in list {
                    // A folder that went since it was listed is left out.
                    let _ = w.watch(path, mode);
                }
            }
            w
        });
        self.key = key;
    }

    /// Whether something changed since the last call.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}
