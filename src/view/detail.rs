//! The skill chosen: where it is, what is wrong with it, and its content, files and links.

use rune::{El, ScrollSpec, Tx, col, el, icon, row, text};
use rune_kit::{
    Collapsible, IconButton, IconButtonVariant, Kit, Markdown, Menu, MenuItem, Placement,
    SegmentedControl,
};

use super::{empty, files, icons};
use crate::app::{App, Dialog, Tab};
use crate::skills::{Scope, Severity, Skill};

pub fn view(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let Some(sk) = s.skill() else {
        return empty(
            k,
            "No skill selected",
            "Choose a skill to see its content, files and links.",
        )
        .key("no-skill");
    };
    let links = s
        .snap
        .as_ref()
        .map(|snap| snap.links_to(sk.real_path.as_deref(), &sk.path))
        .unwrap_or_default();
    let body = match s.tab {
        Tab::Preview => preview(s, sk, k),
        Tab::Files => files::view(s, sk, k, layer),
        Tab::Links => links_view(s, sk, &links, k),
    };
    col()
        .key("detail")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(header(s, sk, links.len(), k, layer))
        .child(
            el().key("detail-rule")
                .w_full()
                .h(1.0)
                .fill(k.c.border_muted),
        )
        .child(body.grow(1.0).min_h(0.0))
}

/// The name, where it is, and what is wrong with it: all that stays while the tabs change.
fn header(s: &App, sk: &Skill, links: usize, k: &Kit, layer: &mut Vec<El>) -> El {
    let c = &k.c;
    let labels = [
        "Preview".to_owned(),
        "Files".to_owned(),
        if links > 0 {
            format!("Links {links}")
        } else {
            "Links".to_owned()
        },
    ];
    let tabs = SegmentedControl::new(&labels, Some(s.tab as usize))
        .label("View")
        .fit()
        .on_pick(|s: &mut App, i, _: &mut Tx| s.tab = [Tab::Preview, Tab::Files, Tab::Links][i])
        .build(k)
        .key("tabs");
    let title = row()
        .key("title-row")
        .w_full()
        .gap(10.0)
        .center_y()
        .child(
            text(sk.name.clone())
                .key("skill-name")
                .role("heading")
                .level(2)
                .font(rune::TextStyle::new(18.0, 600.0).lh(24.0))
                .color(c.text_base)
                .truncate()
                .grow(1.0)
                .shrink(),
        )
        .child(tabs)
        .child(actions(s, sk, k, layer));
    let mut head = col()
        .key("detail-header")
        .w_full()
        .px(20.0)
        .pt(14.0)
        .pb(12.0)
        .gap(6.0)
        .child(title)
        .child(place(s, sk, k));
    // A broken link already says so beside its path.
    if !sk.problems.is_empty() && !sk.broken {
        head = head.child(problems(sk, k));
    }
    head
}

/// Where the skill is: its entry, and for a link the folder it leads to. The hops between are
/// in Links.
fn place(s: &App, sk: &Skill, k: &Kit) -> El {
    let c = &k.c;
    let path = |key: &str, shown: String, ink| {
        text(shown)
            .key(key)
            .font(k.ty.mono)
            .color(ink)
            .truncate_middle()
            .shrink()
            .selectable(true)
            .selection_color(k.selection())
    };
    let mut line = row().key("place").w_full().gap(6.0).center_y().child(path(
        "path",
        s.tilde(&sk.path),
        c.text_muted,
    ));
    if sk.broken {
        let target = sk
            .chain
            .last()
            .map(|h| h.target.display().to_string())
            .unwrap_or_default();
        line = line
            .child(
                icon(&icons::BROKEN, 12.0)
                    .color(c.danger.icon)
                    .a11y_hidden(),
            )
            .child(path("target", target, c.danger.fg))
            .child(
                text("missing")
                    .key("missing")
                    .font(k.ty.small)
                    .color(c.danger.fg),
            );
    } else if let Some(real) = sk
        .real_path
        .as_deref()
        .filter(|real| sk.is_link || *real != sk.path)
    {
        line = line
            .child(
                icon(&icons::ARROW, 12.0)
                    .color(c.icon_faint)
                    .label("links to"),
            )
            .child(path("real-path", s.tilde(real), c.text_muted));
    }
    line
}

fn problems(sk: &Skill, k: &Kit) -> El {
    let c = &k.c;
    let worst = if sk.worst() == Some(Severity::Error) {
        &c.danger
    } else {
        &c.warning
    };
    let lines = sk.problems.iter().enumerate().map(|(i, p)| {
        let tone = if p.severity == Severity::Error {
            &c.danger
        } else {
            &c.warning
        };
        row()
            .key(format!("problem{i}"))
            .gap(6.0)
            .items_start()
            .child(
                el().pt(2.0)
                    .child(icon(&icons::ALERT, 12.0).color(tone.icon))
                    .a11y_hidden(),
            )
            .child(
                text(p.message.clone())
                    .font(k.ty.text_xsmall)
                    .color(c.text_base)
                    .wrap()
                    .grow(1.0)
                    .shrink()
                    .selectable(true),
            )
    });
    col()
        .key("problems")
        .role("alert")
        .w_full()
        .mt(4.0)
        .px(10.0)
        .py(7.0)
        .gap(3.0)
        .radius(k.radius.md)
        .fill(worst.bg)
        .border(1.0, worst.border)
        .children(lines)
}

/// Everything that changes the skill, behind one button.
fn actions(s: &App, sk: &Skill, k: &Kit, layer: &mut Vec<El>) -> El {
    let trigger = IconButton::new(&icons::MORE)
        .variant(IconButtonVariant::GhostMuted)
        .label("Actions")
        .help("Rename, link, move or remove")
        .on_press(|s: &mut App| s.menu_open = !s.menu_open)
        .build(k)
        .key("actions");
    let item = |label: &str, act: fn(&mut App, &mut Tx)| {
        MenuItem::new(label.to_owned()).on_select_tx(move |s: &mut App, tx: &mut Tx| {
            s.menu_open = false;
            act(s, tx);
        })
    };
    let mut menu = Menu::new("skill-menu")
        .trigger(trigger)
        .open(s.menu_open)
        .placement(Placement::BottomEnd)
        .on_close(|s: &mut App| s.menu_open = false)
        .item(item("Rename…", |s, _| s.open_rename()))
        .item(item("Link Into…", |s, _| s.open_link_into()).disabled(sk.real_path.is_none()));
    menu = if sk.is_link {
        menu.item(item("Change Target…", |s, _| s.open_retarget()))
            .item(
                item("Make Real Copy…", |s, _| s.dialog = Dialog::Materialize).disabled(sk.broken),
            )
    } else {
        menu.item(item("Move & Link…", |s, _| s.open_convert()))
    };
    menu = menu.separator().item(item("Copy Path", |s, tx| {
        if let Some(p) = s.skill_path.clone() {
            s.copy(&p, tx);
        }
    }));
    if sk.is_link && !sk.broken {
        menu = menu.item(item("Copy Real Path", |s, tx| {
            if let Some(p) = s.skill().and_then(|sk| sk.real_path.clone()) {
                s.copy(&p, tx);
            }
        }));
    }
    menu = menu.item(item(
        if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else {
            "Show in Folder"
        },
        |s, _| {
            // A link shows the folder it leads to; a broken one, the link itself.
            if let Some(p) = s
                .skill()
                .map(|sk| sk.real_path.clone().unwrap_or_else(|| sk.path.clone()))
            {
                s.reveal(&p);
            }
        },
    ));
    let remove = if sk.is_link {
        "Remove Link…"
    } else {
        "Move to Trash…"
    };
    menu.separator()
        .item(item(remove, |s, _| s.dialog = Dialog::DeleteSkill).danger(true))
        .build(k, layer)
}

/// SKILL.md as it reads: what the skill is for, its other settings folded away, then its body.
fn preview(s: &App, sk: &Skill, k: &Kit) -> El {
    let c = &k.c;
    if sk.broken {
        return empty(
            k,
            "The link is broken",
            "Change Target… in the ··· menu points it at a folder.",
        )
        .key("broken-link");
    }
    if !sk.has_skill_md {
        return empty(k, "No SKILL.md", "Files lists what the folder holds.").key("no-skill-md");
    }
    let m = &sk.meta;
    let mut page = col()
        .key("page")
        .w_full()
        .max_w(780.0)
        .px(24.0)
        .pt(18.0)
        .pb(32.0)
        .gap(14.0);
    if !m.description.is_empty() {
        page = page.child(
            text(m.description.clone())
                .key("description")
                .font(k.ty.body)
                .color(c.text_muted)
                .wrap()
                .selectable(true)
                .selection_color(k.selection()),
        );
    }
    // The name and description show above; the rest of the frontmatter waits behind its title.
    let rest: Vec<_> = m
        .fields
        .iter()
        .filter(|f| f.key != "name" && f.key != "description")
        .collect();
    if m.err.is_some() {
        page = page.child(
            text(m.raw.trim_end().to_owned())
                .key("frontmatter-raw")
                .font(k.ty.mono)
                .color(c.text_muted)
                .wrap()
                .selectable(true),
        );
    } else if !rest.is_empty() {
        let title = format!(
            "Frontmatter · {}",
            rest.iter()
                .map(|f| f.key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut table = col()
            .key("fields")
            .w_full()
            .radius(k.radius.md)
            .border(1.0, c.border_muted)
            .fill(c.layer_01);
        for (i, f) in rest.iter().enumerate() {
            let value = text(f.value.clone())
                .font(if f.multiline { k.ty.mono } else { k.ty.ui })
                .color(c.text_base)
                .wrap()
                .grow(1.0)
                .shrink()
                .selectable(true)
                .selection_color(k.selection());
            table = table.child(
                row()
                    .key(format!("field:{}", f.key))
                    .w_full()
                    .gap(16.0)
                    .px(12.0)
                    .py(7.0)
                    .items_start()
                    .border_sides(
                        [if i > 0 { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                        c.border_muted,
                    )
                    .child(
                        text(f.key.clone())
                            .font(k.ty.mono)
                            .color(c.text_muted)
                            .w(132.0),
                    )
                    .child(value),
            );
        }
        let mut fold = Collapsible::new(title, s.front_open)
            .on_toggle(|s: &mut App| s.front_open = !s.front_open);
        if s.front_open {
            fold = fold.child(table);
        }
        page = page.child(fold.build(k).key("frontmatter"));
    }
    if !m.body.trim().is_empty() {
        page = page.child(
            Markdown::new(m.body.clone())
                .selectable(true)
                .on_link_tx(|s: &mut App, url: &str, tx: &mut Tx| s.follow_link(url, tx))
                .build(k)
                .key("markdown"),
        );
    }
    k.scroll_bar(
        col()
            .key("preview")
            .w_full()
            .h_full()
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(page),
    )
}

/// Where a link leads, hop by hop, and the links elsewhere that lead to the same folder.
fn links_view(s: &App, sk: &Skill, links: &[&Skill], k: &Kit) -> El {
    let c = &k.c;
    let heading = |t: &str| {
        text(t.to_owned())
            .role("heading")
            .level(3)
            .font(k.ty.text_xsmall_medium)
            .color(c.text_muted)
    };
    let mut page = col()
        .key("page")
        .w_full()
        .max_w(780.0)
        .px(24.0)
        .pt(18.0)
        .pb(32.0)
        .gap(8.0);
    if sk.is_link {
        page = page.child(heading("Points to"));
        for (i, hop) in sk.chain.iter().enumerate() {
            let last = i + 1 == sk.chain.len();
            let (glyph, ink) = if sk.broken && last {
                (&icons::BROKEN, c.danger.icon)
            } else {
                (&icons::ARROW, c.icon_faint)
            };
            page = page.child(
                row()
                    .key(format!("hop{i}"))
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .pl(i as f32 * 14.0)
                    .child(icon(glyph, 12.0).color(ink).a11y_hidden())
                    .child(
                        text(s.tilde(&hop.target))
                            .font(k.ty.mono)
                            .color(c.text_base)
                            .truncate_middle()
                            .shrink()
                            .selectable(true)
                            .selection_color(k.selection()),
                    ),
            );
        }
        match sk.real_path.as_deref() {
            None => {
                page = page.child(
                    text("The link points to nothing.")
                        .key("dead")
                        .font(k.ty.ui)
                        .color(c.danger.fg),
                )
            }
            Some(real) => {
                // The folder itself, when it is a skill the app lists.
                if let Some(orig) = s.snap.as_ref().and_then(|snap| snap.find(real)) {
                    page = page.child(place_row(s, orig, "The real folder", k).key("real-folder"));
                }
            }
        }
        page = page.child(el().h(8.0));
    }
    if sk.real_path.is_some() {
        page = page.child(heading(if sk.is_link {
            "Other links to the same folder"
        } else {
            "Links to this folder"
        }));
        if links.is_empty() {
            page = page.child(
                text("None in the folders scanned.")
                    .key("no-links")
                    .font(k.ty.ui)
                    .color(c.text_faint),
            );
        }
        for l in links {
            page = page.child(place_row(s, l, "", k));
        }
    }
    k.scroll_bar(
        col()
            .key("links")
            .w_full()
            .h_full()
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(page),
    )
}

/// Another skill, as a row that goes to it.
fn place_row(s: &App, other: &Skill, note: &str, k: &Kit) -> El {
    let c = &k.c;
    let scope = if other.scope == Scope::Global {
        "Global".to_owned()
    } else {
        crate::app::base_name(&other.base)
    };
    let mut caption = format!("{} · {scope}", other.agent.name());
    if !note.is_empty() {
        caption = format!("{note} · {caption}");
    }
    let shown = s.tilde(&other.path);
    let path = other.path.clone();
    let r = row()
        .key(format!("goto:{}", other.path.display()))
        .role("button")
        .label(format!("Show {shown}"))
        .focusable(true)
        .w_full()
        .gap(10.0)
        .px(10.0)
        .py(7.0)
        .center_y()
        .radius(k.radius.md)
        .border(1.0, c.border_muted)
        .child(
            icon(
                if other.is_link {
                    &icons::LINK
                } else {
                    &icons::FOLDER
                },
                14.0,
            )
            .color(c.icon_muted)
            .a11y_hidden(),
        )
        .child(
            col()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(
                    text(shown)
                        .font(k.ty.mono)
                        .color(c.text_base)
                        .truncate_middle(),
                )
                .child(
                    text(caption)
                        .font(k.ty.caption())
                        .color(c.text_faint)
                        .truncate(),
                ),
        )
        .child(
            icon(k.glyph.caret_right, 12.0)
                .color(c.icon_faint)
                .a11y_hidden(),
        )
        .on_press_tx(move |s: &mut App, tx: &mut Tx| s.go_to(&path, tx));
    k.interactive(r)
}
