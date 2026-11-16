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
    for (i, l) in locations.iter().enumerate() {
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
        // A folder that is a link to one listed above holds the same skills: it folds to its
        // title, unless the skill shown is one of its own.
        let twin = l
            .real_dir
            .as_ref()
            .filter(|_| !l.links.is_empty())
            .and_then(|real| {
                // The folder the link leads to, else a link to it listed above.
                locations.iter().enumerate().find(|(j, o)| {
                    *j != i && o.real_dir.as_ref() == Some(real) && (o.links.is_empty() || *j < i)
                })
            })
            .map(|(_, o)| o);
        let title = rel_dir(l.agent, l.scope);
        let mut section = SidebarSection::new(match twin {
            Some(o) => format!("{title} → {}", rel_dir(o.agent, o.scope)),
            None => title.to_owned(),
        })
        .key(format!("location:{}", l.agent.name()));
        if twin.is_some() {
            let own = s
                .skill_path
                .as_deref()
                .is_some_and(|p| l.skills.iter().any(|sk| sk.path == p));
            let folded = !own && !s.twins_open;
            section = section
                .collapsed(folded)
                .on_toggle(|s: &mut App, _: &mut Tx| s.twins_open = !s.twins_open);
            if folded {
                section = section.count(l.skills.len());
            }
        } else if let Some(hop) = l.links.last() {
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

/// What stands out about a skill: it is a link, something is wrong with it, or another folder
/// has its name. A plain, sound folder shows nothing.
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
    // Links to the one folder are one skill; only another folder of the name competes with it.
    let mut same: Vec<&Skill> = Vec::new();
    for o in s
        .snap
        .as_ref()
        .map(|snap| snap.same(sk, s.scope()))
        .unwrap_or_default()
    {
        let real = o.real_path.as_ref();
        if real.is_none()
            || (real != sk.real_path.as_ref() && same.iter().all(|x| x.real_path.as_ref() != real))
        {
            same.push(o);
        }
    }
    if !same.is_empty() {
        let mut tip = format!("Also named {}:", sk.name);
        for o in &same {
            tip.push_str(&format!("\n{}", s.tilde(&o.path)));
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
