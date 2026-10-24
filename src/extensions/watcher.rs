#![allow(dead_code)]
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

pub struct ExtensionWatcher {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ExtensionWatcher {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Starts a background thread watching the given directory or default parts directory,
    /// sending changed script paths over the provided channel.
    pub fn start<F>(&mut self, watch_dirs: Vec<PathBuf>, on_change: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let on_change = Arc::new(on_change);

        let handle = thread::spawn(move || {
            let mut file_mtimes: HashMap<PathBuf, SystemTime> = HashMap::new();

            // Initial scan
            for dir in &watch_dirs {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file()
                            && path.extension().and_then(|s| s.to_str()) == Some("pts")
                            && let Ok(meta) = std::fs::metadata(&path)
                            && let Ok(mtime) = meta.modified()
                        {
                            file_mtimes.insert(path, mtime);
                        }
                    }
                }
            }

            while running.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(800));

                let removed: Vec<_> = file_mtimes.keys().filter(|p| !p.exists()).cloned().collect();
                for path in removed {
                    file_mtimes.remove(&path);
                    on_change(path.to_string_lossy().into_owned());
                }

                for dir in &watch_dirs {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file()
                                && path.extension().and_then(|s| s.to_str()) == Some("pts")
                                && let Ok(meta) = std::fs::metadata(&path)
                                && let Ok(mtime) = meta.modified()
                            {
                                let is_changed = match file_mtimes.get(&path) {
                                    Some(&prev_time) => mtime != prev_time,
                                    None => true,
                                };

                                if is_changed {
                                    file_mtimes.insert(path.clone(), mtime);
                                    let path_str = path.to_string_lossy().to_string();
                                    eprintln!(
                                        "[ExtensionWatcher] Detected change in script: {}",
                                        path_str
                                    );
                                    on_change(path_str);
                                }
                            }
                        }
                    }
                }
            }
        });

        self.handle = Some(handle);
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ExtensionWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watches_added_edited_and_deleted_scripts() {
        let dir = std::env::temp_dir().join(format!("cedarapple-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("example.pts");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = ExtensionWatcher::new();
        watcher.start(vec![dir.clone()], move |p| { let _ = tx.send(p); });
        // Let the initial snapshot finish before adding the first file.
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(&path, "return |> <|;").unwrap();
        assert_eq!(PathBuf::from(rx.recv_timeout(Duration::from_secs(4)).unwrap()), path);
        std::fs::write(&path, "return |> name: 'edited' <|;").unwrap();
        assert_eq!(PathBuf::from(rx.recv_timeout(Duration::from_secs(4)).unwrap()), path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(PathBuf::from(rx.recv_timeout(Duration::from_secs(4)).unwrap()), path);
        watcher.stop();
        std::fs::remove_dir(dir).unwrap();
    }
}
