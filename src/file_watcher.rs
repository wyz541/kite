use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub fn trace_directory_refresh(message: &str) {
    let Some(path) = std::env::var_os("KITE_FILE_WATCH_TRACE") else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

pub struct DirectoryWatcher {
    watcher: RecommendedWatcher,
    watched_paths: BTreeSet<PathBuf>,
}

impl DirectoryWatcher {
    pub fn new(mut on_change: impl FnMut(Vec<PathBuf>) + Send + 'static) -> Result<Self, String> {
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            let Ok(event) = result else {
                return;
            };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            on_change(event.paths);
        })
        .map_err(|error| format!("cannot start directory watcher: {error}"))?;

        Ok(Self {
            watcher,
            watched_paths: BTreeSet::new(),
        })
    }

    pub fn set_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let desired = paths
            .into_iter()
            .filter(|path| path.is_dir())
            .map(|path| path.canonicalize().unwrap_or(path))
            .collect::<BTreeSet<_>>();

        let removed = self
            .watched_paths
            .difference(&desired)
            .cloned()
            .collect::<Vec<_>>();
        for path in removed {
            let _ = self.watcher.unwatch(&path);
            self.watched_paths.remove(&path);
        }

        for path in desired {
            if self.watched_paths.contains(&path) {
                continue;
            }
            if self
                .watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .is_ok()
            {
                self.watched_paths.insert(path);
            }
        }
    }

    pub fn clear(&mut self) {
        self.set_paths(std::iter::empty());
    }

    pub fn rearm_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.clear();
        self.set_paths(paths);
    }

    pub fn watched_paths(&self) -> impl Iterator<Item = &Path> {
        self.watched_paths.iter().map(PathBuf::as_path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn reports_immediate_directory_changes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "kite-directory-watcher-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create watched directory");

        let (sender, receiver) = mpsc::channel();
        let mut watcher = DirectoryWatcher::new(move |_| {
            let _ = sender.send(());
        })
        .expect("create directory watcher");
        watcher.set_paths([directory.clone()]);
        fs::write(directory.join("created.txt"), b"created").expect("create watched file");

        let changed = receiver.recv_timeout(Duration::from_secs(5)).is_ok();
        watcher.clear();
        fs::remove_dir_all(&directory).expect("remove watched directory");
        assert!(changed, "directory watcher did not report file creation");
    }
}
