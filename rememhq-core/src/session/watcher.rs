use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

pub struct TranscriptWatcher {
    directory: PathBuf,
}

impl TranscriptWatcher {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Starts watching the directory in a background task.
    /// Yields modified file paths.
    pub fn watch(&self) -> mpsc::Receiver<PathBuf> {
        let (tx, rx) = mpsc::channel(100);
        let dir = self.directory.clone();

        tokio::spawn(async move {
            let (std_tx, std_rx) = channel();
            
            let mut watcher = match RecommendedWatcher::new(std_tx, Config::default()) {
                Ok(w) => w,
                Err(e) => {
                    error!("Failed to create watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
                error!("Failed to watch directory {}: {}", dir.display(), e);
                return;
            }

            info!("Watching for transcript changes in {}", dir.display());

            // Poll the std mpsc channel for events
            loop {
                if let Ok(res) = std_rx.recv_timeout(Duration::from_millis(500)) {
                    match res {
                        Ok(event) => {
                            // Basic logic: if a file is modified/created, send its path.
                            // We only care about .jsonl files.
                            for path in event.paths {
                                if let Some(ext) = path.extension() {
                                    if ext == "jsonl" {
                                        let _ = tx.send(path).await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Watcher error: {:?}", e);
                        }
                    }
                }
                
                // Yield to async runtime
                tokio::task::yield_now().await;
            }
        });

        rx
    }
}
