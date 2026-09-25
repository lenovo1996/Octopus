//! Versioned local persistence (security/data §persistence schema v1 subset).
//! T03 owns recents + trust; T14 extends settings with migrations.
//! Atomic temp-write + rename; corrupted files are backed up, then defaulted.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::domain::OpenWorkspaceEntry;
use crate::domain::RecentEntry;

pub const SCHEMA_VERSION: u32 = 1;
// Keep the existing filename when rebranding to Octopus so saved workspaces,
// preferences, recents and trust remain available without a data migration.
const FILE_NAME: &str = "gitdock-settings.json";
const MAX_RECENTS: usize = 20;
const MAX_OPEN_WORKSPACES: usize = 20;

/// Settings v1 (T14). `version` is an optimistic-concurrency counter bumped
/// by every `settings_update`; `font_scale` multiplies the base UI font.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsV1 {
    pub version: u64,
    pub font_scale: f32,
}

impl Default for SettingsV1 {
    fn default() -> Self {
        Self {
            version: 1,
            font_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreData {
    pub schema_version: u32,
    pub recent_repositories: Vec<RecentEntry>,
    pub trusted_repositories: Vec<String>,
    #[serde(default)]
    pub settings: SettingsV1,
    /// Workspaces left open when the app last saved (T19). Restored on
    /// launch; entries whose repositories vanished are pruned on restore.
    #[serde(default)]
    pub open_workspaces: Vec<OpenWorkspaceEntry>,
    #[serde(default)]
    pub active_workspace: Option<String>,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            recent_repositories: Vec::new(),
            trusted_repositories: Vec::new(),
            settings: SettingsV1::default(),
            open_workspaces: Vec::new(),
            active_workspace: None,
        }
    }
}

pub struct Store {
    path: PathBuf,
    data: StoreData,
}

impl Store {
    pub fn file_name() -> &'static str {
        FILE_NAME
    }

    pub fn open(dir: &Path) -> Self {
        let path = dir.join(FILE_NAME);
        let data = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StoreData>(&bytes) {
                Ok(mut data) => {
                    // Migration v0 -> v1: pre-settings files gain defaults.
                    // Newer-than-supported schemas fail safe: back up, default.
                    if data.schema_version == 0 {
                        data.schema_version = SCHEMA_VERSION;
                    }
                    if data.schema_version > SCHEMA_VERSION {
                        let backup = path.with_extension("json.corrupt");
                        let _ = std::fs::rename(&path, backup);
                        StoreData::default()
                    } else {
                        data
                    }
                }
                Err(_) => {
                    let backup = path.with_extension("json.corrupt");
                    let _ = std::fs::rename(&path, backup);
                    StoreData::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreData::default(),
            Err(_) => StoreData::default(),
        };
        Self { path, data }
    }

    pub fn settings(&self) -> &SettingsV1 {
        &self.data.settings
    }

    /// Apply a validated font scale, bumping the settings version.
    /// Returns the new settings. Callers validate the range first.
    pub fn set_font_scale(&mut self, font_scale: f32) -> SettingsV1 {
        self.data.settings.font_scale = font_scale;
        self.data.settings.version = self.data.settings.version.saturating_add(1);
        let _ = self.save();
        self.data.settings.clone()
    }

    pub fn recents(&self) -> &[RecentEntry] {
        &self.data.recent_repositories
    }

    pub fn is_trusted(&self, key: &str) -> bool {
        self.data.trusted_repositories.iter().any(|k| k == key)
    }

    pub fn set_trusted(&mut self, key: &str, trusted: bool) {
        if trusted {
            if !self.is_trusted(key) {
                self.data.trusted_repositories.push(key.to_string());
            }
        } else {
            self.data.trusted_repositories.retain(|k| k != key);
        }
        let _ = self.save();
    }

    pub fn push_recent(&mut self, key: &str, display_path: &str) {
        self.data.recent_repositories.retain(|e| {
            e.key != key && (e.key.starts_with("worktree-v1:") || e.display_path != display_path)
        });
        self.data.recent_repositories.insert(
            0,
            RecentEntry {
                entry_id: key.to_string(),
                key: key.to_string(),
                display_path: display_path.to_string(),
                last_opened_at: now_epoch_secs(),
            },
        );
        self.data.recent_repositories.truncate(MAX_RECENTS);
        let _ = self.save();
    }

    pub fn open_workspaces(&self) -> &[OpenWorkspaceEntry] {
        &self.data.open_workspaces
    }

    pub fn active_workspace(&self) -> Option<&str> {
        self.data.active_workspace.as_deref()
    }

    /// Replace the saved workspace list (tab order preserved, capped).
    /// Keys must look like backend-issued worktree keys; anything else is
    /// dropped instead of persisted.
    pub fn save_open_workspaces(
        &mut self,
        entries: Vec<OpenWorkspaceEntry>,
        active_key: Option<String>,
    ) {
        let mut seen = std::collections::HashSet::new();
        let mut kept: Vec<OpenWorkspaceEntry> = Vec::new();
        for entry in entries {
            if !is_workspace_key(&entry.key) || !seen.insert(entry.key.clone()) {
                continue;
            }
            kept.push(entry);
            if kept.len() >= MAX_OPEN_WORKSPACES {
                break;
            }
        }
        self.data.open_workspaces = kept;
        self.data.active_workspace = active_key.filter(|key| is_workspace_key(key));
        let _ = self.save();
    }

    /// Drop saved entries that failed to reopen (moved or deleted repos).
    pub fn prune_open_workspaces(&mut self, drop_keys: &[String]) {
        if drop_keys.is_empty() {
            return;
        }
        self.data
            .open_workspaces
            .retain(|entry| !drop_keys.iter().any(|key| key == &entry.key));
        if let Some(active) = self.data.active_workspace.clone() {
            if drop_keys.iter().any(|key| key == &active) {
                self.data.active_workspace = None;
            }
        }
        let _ = self.save();
    }

    /// Returns true when an entry was actually removed.
    pub fn remove_recent(&mut self, entry_id: &str) -> bool {
        let before = self.data.recent_repositories.len();
        self.data
            .recent_repositories
            .retain(|e| e.entry_id != entry_id);
        let removed = self.data.recent_repositories.len() != before;
        if removed {
            let _ = self.save();
        }
        removed
    }

    fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&self.data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn is_workspace_key(key: &str) -> bool {
    let hex = match key.strip_prefix("worktree-v1:") {
        Some(hex) => hex,
        None => return false,
    };
    !hex.is_empty()
        && hex.len() <= 4096
        && hex.len() % 2 == 0
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
