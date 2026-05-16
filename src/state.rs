use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use zellij_tile::prelude::*;

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub const FLASH_DURATION_MS: u64 = 2000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Activity {
    Init,
    Thinking,
    Tool(String),
    Prompting,
    Waiting,
    Notification,
    Done,
    AgentDone,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub pane_id: u32,
    pub activity: Activity,
    pub tab_name: Option<String>,
    pub tab_index: Option<usize>,
    pub last_event_ts: u64,
    pub cwd: Option<String>,
    #[serde(default)]
    pub last_ts_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteFile {
    pub session_name: String,
    pub sessions: BTreeMap<u32, SessionInfo>,
    pub wrote_at_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct HookPayload {
    pub session_id: Option<String>,
    pub pane_id: u32,
    pub hook_event: String,
    pub tool_name: Option<String>,
    pub cwd: Option<String>,
    pub zellij_session: Option<String>,
    pub term_program: Option<String>,
    pub ts_ms: Option<u64>,
}

pub struct ClickRegion {
    pub start_col: usize,
    pub end_col: usize,
    pub tab_index: usize,
    pub pane_id: u32,
    pub is_waiting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RemoteTagKind {
    Waiting,
    Done,
}

pub struct RemoteTagClickRegion {
    pub start_col: usize,
    pub end_col: usize,
    pub session_name: String,
    pub kind: RemoteTagKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum NotifyMode {
    Never,
    Unfocused,
    #[default]
    Always,
}

impl NotifyMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Always => Self::Unfocused,
            Self::Unfocused => Self::Never,
            Self::Never => Self::Always,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum FlashMode {
    Off,
    #[default]
    Once,
    Persist,
}

impl FlashMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Once => Self::Persist,
            Self::Persist => Self::Off,
            Self::Off => Self::Once,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum BeepMode {
    Off,
    #[default]
    On,
    CrossSession,
}

impl BeepMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::On => Self::CrossSession,
            Self::CrossSession => Self::Off,
            Self::Off => Self::On,
        }
    }

    pub fn beeps_local(self) -> bool {
        matches!(self, Self::On)
    }

    pub fn beeps_remote(self) -> bool {
        matches!(self, Self::On | Self::CrossSession)
    }
}

/// Verbosity for the disk-backed debug log. `Off` disables logging entirely
/// (no `run_command` disk writes).
///
/// Do not reorder variants — `Ord` is load-bearing. Declaration order matches
/// severity (Off < Error < Warn < Info < Debug < Trace), and call sites gate
/// with `if settings.log_level >= LogLevel::Debug`. Alphabetizing or otherwise
/// shuffling the order would silently invert the gating and either suppress
/// all logs or open the floodgates depending on direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum LogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::Error,
            Self::Error => Self::Warn,
            Self::Warn => Self::Info,
            Self::Info => Self::Debug,
            Self::Debug => Self::Trace,
            Self::Trace => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub notifications: NotifyMode,
    pub flash: FlashMode,
    pub elapsed_time: bool,
    pub mode_indicator: bool,
    pub beep: BeepMode,
    pub cross_session_tag_max_len: usize,
    pub persist_cross_session_tags: bool,
    pub max_cross_session_tags: usize,
    pub log_level: LogLevel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: NotifyMode::Always,
            flash: FlashMode::Once,
            elapsed_time: true,
            mode_indicator: true,
            beep: BeepMode::On,
            cross_session_tag_max_len: 12,
            persist_cross_session_tags: false,
            max_cross_session_tags: 1,
            log_level: LogLevel::Off,
        }
    }
}

#[derive(Default, PartialEq)]
pub enum ViewMode {
    #[default]
    Normal,
    Settings,
}

#[derive(Clone, Copy)]
pub enum SettingKey {
    Notifications,
    Flash,
    ElapsedTime,
    ModeIndicator,
    Beep,
    PersistCrossSessionTags,
    MaxCrossSessionTags,
    LogLevel,
}

pub enum MenuAction {
    ToggleSetting(SettingKey),
    CloseMenu,
}

pub struct MenuClickRegion {
    pub start_col: usize,
    pub end_col: usize,
    pub action: MenuAction,
}

#[derive(Default)]
pub struct State {
    pub sessions: BTreeMap<u32, SessionInfo>,
    pub pane_to_tab: HashMap<u32, (usize, String)>,
    pub tabs: Vec<TabInfo>,
    pub pane_manifest: Option<PaneManifest>,
    pub active_tab_index: Option<usize>,
    pub click_regions: Vec<ClickRegion>,
    /// pane_id -> flash deadline in ms (for waiting animation)
    pub flash_deadlines: HashMap<u32, u64>,
    /// pane_ids that should emit a terminal bell on the next render
    pub beep_pending: HashSet<u32>,
    /// Set when a new cross-session tag is enqueued by reconcile_remote_tags;
    /// drained by render to emit a terminal bell. Not gated on active tab —
    /// cross-session events aren't bound to any local tab.
    pub beep_remote_pending: bool,
    pub zellij_session_name: Option<String>,
    pub term_program: Option<String>,
    pub input_mode: InputMode,
    pub settings: Settings,
    pub view_mode: ViewMode,
    pub prefix_click_region: Option<(usize, usize)>,
    pub menu_click_regions: Vec<MenuClickRegion>,
    pub config_loaded: bool,
    pub hooks_installed: bool,
    pub remote_sessions: BTreeMap<String, RemoteFile>,
    pub remote_tag_order: VecDeque<(String, RemoteTagKind)>,
    /// (name, kind) pairs we've already shown for the current remote-session
    /// lifetime. Once a tag has been presented (or dismissed via click, or
    /// superseded by a more urgent kind), we record it here so it can't pop
    /// back when the remote cycles through the same state again — e.g. a
    /// `/loop` repeatedly hitting Stop, or `cleanup_stale_sessions` flipping
    /// Done→Idle and the next iteration flipping it back. Cleared only when
    /// the remote disappears from `remote_sessions` entirely (file evicted
    /// after 30s of no heartbeats).
    pub remote_tag_presented: HashSet<(String, RemoteTagKind)>,
    /// False until the first `merge_remote_state` completes. The first poll
    /// seeds `remote_tag_presented` with whatever Done/Waiting pairs already
    /// exist on disk so attaching to a different Zellij session via Ctrl+o,w
    /// doesn't replay a bell + tag for state the user already saw locally
    /// before detaching. Only subsequent transitions notify.
    pub remote_bootstrap_done: bool,
    pub remote_tag_click_regions: Vec<RemoteTagClickRegion>,
    pub state_dirty: bool,
    pub last_write_ms: u64,
    pub last_poll_ms: u64,
}
