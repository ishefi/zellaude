use crate::state::{Activity, FlashMode, HookPayload, NotifyMode, SessionInfo, State};
use std::collections::BTreeMap;
use zellij_tile::prelude::*;

pub fn handle_hook_event(state: &mut State, payload: HookPayload) {
    // Capture env info for use in notifications
    if let Some(ref name) = payload.zellij_session {
        state.zellij_session_name = Some(name.clone());
    }
    if let Some(ref tp) = payload.term_program {
        state.term_program = Some(tp.clone());
    }

    let event = payload.hook_event.as_str();

    // SessionEnd → remove session
    if event == "SessionEnd" {
        state.sessions.remove(&payload.pane_id);
        return;
    }

    let activity = match event {
        "SessionStart" => Activity::Init,
        "PreToolUse" => {
            Activity::Tool(payload.tool_name.clone().unwrap_or_default())
        }
        "PostToolUse" => Activity::Thinking,
        "UserPromptSubmit" => Activity::Thinking,
        "PermissionRequest" => Activity::Waiting,
        // Notification is informational — just refresh the timestamp, keep current activity
        "Notification" => {
            if let Some(session) = state.sessions.get_mut(&payload.pane_id) {
                session.last_event_ts = crate::state::unix_now();
            }
            return;
        }
        "Stop" => Activity::Done,
        "SubagentStop" => Activity::AgentDone,
        _ => Activity::Idle,
    };

    let (tab_index, tab_name) = state
        .pane_to_tab
        .get(&payload.pane_id)
        .cloned()
        .unzip();

    let session = state
        .sessions
        .entry(payload.pane_id)
        .or_insert_with(|| SessionInfo {
            session_id: payload.session_id.clone().unwrap_or_default(),
            pane_id: payload.pane_id,
            activity: Activity::Init,
            tab_name: None,
            tab_index: None,
            last_event_ts: 0,
            cwd: None,
        });

    if matches!(activity, Activity::Waiting) {
        match state.settings.flash {
            FlashMode::Once => {
                state.flash_deadlines.insert(
                    payload.pane_id,
                    crate::state::unix_now_ms() + crate::state::FLASH_DURATION_MS,
                );
            }
            FlashMode::Persist => {
                state.flash_deadlines.insert(payload.pane_id, u64::MAX);
            }
            FlashMode::Off => {}
        }
        let should_notify = match state.settings.notifications {
            NotifyMode::Always => true,
            NotifyMode::Unfocused => {
                // Only notify if the pane is on a different tab
                tab_index.map_or(true, |idx| state.active_tab_index != Some(idx))
            }
            NotifyMode::Never => false,
        };
        if should_notify {
            let now = crate::state::unix_now();
            let cooldown_key = tab_index.unwrap_or(usize::MAX) as u32;
            let last = state.last_notify_ts.get(&cooldown_key).copied().unwrap_or(0);
            if now.saturating_sub(last) >= 10 {
                state.last_notify_ts.insert(cooldown_key, now);
                let tab = tab_name.as_deref().unwrap_or("Claude Code");
                let tool = payload.tool_name.as_deref().unwrap_or("");
                let zj_session = state.zellij_session_name.as_deref().unwrap_or("");
                let term = state.term_program.as_deref().unwrap_or("");
                send_notification(tab, tool, payload.pane_id, zj_session, term);
            }
        }
    } else {
        state.flash_deadlines.remove(&payload.pane_id);
    }

    session.activity = activity;
    session.last_event_ts = crate::state::unix_now();
    if let Some(sid) = &payload.session_id {
        session.session_id = sid.clone();
    }
    if let Some(cwd) = payload.cwd {
        session.cwd = Some(cwd);
    }
    if let Some((idx, name)) = tab_index.zip(tab_name) {
        session.tab_index = Some(idx);
        session.tab_name = Some(name);
    }
}

fn send_notification(
    tab_name: &str,
    tool_name: &str,
    pane_id: u32,
    zellij_session: &str,
    term_program: &str,
) {
    let tool_suffix = if tool_name.is_empty() {
        String::new()
    } else {
        format!(" — {tool_name}")
    };
    let title = format!("⚠ {tab_name}");
    let message = format!("Permission requested{tool_suffix}");
    // Escape single quotes for shell
    let title_esc = title.replace('\'', "'\\''");
    let message_esc = message.replace('\'', "'\\''");
    let session_esc = zellij_session.replace('\'', "'\\''");
    let term_esc = term_program.replace('\'', "'\\''");

    // Click callback: activate terminal + pipe a focus request to the plugin
    let activate = if term_program.is_empty() {
        String::new()
    } else {
        format!("open -a '{term_esc}' && ")
    };
    let focus_cmd = format!(
        "{activate}/opt/homebrew/bin/zellij -s '{session_esc}' pipe --name zellaude:focus -- {pane_id}"
    );
    let focus_esc = focus_cmd.replace('\'', "'\\''");

    // terminal-notifier supports click-to-focus; fall back to osascript
    let cmd = format!(
        "if command -v terminal-notifier >/dev/null 2>&1; then \
           terminal-notifier \
             -title '{title_esc}' \
             -message '{message_esc}' \
             -execute '{focus_esc}'; \
         else \
           osascript -e 'display notification \"{message_esc}\" with title \"{title_esc}\"'; \
         fi"
    );
    run_command(&["sh", "-c", &cmd], BTreeMap::new());
}
