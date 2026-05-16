mod event_handler;
mod installer;
mod render;
mod state;
mod tab_pane_map;

use state::{
    unix_now, unix_now_ms, HookPayload, MenuAction, RemoteFile, RemoteTagKind, SessionInfo,
    Settings, State, ViewMode,
};
use std::collections::{BTreeMap, HashSet};
use zellij_tile::prelude::*;

const DONE_TIMEOUT: u64 = 30;
const TIMER_INTERVAL: f64 = 1.0;
const FLASH_TICK: f64 = 0.25;

register_plugin!(State);

fn dismiss_kind_str(kind: RemoteTagKind) -> &'static str {
    match kind {
        RemoteTagKind::Waiting => "waiting",
        RemoteTagKind::Done => "done",
    }
}

fn parse_dismiss_payload(payload: &str) -> Option<(RemoteTagKind, String)> {
    let payload = payload.trim();
    let (prefix, name) = payload.split_once(':')?;
    let kind = match prefix {
        "waiting" => RemoteTagKind::Waiting,
        "done" => RemoteTagKind::Done,
        _ => return None,
    };
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((kind, name.to_string()))
}

fn remote_in_state(remote: &RemoteFile, kind: RemoteTagKind) -> bool {
    remote.sessions.values().any(|s| match kind {
        RemoteTagKind::Waiting => matches!(s.activity, state::Activity::Waiting),
        RemoteTagKind::Done => {
            matches!(
                s.activity,
                state::Activity::Done | state::Activity::AgentDone
            )
        }
    })
}

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
            PermissionType::ReadCliPipes,
            PermissionType::MessageAndLaunchOtherPlugins,
        ]);
        subscribe(&[
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::ModeUpdate,
            EventType::Timer,
            EventType::Mouse,
            EventType::RunCommandResult,
            EventType::PermissionRequestResult,
        ]);
        set_timeout(TIMER_INTERVAL);

        // Load persisted settings (may be retried in PermissionRequestResult
        // if this fires before permissions are granted)
        self.load_config();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => {
                let new_active = tabs.iter().find(|t| t.active).map(|t| t.position);
                if new_active != self.active_tab_index {
                    // Tab focus changed — clear persist flashes on the newly focused tab
                    if let Some(idx) = new_active {
                        self.clear_flashes_on_tab(idx);
                    }
                }
                self.active_tab_index = new_active;
                self.tabs = tabs;
                self.rebuild_pane_map();
                true
            }
            Event::PaneUpdate(manifest) => {
                self.pane_manifest = Some(manifest);
                self.rebuild_pane_map();
                true
            }
            Event::ModeUpdate(mode_info) => {
                self.input_mode = mode_info.mode;
                if let Some(name) = mode_info.session_name {
                    self.zellij_session_name = Some(name);
                }
                true
            }
            Event::Mouse(Mouse::LeftClick(_, col)) => {
                let col = col as usize;

                // Check prefix click region first → toggle ViewMode
                if let Some((start, end)) = self.prefix_click_region {
                    if col >= start && col < end {
                        self.view_mode = match self.view_mode {
                            ViewMode::Normal => ViewMode::Settings,
                            ViewMode::Settings => ViewMode::Normal,
                        };
                        return true;
                    }
                }

                match self.view_mode {
                    ViewMode::Normal => {
                        for region in &self.remote_tag_click_regions {
                            if col >= region.start_col && col < region.end_col {
                                self.broadcast_dismiss_tag(&region.session_name, region.kind);
                                return false;
                            }
                        }
                        for region in &self.click_regions {
                            if col >= region.start_col && col < region.end_col {
                                if region.is_waiting {
                                    focus_terminal_pane(region.pane_id, false);
                                } else {
                                    switch_tab_to(region.tab_index as u32 + 1);
                                }
                                return false;
                            }
                        }
                        false
                    }
                    ViewMode::Settings => {
                        for region in &self.menu_click_regions {
                            if col >= region.start_col && col < region.end_col {
                                match &region.action {
                                    MenuAction::ToggleSetting(key) => {
                                        match key {
                                            state::SettingKey::Notifications => {
                                                self.settings.notifications =
                                                    self.settings.notifications.cycle();
                                            }
                                            state::SettingKey::Flash => {
                                                self.settings.flash = self.settings.flash.cycle();
                                            }
                                            state::SettingKey::ElapsedTime => {
                                                self.settings.elapsed_time =
                                                    !self.settings.elapsed_time;
                                            }
                                            state::SettingKey::ModeIndicator => {
                                                self.settings.mode_indicator =
                                                    !self.settings.mode_indicator;
                                            }
                                            state::SettingKey::Beep => {
                                                self.settings.beep = self.settings.beep.cycle();
                                            }
                                            state::SettingKey::PersistCrossSessionTags => {
                                                self.settings.persist_cross_session_tags =
                                                    !self.settings.persist_cross_session_tags;
                                                self.reconcile_remote_tags();
                                            }
                                            state::SettingKey::MaxCrossSessionTags => {
                                                self.settings.max_cross_session_tags =
                                                    match self.settings.max_cross_session_tags {
                                                        1 => 2,
                                                        2 => 3,
                                                        3 => 4,
                                                        _ => 1,
                                                    };
                                            }
                                        }
                                        self.save_config();
                                    }
                                    MenuAction::CloseMenu => {
                                        self.view_mode = ViewMode::Normal;
                                    }
                                }
                                return true;
                            }
                        }
                        false
                    }
                }
            }
            Event::RunCommandResult(exit_code, stdout, _stderr, context) => {
                match context.get("type").map(|s| s.as_str()) {
                    Some("load_config") if exit_code == Some(0) => {
                        let raw = String::from_utf8_lossy(&stdout);
                        if let Ok(settings) = serde_json::from_str::<Settings>(raw.trim()) {
                            self.settings = settings;
                        }
                        self.config_loaded = true;
                        true
                    }
                    Some("install_hooks") => {
                        self.hooks_installed = true;
                        false
                    }
                    Some("poll_remote") if exit_code == Some(0) => {
                        let raw = String::from_utf8_lossy(&stdout);
                        self.merge_remote_state(&raw);
                        true
                    }
                    Some("write_state") => false,
                    _ => false,
                }
            }
            Event::Timer(_) => {
                let stale_changed = self.cleanup_stale_sessions();
                let flash_changed = self.cleanup_expired_flashes();
                if stale_changed {
                    self.state_dirty = true;
                }
                let now_ms = unix_now_ms();
                if now_ms.saturating_sub(self.last_poll_ms) >= 1000 {
                    self.poll_remote_state();
                }
                if self.state_dirty && now_ms.saturating_sub(self.last_write_ms) >= 250 {
                    self.write_own_state();
                } else if self.has_persistable_session()
                    && now_ms.saturating_sub(self.last_write_ms) >= 10_000
                {
                    // Heartbeat: refresh our state file so peers don't drop us
                    // as stale (30s threshold) while a Waiting/Done tag is
                    // legitimately parked waiting for the user.
                    self.write_own_state();
                }
                let has_flashes = self.has_active_flashes();
                if has_flashes {
                    set_timeout(FLASH_TICK);
                } else {
                    set_timeout(TIMER_INTERVAL);
                }
                has_flashes || stale_changed || flash_changed || self.has_elapsed_display()
            }
            Event::PermissionRequestResult(_) => {
                // Now that permissions are granted, mark as non-selectable
                // so the plugin stays visible during fullscreen
                set_selectable(false);
                // Permissions granted — ask existing instances for their state
                self.request_sync();
                // Retry config load (the one in load() may have been dropped
                // because it ran before permissions were granted)
                if !self.config_loaded {
                    self.load_config();
                }
                // Auto-install hook script and register Claude Code hooks
                if !self.hooks_installed {
                    installer::run_install();
                }
                false
            }
            _ => false,
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        match pipe_message.name.as_str() {
            "zellaude" => {
                // Hook event from CLI
                let payload_str = match pipe_message.payload {
                    Some(ref s) => s,
                    None => return false,
                };
                let payload: HookPayload = match serde_json::from_str(payload_str) {
                    Ok(p) => p,
                    Err(_) => return false,
                };
                event_handler::handle_hook_event(self, payload);
                true
            }
            "zellaude:focus" => {
                // Notification click — focus the requested pane
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(pane_id) = payload.trim().parse::<u32>() {
                        focus_terminal_pane(pane_id, false);
                    }
                }
                false
            }
            "zellaude:request" => {
                // Another instance asking for state — respond with ours
                self.broadcast_sessions();
                false
            }
            "zellaude:settings" => {
                // Another instance broadcast new settings
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(settings) = serde_json::from_str::<Settings>(payload) {
                        self.settings = settings;
                        return true;
                    }
                }
                false
            }
            "zellaude:sync" => {
                // Another instance sharing state — merge it
                if let Some(ref payload) = pipe_message.payload {
                    if let Ok(sessions) =
                        serde_json::from_str::<BTreeMap<u32, SessionInfo>>(payload)
                    {
                        self.merge_sessions(sessions);
                        return true;
                    }
                }
                false
            }
            "zellaude:dismiss-tag" => {
                // Another instance (or this one's click handler) is dismissing
                // a remote tag. Apply locally so every tab in the server agrees.
                if let Some(ref payload) = pipe_message.payload {
                    if let Some((kind, name)) = parse_dismiss_payload(payload) {
                        let key = (name, kind);
                        self.remote_tag_order.retain(|entry| entry != &key);
                        self.remote_tag_dismissed.insert(key);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        render::render_status_bar(self, rows, cols);
    }
}

impl State {
    fn rebuild_pane_map(&mut self) {
        if let Some(ref manifest) = self.pane_manifest {
            self.pane_to_tab = tab_pane_map::build_pane_to_tab_map(&self.tabs, manifest);
            self.refresh_session_tab_names();
            self.remove_dead_panes();
        }
    }

    fn refresh_session_tab_names(&mut self) {
        for session in self.sessions.values_mut() {
            if let Some((idx, name)) = self.pane_to_tab.get(&session.pane_id) {
                session.tab_index = Some(*idx);
                session.tab_name = Some(name.clone());
            }
        }
    }

    fn remove_dead_panes(&mut self) {
        self.sessions
            .retain(|pane_id, _| self.pane_to_tab.contains_key(pane_id));
    }

    fn cleanup_stale_sessions(&mut self) -> bool {
        let now = unix_now();
        let mut changed = false;
        for session in self.sessions.values_mut() {
            match session.activity {
                state::Activity::Done | state::Activity::AgentDone => {
                    if now.saturating_sub(session.last_event_ts) >= DONE_TIMEOUT {
                        session.activity = state::Activity::Idle;
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        changed
    }

    fn clear_flashes_on_tab(&mut self, tab_idx: usize) {
        let pane_ids: Vec<u32> = self
            .sessions
            .values()
            .filter(|s| s.tab_index == Some(tab_idx))
            .map(|s| s.pane_id)
            .collect();
        for pane_id in pane_ids {
            self.flash_deadlines.remove(&pane_id);
        }
    }

    fn has_active_flashes(&self) -> bool {
        let now = unix_now_ms();
        self.flash_deadlines
            .values()
            .any(|&deadline| now < deadline)
    }

    fn cleanup_expired_flashes(&mut self) -> bool {
        let before = self.flash_deadlines.len();
        let now = unix_now_ms();
        self.flash_deadlines.retain(|_, deadline| now < *deadline);
        self.flash_deadlines.len() != before
    }

    fn has_elapsed_display(&self) -> bool {
        if !self.settings.elapsed_time {
            return false;
        }
        let now = unix_now();
        self.sessions.values().any(|s| {
            !matches!(s.activity, state::Activity::Idle)
                && now.saturating_sub(s.last_event_ts) >= DONE_TIMEOUT
        })
    }

    fn has_persistable_session(&self) -> bool {
        self.sessions.values().any(|s| {
            matches!(
                s.activity,
                state::Activity::Waiting | state::Activity::Done | state::Activity::AgentDone
            )
        })
    }

    fn request_sync(&self) {
        pipe_message_to_plugin(MessageToPlugin::new("zellaude:request"));
    }

    fn broadcast_sessions(&self) {
        let mut msg = MessageToPlugin::new("zellaude:sync");
        msg.message_payload = Some(serde_json::to_string(&self.sessions).unwrap_or_default());
        pipe_message_to_plugin(msg);
    }

    fn broadcast_settings(&self) {
        let mut msg = MessageToPlugin::new("zellaude:settings");
        msg.message_payload = Some(serde_json::to_string(&self.settings).unwrap_or_default());
        pipe_message_to_plugin(msg);
    }

    fn broadcast_dismiss_tag(&self, session_name: &str, kind: RemoteTagKind) {
        let mut msg = MessageToPlugin::new("zellaude:dismiss-tag");
        msg.message_payload = Some(format!("{}:{}", dismiss_kind_str(kind), session_name));
        pipe_message_to_plugin(msg);
    }

    fn load_config(&self) {
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "load_config".into());
        run_command(
            &[
                "sh",
                "-c",
                "cat \"$HOME/.config/zellij/plugins/zellaude.json\" 2>/dev/null || echo '{}'",
            ],
            ctx,
        );
    }

    fn save_config(&self) {
        if !self.config_loaded {
            return;
        }
        self.broadcast_settings();
        let json = serde_json::to_string(&self.settings).unwrap_or_default();
        let json_esc = json.replace('\'', "'\\''");
        let cmd = format!(
            "mkdir -p \"$HOME/.config/zellij/plugins\" && printf '%s' '{json_esc}' > \"$HOME/.config/zellij/plugins/zellaude.json\""
        );
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "save_config".into());
        run_command(&["sh", "-c", &cmd], ctx);
    }

    fn merge_sessions(&mut self, incoming: BTreeMap<u32, SessionInfo>) {
        for (pane_id, mut session) in incoming {
            let dominated = self
                .sessions
                .get(&pane_id)
                .map(|existing| session.last_event_ts > existing.last_event_ts)
                .unwrap_or(true);
            if dominated {
                // Refresh tab name from our local pane map
                if let Some((idx, name)) = self.pane_to_tab.get(&pane_id) {
                    session.tab_index = Some(*idx);
                    session.tab_name = Some(name.clone());
                }
                self.sessions.insert(pane_id, session);
                self.state_dirty = true;
            }
        }
    }

    fn sanitize_session_name(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    fn write_own_state(&mut self) {
        let Some(name) = self.zellij_session_name.clone() else {
            return;
        };
        let safe = Self::sanitize_session_name(&name);
        let payload = RemoteFile {
            session_name: name,
            sessions: self.sessions.clone(),
            wrote_at_ms: unix_now_ms(),
        };
        let Ok(json) = serde_json::to_string(&payload) else {
            return;
        };
        let json_esc = json.replace('\'', "'\\''");
        let cmd = format!(
            "DIR=\"$HOME/.config/zellij/plugins/zellaude-state.d\" && \
             mkdir -p \"$DIR\" && \
             printf '%s' '{json_esc}' > \"$DIR/{safe}.json.tmp.$$\" && \
             mv \"$DIR/{safe}.json.tmp.$$\" \"$DIR/{safe}.json\""
        );
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "write_state".into());
        run_command(&["sh", "-c", &cmd], ctx);
        self.last_write_ms = unix_now_ms();
        self.state_dirty = false;
    }

    fn poll_remote_state(&mut self) {
        // Portable across GNU and BSD shells: avoid `xargs -r`.
        // The `if [ -n "$(ls ...)" ]` guard returns `[]` when the dir is empty
        // or missing, instead of letting jq see a literal `*.json`.
        let cmd = "DIR=\"$HOME/.config/zellij/plugins/zellaude-state.d\"; \
                   { if [ -n \"$(ls \"$DIR\"/*.json 2>/dev/null)\" ]; then \
                       jq -s '.' \"$DIR\"/*.json; \
                     else echo '[]'; fi; } 2>/dev/null";
        let mut ctx = BTreeMap::new();
        ctx.insert("type".into(), "poll_remote".into());
        run_command(&["sh", "-c", cmd], ctx);
        self.last_poll_ms = unix_now_ms();
    }

    fn merge_remote_state(&mut self, raw: &str) {
        let Ok(files) = serde_json::from_str::<Vec<RemoteFile>>(raw.trim()) else {
            return;
        };
        let now_ms = unix_now_ms();
        let own = self.zellij_session_name.as_deref();
        self.remote_sessions.clear();
        for f in files {
            if Some(f.session_name.as_str()) == own {
                continue;
            }
            if f.wrote_at_ms + 30_000 < now_ms {
                continue;
            }
            self.remote_sessions.insert(f.session_name.clone(), f);
        }
        self.reconcile_remote_tags();
    }

    fn reconcile_remote_tags(&mut self) {
        // Drop "dismissed" markers once the remote leaves the matching state
        // (or disappears) — that gates user-click suppression so the next
        // matching event for the same remote re-shows the tag.
        self.remote_tag_dismissed.retain(|(name, kind)| {
            self.remote_sessions
                .get(name)
                .is_some_and(|r| remote_in_state(r, *kind))
        });

        // Compute the current set of (remote, kind) pairs in matching state,
        // and beep on any pair that wasn't in the previous set — i.e. a true
        // transition into Waiting or Done. Done independently of the tag
        // queue so persist-tag mode doesn't suppress beeps on repeat events.
        let mut current_in_state: HashSet<(String, RemoteTagKind)> = HashSet::new();
        for kind in [RemoteTagKind::Waiting, RemoteTagKind::Done] {
            for (name, remote) in &self.remote_sessions {
                if remote_in_state(remote, kind) {
                    current_in_state.insert((name.clone(), kind));
                }
            }
        }
        for key in &current_in_state {
            if !self.remote_in_state_prev.contains(key) {
                self.beep_remote_pending = true;
            }
        }
        self.remote_in_state_prev = current_in_state;

        // Drop entries whose remote no longer exists, or — when persistence
        // is off — whose remote has left the matching state.
        let persist = self.settings.persist_cross_session_tags;
        self.remote_tag_order.retain(|(name, kind)| {
            let Some(remote) = self.remote_sessions.get(name) else {
                return false;
            };
            if persist {
                return true;
            }
            remote_in_state(remote, *kind)
        });

        // Add any newly-matching remote not yet in the queue and not dismissed.
        // Iterate Waiting first so a remote that flips Waiting→Done in one
        // poll cycle gets the more urgent tag enqueued ahead of the other.
        for kind in [RemoteTagKind::Waiting, RemoteTagKind::Done] {
            for (name, remote) in &self.remote_sessions {
                let key = (name.clone(), kind);
                if self.remote_tag_order.iter().any(|entry| entry == &key) {
                    continue;
                }
                if self.remote_tag_dismissed.contains(&key) {
                    continue;
                }
                if remote_in_state(remote, kind) {
                    self.remote_tag_order.push_back(key);
                }
            }
        }
    }
}
