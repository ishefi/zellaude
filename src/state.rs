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

pub struct RemoteTagClickRegion {
    pub start_col: usize,
    pub end_col: usize,
    pub session_name: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub notifications: NotifyMode,
    pub flash: FlashMode,
    pub elapsed_time: bool,
    pub mode_indicator: bool,
    pub beep_enabled: bool,
    pub remote_name_max_len: usize,
    pub persist_remote_tags: bool,
    pub max_remote_tags: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notifications: NotifyMode::Always,
            flash: FlashMode::Once,
            elapsed_time: true,
            mode_indicator: true,
            beep_enabled: true,
            remote_name_max_len: 12,
            persist_remote_tags: false,
            max_remote_tags: 1,
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
    BeepEnabled,
    PersistRemoteTags,
    MaxRemoteTags,
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
    pub remote_tag_order: VecDeque<String>,
    /// Names dismissed via click. Suppresses re-adding to the queue until
    /// the remote leaves Waiting at least once — without this, a click on a
    /// still-Waiting remote pops back ~1s later via reconcile.
    pub remote_tag_dismissed: HashSet<String>,
    pub remote_tag_click_regions: Vec<RemoteTagClickRegion>,
    pub state_dirty: bool,
    pub last_write_ms: u64,
    pub last_poll_ms: u64,
}
