//! The skills of the scope shown, by the folder they are in.

use rune::{El, Tx, icon, row, text};
use rune_kit::shell::{Sidebar, SidebarRow, SidebarSection};
use rune_kit::{Kit, TextInput};

use super::{empty, icons};
use crate::app::App;
use crate::skills::{Location, Severity, Skill, rel_dir};

pub fn view(s: &App, k: &Kit) -> El {
    let Some(snap) = &s.snap else {
        return empty(k, "Scanning…", "");
    };
    let locations = snap.locations(s.scope());
    let total: usize = locations.iter().map(|l| l.skills.len()).sum();
    let mut sb = Sidebar::new()
        .label("Skills")
        .header(
            TextInput::new(&s.skill_query)
                .placeholder("Filter skills")
                .leading_icon(k.glyph.search)
                .width(None)
                .on_edit(|s: &mut App| &mut s.skill_query)
                .build(k)
                .key("skill-filter"),
        )
        .header_padding(8.0, 6.0);
    let mut shown = 0;
    for l in locations {
        let rows: Vec<El> = l
            .skills
            .iter()
            .filter(|sk| s.matches(sk))
            .map(|sk| skill_row(s, sk, k))
            .collect();
        shown += rows.len();
        // A folder with nothing to show is left out; New Skill names every one of them.
        if rows.is_empty() && l.err.is_none() {
            continue;
        }
        let mut section = SidebarSection::new(rel_dir(l.agent, l.scope))
            .key(format!("location:{}", l.agent.name()))
            .count(l.skills.len());
        if let Some(hop) = l.links.last() {
            let tip = format!(
                "A linked folder: {} → {}",
                s.tilde(&hop.link),
                hop.target.display()
            );
            section = section.action(
                icon(&icons::LINK, 12.0)
                    .color(k.c.icon_faint)
                    .label("Linked folder")
                    .help(tip),
            );
        }
        if let Some(err) = &l.err {
            section = section.row(
                text(err.clone())
                    .font(k.ty.small)
                    .color(k.c.danger.fg)
                    .wrap()
                    .px(8.0)
                    .py(4.0),
            );
        }
        sb = sb.section(section.rows(rows));
    }
    if shown == 0 && locations.iter().all(|l| l.err.is_none()) {
        let (title, hint) = if total == 0 {
            ("No skills here yet", "New Skill makes the first one.")
        } else {
            ("No skills match", "")
        };
        sb = sb.child(empty(k, title, hint).h(160.0).key("no-skills"));
    }
    sb.build(k).key("skills")
}

/// One skill: its name over its description, and what stands out about it.
fn skill_row(s: &App, sk: &Skill, k: &Kit) -> El {
    let path = sk.path.clone();
    let mut r = SidebarRow::new(sk.name.clone())
        .selected(s.skill_path.as_deref() == Some(sk.path.as_path()))
        .on_select(move |s: &mut App, tx: &mut Tx| s.select_skill(&path, tx));
    let subtitle = if sk.broken {
        format!(
            "→ {} · missing",
            sk.target().map(|t| s.tilde(t)).unwrap_or_default()
        )
    } else {
        sk.meta
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if !subtitle.is_empty() {
        r = r.subtitle(subtitle);
    }
    if let Some(marks) = marks(s, sk, k) {
        r = r.trailing(marks);
    }
    r.build(k).key(format!("skill:{}", sk.path.display()))
}

/// What stands out about a skill: it is a link, something is wrong with it, or others share
/// its name. A plain, sound folder shows nothing.
fn marks(s: &App, sk: &Skill, k: &Kit) -> Option<El> {
    let c = &k.c;
    let mut out = Vec::new();
    if let Some(worst) = sk.worst().filter(|_| !sk.broken) {
        let color = if worst == Severity::Error {
            c.danger.icon
        } else {
            c.warning.icon
        };
        let said = sk
            .problems
            .iter()
            .map(|p| p.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        out.push(
            icon(&icons::ALERT, 13.0)
                .key("problems")
                .color(color)
                .label(crate::app::plural(sk.problems.len(), "problem", "problems"))
                .help(said),
        );
    }
    if sk.broken {
        out.push(
            icon(&icons::BROKEN, 13.0)
                .key("broken")
                .color(c.danger.icon)
                .label("Broken link")
                .help("The link points to nothing"),
        );
    } else if sk.is_link {
        let to = sk
            .real_path
            .as_deref()
            .map(|p| s.tilde(p))
            .unwrap_or_default();
        out.push(
            icon(&icons::LINK, 13.0)
                .key("link")
                .color(c.icon_faint)
                .label("Link")
                .help(format!("A link to {to}")),
        );
    }
    let same = s
        .snap
        .as_ref()
        .map(|snap| snap.same(sk, s.scope()))
        .unwrap_or_default();
    if !same.is_empty() {
        let mut tip = format!("Also named {}:", sk.name);
        for o in &same {
            let note = if o.real_path.is_some() && o.real_path == sk.real_path {
                " (same folder)"
            } else {
                ""
            };
            tip.push_str(&format!("\n{}{note}", s.tilde(&o.path)));
        }
        out.push(
            text(format!("×{}", same.len() + 1))
                .key("same")
                .font(k.ty.small)
                .color(c.text_faint)
                .help(tip),
        );
    }
    (!out.is_empty()).then(|| row().key("marks").gap(5.0).center_y().children(out))
}

/// The folders of a scope a skill can be made in, as New Skill and Link Into list them.
pub(crate) fn location_note(l: &Location) -> &'static str {
    if l.exists { "" } else { " — will be created" }
}
