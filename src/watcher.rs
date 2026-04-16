use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Simplified file watcher using std channels (no tokio).
pub struct FileWatcher {
    rx: mpsc::Receiver<PathBuf>,
    _watcher: notify::RecommendedWatcher,
    last_event: Instant,
    pending: Option<PathBuf>,
}

impl FileWatcher {
    pub fn new(project_dir: &PathBuf) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |event: Result<Event, _>| {
            if let Ok(event) = event {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in event.paths {
                            if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                                let _ = tx.send(path);
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        watcher
            .watch(project_dir, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch directory: {}", e))?;

        Ok(Self {
            rx,
            _watcher: watcher,
            last_event: Instant::now(),
            pending: None,
        })
    }

    /// Check for file changes. Returns a path if a file changed
    /// (with 200ms debounce).
    pub fn poll(&mut self) -> Option<PathBuf> {
        // Drain all pending events
        while let Ok(path) = self.rx.try_recv() {
            self.pending = Some(path);
            self.last_event = Instant::now();
        }

        // Debounce: only emit after 200ms of quiet
        if let Some(path) = &self.pending {
            if self.last_event.elapsed() >= Duration::from_millis(200) {
                let p = path.clone();
                self.pending = None;
                return Some(p);
            }
        }

        None
    }
}
