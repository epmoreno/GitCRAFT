//! The history of git commands shown in the operations panel: every command
//! GitCRAFT runs, newest last, with its duration and outcome. Only the
//! command line is kept (credentials already redacted by the core), never
//! its output or environment.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::UNIX_EPOCH;

use gitcraft_core::{Access, CommandEvent, CommandLog, CommandOutcome};
use serde::Serialize;

/// Commands kept; older ones are forgotten.
pub const HISTORY_LIMIT: usize = 1000;

/// One git command, running (`outcome` is `None`) or finished.
#[derive(
    Debug, Clone, PartialEq, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event,
)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub id: u32,
    /// Folder git ran in.
    pub cwd: String,
    /// Exact command line, quoted for a shell, credentials redacted.
    pub command: String,
    /// May modify the repository (as opposed to only reading it).
    pub write: bool,
    /// Milliseconds since the Unix epoch (a whole number; typed as a plain
    /// number for TypeScript, where it is exact).
    #[specta(type = i32)]
    pub started_at: f64,
    pub duration_ms: Option<u32>,
    pub outcome: Option<CommandOutcome>,
}

/// Called with each new or updated record.
pub type RecordNotifier = Arc<dyn Fn(CommandRecord) + Send + Sync>;

pub struct CommandHistory {
    records: Mutex<VecDeque<CommandRecord>>,
    notify: RecordNotifier,
}

impl CommandHistory {
    pub fn new(notify: RecordNotifier) -> Self {
        Self {
            records: Mutex::new(VecDeque::new()),
            notify,
        }
    }

    /// Every kept record, oldest first.
    pub fn records(&self) -> Vec<CommandRecord> {
        self.lock().iter().cloned().collect()
    }

    /// Forgets the finished commands; running ones stay.
    pub fn clear(&self) {
        self.lock().retain(|r| r.outcome.is_none());
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<CommandRecord>> {
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CommandLog for CommandHistory {
    fn record(&self, event: CommandEvent) {
        let record = {
            let mut records = self.lock();
            match event {
                CommandEvent::Started {
                    id,
                    cwd,
                    command,
                    access,
                    started,
                } => {
                    let record = CommandRecord {
                        // Ids only need to be unique among the kept records.
                        id: id as u32,
                        cwd: cwd.to_string_lossy().into_owned(),
                        command,
                        write: access == Access::Write,
                        started_at: started
                            .duration_since(UNIX_EPOCH)
                            .map_or(0.0, |d| d.as_millis() as f64),
                        duration_ms: None,
                        outcome: None,
                    };
                    records.push_back(record.clone());
                    while records.len() > HISTORY_LIMIT {
                        records.pop_front();
                    }
                    record
                }
                CommandEvent::Finished {
                    id,
                    duration,
                    outcome,
                } => {
                    let Some(record) = records.iter_mut().rev().find(|r| r.id == id as u32) else {
                        return;
                    };
                    record.duration_ms =
                        Some(u32::try_from(duration.as_millis()).unwrap_or(u32::MAX));
                    record.outcome = Some(outcome);
                    record.clone()
                }
            }
        };
        (self.notify)(record);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;

    fn history() -> (CommandHistory, Arc<Mutex<Vec<CommandRecord>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&sent);
        let history = CommandHistory::new(Arc::new(move |r| {
            sink.lock().expect("sink").push(r);
        }));
        (history, sent)
    }

    fn started(id: u64) -> CommandEvent {
        CommandEvent::Started {
            id,
            cwd: PathBuf::from("/repo"),
            command: format!("git status {id}"),
            access: Access::Read,
            started: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        }
    }

    #[test]
    fn records_start_and_end_and_notifies_both() {
        let (history, sent) = history();
        history.record(started(7));
        history.record(CommandEvent::Finished {
            id: 7,
            duration: Duration::from_millis(42),
            outcome: CommandOutcome::Failed { code: Some(128) },
        });
        let records = history.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].started_at, 1000.0);
        assert_eq!(records[0].duration_ms, Some(42));
        assert_eq!(
            records[0].outcome,
            Some(CommandOutcome::Failed { code: Some(128) })
        );
        let sent = sent.lock().expect("sent");
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].outcome, None);
        assert_eq!(sent[1], records[0]);
    }

    #[test]
    fn keeps_only_the_newest_commands() {
        let (history, _) = history();
        for id in 0..(HISTORY_LIMIT as u64 + 5) {
            history.record(started(id));
        }
        let records = history.records();
        assert_eq!(records.len(), HISTORY_LIMIT);
        assert_eq!(records[0].id, 5);
    }

    #[test]
    fn clearing_keeps_running_commands() {
        let (history, _) = history();
        history.record(started(1));
        history.record(started(2));
        history.record(CommandEvent::Finished {
            id: 1,
            duration: Duration::ZERO,
            outcome: CommandOutcome::Succeeded,
        });
        history.clear();
        let ids: Vec<u32> = history.records().iter().map(|r| r.id).collect();
        assert_eq!(ids, [2]);
    }
}
