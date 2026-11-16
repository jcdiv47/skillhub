//! States to look at headless: `skillhub --snap out.png --scene files`. Each starts from what
//! the app read at launch (`SKILLHUB_HOME` and `SKILLHUB_CONFIG` point it at a fixture).

use std::path::PathBuf;

use rune::TextEdit;

use crate::app::{App, Dialog, Tab, focused};
use crate::skills::Skill;

/// A named state.
type Scene = (&'static str, fn(&mut App));

pub const SCENES: &[Scene] = &[
    ("skill", |s| pick(s, |_| true)),
    ("link", |s| pick(s, |sk| sk.is_link && !sk.broken)),
    ("problem", |s| {
        pick(s, |sk| !sk.problems.is_empty() && !sk.broken)
    }),
    ("broken", |s| {
        project(s);
        pick(s, |sk| sk.broken);
    }),
    ("project", |s| {
        project(s);
        pick(s, |_| true);
    }),
    ("files", |s| {
        pick(s, |sk| sk.has_skill_md);
        s.tab = Tab::Files;
    }),
    ("edited", |s| {
        pick(s, |sk| sk.has_skill_md);
        s.tab = Tab::Files;
        s.doc.edit.text.push_str("\nMore to say.\n");
    }),
    ("links", |s| {
        pick(s, |sk| sk.is_link && !sk.broken);
        s.tab = Tab::Links;
    }),
    ("frontmatter", |s| {
        pick(s, |sk| sk.meta.fields.len() > 2);
        s.front_open = true;
    }),
    ("menu", |s| {
        pick(s, |sk| sk.is_link && !sk.broken);
        s.menu_open = true;
    }),
    ("new-skill", |s| {
        project(s);
        s.open_new_skill();
        if let Dialog::NewSkill { name, desc, .. } = &mut s.dialog {
            *name = focused("data-viz");
            *desc = TextEdit::new("Draw charts from tables.");
        }
    }),
    ("rename", |s| {
        pick(s, |sk| !sk.is_link);
        s.open_rename();
    }),
    ("link-into", |s| {
        pick(s, |sk| !sk.is_link && sk.has_skill_md);
        s.open_link_into();
        if let Dialog::LinkInto { agents, .. } = &mut s.dialog {
            agents[2] = true;
        }
    }),
    ("retarget", |s| {
        project(s);
        pick(s, |sk| sk.broken);
        s.open_retarget();
    }),
    ("move", |s| {
        project(s);
        pick(s, |sk| !sk.is_link);
        s.open_convert();
    }),
    ("delete", |s| {
        pick(s, |sk| !sk.is_link && sk.has_skill_md);
        s.dialog = Dialog::DeleteSkill;
    }),
    ("folders", |s| s.dialog = Dialog::Folders),
    ("filter", |s| s.skill_query = TextEdit::new("pdf")),
    ("welcome", |s| {
        s.welcome = true;
        s.cfg.roots.clear();
    }),
    ("sidebar-closed", |s| {
        pick(s, |_| true);
        s.sidebar.set_open(false);
    }),
];

/// Shows the first project with skills.
fn project(s: &mut App) {
    let root = s.snap.as_ref().and_then(|snap| {
        snap.projects
            .iter()
            .max_by_key(|p| p.count())
            .map(|p| p.info.root.clone())
    });
    if root.is_some() {
        s.show_scope(root);
    }
}

/// Shows the first skill of the scope that `want` takes, else its first.
fn pick(s: &mut App, want: fn(&Skill) -> bool) {
    let Some(snap) = &s.snap else { return };
    let all: Vec<&Skill> = snap
        .locations(s.scope())
        .iter()
        .flat_map(|l| &l.skills)
        .collect();
    let path: Option<PathBuf> = all
        .iter()
        .find(|sk| want(sk))
        .or(all.first())
        .map(|sk| sk.path.clone());
    if let Some(path) = path {
        s.show_skill(&path);
    }
}
