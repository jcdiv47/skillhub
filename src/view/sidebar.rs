//! The scopes: the global skills, then every project found.

use std::collections::HashMap;

use rune::{El, Tx};
use rune_kit::shell::{Sidebar, SidebarHeader, SidebarRow};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit, TextInput};

use super::icons;
use crate::app::{App, Dialog};

/// Projects past this many get a filter above them.
const FILTER_FROM: usize = 9;

pub fn view(s: &App, k: &Kit) -> El {
    let Some(snap) = &s.snap else {
        return Sidebar::new().label("Scopes").build(k);
    };
    let mut sb = Sidebar::new().label("Scopes");
    if snap.projects.len() >= FILTER_FROM {
        sb = sb.header(
            TextInput::new(&s.proj_query)
                .placeholder("Filter projects")
                .leading_icon(k.glyph.search)
                .width(None)
                .on_edit(|s: &mut App| &mut s.proj_query)
                .build(k)
                .key("project-filter"),
        );
    }
    sb = sb.child(
        SidebarRow::new("Global")
            .icon(&icons::GLOBE)
            .detail(snap.global_count().to_string())
            .selected(s.scope.is_none())
            .on_select(|s: &mut App, tx: &mut Tx| s.select_scope(None, tx))
            .build(k)
            .key("scope-global")
            .help("Skills in your home folder: every project sees them"),
    );
    let projects = s.visible_projects();
    sb = sb.child(
        SidebarHeader::new("Projects")
            .count(snap.projects.len())
            .build(k)
            .key("projects-header"),
    );
    // Projects of one name say which folder each is in.
    let mut names: HashMap<&str, usize> = HashMap::new();
    for p in &snap.projects {
        *names.entry(p.info.name.as_str()).or_default() += 1;
    }
    for p in &projects {
        let root = p.info.root.clone();
        let mut r = SidebarRow::new(p.info.name.clone())
            .detail(p.count().to_string())
            .selected(s.scope() == Some(root.as_path()))
            .on_select({
                let root = root.clone();
                move |s: &mut App, tx: &mut Tx| s.select_scope(Some(root.clone()), tx)
            });
        if names.get(p.info.name.as_str()).is_some_and(|n| *n > 1) {
            let parent = p.info.root.parent().map(|d| s.tilde(d)).unwrap_or_default();
            r = r.subtitle(parent);
        }
        sb = sb.child(
            r.build(k)
                .key(format!("project:{}", root.display()))
                .help(s.tilde(&root)),
        );
    }
    if projects.is_empty() {
        let note = if snap.projects.is_empty() {
            "No projects with skills found."
        } else {
            "No projects match."
        };
        sb = sb.child(
            rune::text(note)
                .key("no-projects")
                .font(k.ty.small)
                .color(k.c.text_faint)
                .wrap()
                .px(8.0)
                .py(6.0),
        );
    }
    let rescan = IconButton::new(&icons::REFRESH)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .label("Rescan")
        .help("Look for projects again")
        .disabled(s.scanning)
        .on_press(|s: &mut App| s.scan())
        .build(k)
        .key("rescan");
    sb.footer(
        SidebarRow::new("Folders…")
            .icon(&icons::FOLDERS)
            .trailing(rescan)
            .select_only()
            .on_select(|s: &mut App, _: &mut Tx| s.dialog = Dialog::Folders)
            .build(k)
            .key("folders")
            .help("Choose the folders to scan for projects"),
    )
    .build(k)
}
