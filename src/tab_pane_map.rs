use std::collections::HashMap;
use zellij_tile::prelude::*;

/// Build a mapping from terminal pane_id -> (tab_index, tab_name).
/// Uses PaneManifest (keyed by tab_index) cross-referenced with TabInfo list.
pub fn build_pane_to_tab_map(
    tabs: &[TabInfo],
    manifest: &PaneManifest,
) -> HashMap<u32, (usize, String)> {
    let tab_name_by_position: HashMap<usize, String> = tabs
        .iter()
        .map(|t| (t.position, t.name.clone()))
        .collect();

    let mut map = HashMap::new();
    for (&tab_index, panes) in &manifest.panes {
        let tab_name = tab_name_by_position
            .get(&tab_index)
            .cloned()
            .unwrap_or_default();
        for pane in panes {
            if !pane.is_plugin {
                map.insert(pane.id, (tab_index, tab_name.clone()));
            }
        }
    }
    map
}
