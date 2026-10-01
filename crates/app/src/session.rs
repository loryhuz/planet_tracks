//! The tuning session: every gameplay profile with its tuned parameters, best run and
//! whether the player eliminated it. Saved to tuning/session.json so it survives restarts.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use physics::CarParams;
use serde::{Deserialize, Serialize};

use crate::race::Frame;

pub struct Best {
    pub ticks: u32,
    pub splits: Vec<u32>,
    /// The parameters the record was driven with (JSON), to tell whether the ghost still applies.
    pub params_json: String,
    pub frames: Vec<Frame>,
}

pub struct Profile {
    pub params: CarParams,
    pub defaults: CarParams,
    pub best: Option<Best>,
    pub runs: u32,
    pub finishes: u32,
    pub eliminated: bool,
}

impl Profile {
    pub fn params_json(&self) -> String {
        serde_json::to_string(&self.params).unwrap_or_default()
    }

    /// The best run, if it was driven with the current parameters.
    pub fn current_best(&self) -> Option<&Best> {
        let json = self.params_json();
        self.best.as_ref().filter(|b| b.params_json == json)
    }
}

pub struct Session {
    pub profiles: Vec<Profile>,
    pub current: usize,
    dirty_since: Option<Instant>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    current: usize,
    profiles: Vec<SavedProfile>,
}

#[derive(Serialize, Deserialize)]
struct SavedProfile {
    name: String,
    params: serde_json::Value,
    /// The preset the tuned params started from; when the code's preset changes, the old
    /// tuning is dropped so a new physics version is never hidden behind stale values.
    #[serde(default)]
    defaults: String,
    runs: u32,
    finishes: u32,
    eliminated: bool,
    best_ticks: Option<u32>,
    #[serde(default)]
    best_splits: Vec<u32>,
    #[serde(default)]
    best_params: String,
    #[serde(default)]
    best_frames: Vec<Frame>,
}

pub fn path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tuning/session.json"))
}

impl Session {
    pub fn load() -> Self {
        let saved: Saved = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let profiles = physics::presets()
            .into_iter()
            .map(|preset| {
                let s = saved.profiles.iter().find(|p| p.name == preset.name);
                let preset_json = serde_json::to_string(&preset).unwrap_or_default();
                let params = s
                    .filter(|s| s.defaults == preset_json)
                    .and_then(|s| serde_json::from_value::<CarParams>(s.params.clone()).ok())
                    .unwrap_or_else(|| preset.clone());
                Profile {
                    params,
                    defaults: preset,
                    best: s.and_then(|s| {
                        s.best_ticks.map(|ticks| Best {
                            ticks,
                            splits: s.best_splits.clone(),
                            params_json: s.best_params.clone(),
                            frames: s.best_frames.clone(),
                        })
                    }),
                    runs: s.map_or(0, |s| s.runs),
                    finishes: s.map_or(0, |s| s.finishes),
                    eliminated: s.is_some_and(|s| s.eliminated),
                }
            })
            .collect::<Vec<_>>();
        let current = saved.current.min(profiles.len().saturating_sub(1));
        Self { profiles, current, dirty_since: None }
    }

    pub fn profile(&self) -> &Profile {
        &self.profiles[self.current]
    }

    pub fn profile_mut(&mut self) -> &mut Profile {
        &mut self.profiles[self.current]
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    /// Saves a moment after the last change (sliders fire every frame while dragged).
    pub fn autosave(&mut self) {
        if self.dirty_since.is_some_and(|t| t.elapsed() > Duration::from_millis(800)) {
            self.save();
        }
    }

    pub fn save(&mut self) {
        self.dirty_since = None;
        let saved = Saved {
            current: self.current,
            profiles: self
                .profiles
                .iter()
                .map(|p| SavedProfile {
                    name: p.defaults.name.clone(),
                    params: serde_json::to_value(&p.params).unwrap_or_default(),
                    defaults: serde_json::to_string(&p.defaults).unwrap_or_default(),
                    runs: p.runs,
                    finishes: p.finishes,
                    eliminated: p.eliminated,
                    best_ticks: p.best.as_ref().map(|b| b.ticks),
                    best_splits: p.best.as_ref().map(|b| b.splits.clone()).unwrap_or_default(),
                    best_params: p.best.as_ref().map(|b| b.params_json.clone()).unwrap_or_default(),
                    best_frames: p.best.as_ref().map(|b| b.frames.clone()).unwrap_or_default(),
                })
                .collect(),
        };
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&saved) {
            let _ = std::fs::write(path, json);
        }
    }
}
