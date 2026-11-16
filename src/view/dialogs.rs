//! The dialogs: one at a time, each asking only what its action needs.

use std::path::PathBuf;

use rune::{El, TextEdit, Tx, col, el, icon, row, text};
use rune_kit::{
    Button, ButtonVariant, Checkbox, ControlSize, IconButton, IconButtonVariant, Kit,
    SegmentedControl, Select, SelectAppearance, TextArea, TextInput,
};

use super::icons;
use super::list::location_note;
use crate::app::{App, Dialog, base_name, plural};
use crate::skills::{self, AGENTS, Scope};

const DEPTHS: [&str; 7] = ["1", "2", "3", "4", "5", "6", "8"];

pub fn build(s: &App, k: &Kit, layer: &mut Vec<El>) {
    match &s.dialog {
        Dialog::None => {}
        Dialog::Folders => {
            let actions = buttons(vec![
                cancel(k),
                primary(k, "Save & Rescan", false, |s, _| s.save_folders()),
            ]);
            let body = vec![
                note(
                    k,
                    "SkillHub looks for projects with skills in these folders.",
                ),
                roots_editor(s, k, layer),
                actions,
            ];
            sheet(k, layer, "Folders to Scan", body);
        }
        Dialog::NewSkill { agent, name, desc } => new_skill(s, *agent, name, desc, k, layer),
        Dialog::Rename { name, meta, relink } => rename(s, name, *meta, *relink, k, layer),
        Dialog::LinkInto {
            scope,
            agents,
            name,
        } => link_into(s, scope.as_ref(), agents, name, k, layer),
        Dialog::Retarget { path } => retarget(s, path, k, layer),
        Dialog::Convert { dest } => convert(s, dest, k, layer),
        Dialog::Materialize => {
            let Some(sk) = s.skill() else { return };
            let real = sk
                .real_path
                .as_deref()
                .map(|p| s.tilde(p))
                .unwrap_or_default();
            let said = format!(
                "Copies {real} to {}. Later changes to either are no longer shared.",
                s.tilde(&sk.path)
            );
            let actions = buttons(vec![
                cancel(k),
                primary(k, "Make Copy", false, |s, tx| s.materialize(tx)),
            ]);
            sheet(
                k,
                layer,
                "Replace the link with a copy?",
                vec![note(k, &said), actions],
            );
        }
        Dialog::DeleteSkill => {
            let Some((title, said, button)) = s.delete_summary() else {
                return;
            };
            let actions = buttons(vec![
                cancel(k),
                danger(k, button, |s, tx| s.delete_skill(tx)),
            ]);
            sheet(k, layer, &title, vec![note(k, &said), actions]);
        }
        Dialog::NewFile { rel } => {
            let field = input(
                k,
                rel,
                "File path",
                "references/guide.md",
                |s| match &mut s.dialog {
                    Dialog::NewFile { rel } => rel,
                    _ => &mut s.scratch,
                },
            )
            .on_submit(|s: &mut App, _: &mut Tx| s.create_file())
            .build(k)
            .key("file-path");
            let actions = buttons(vec![
                cancel(k),
                primary(k, "Create", rel.text.trim().is_empty(), |s, _| {
                    s.create_file()
                }),
            ]);
            let body = vec![
                field,
                hint(
                    k,
                    "A path inside the skill's folder; folders on the way are created.",
                ),
                actions,
            ];
            sheet(k, layer, "New File", body);
        }
        Dialog::DeleteFile(f) => {
            let (title, said, button) = App::delete_file_summary(f);
            let mut body = Vec::new();
            if !said.is_empty() {
                body.push(note(k, &said));
            }
            body.push(buttons(vec![
                cancel(k),
                danger(k, button, |s, tx| s.remove_file(tx)),
            ]));
            sheet(k, layer, &title, body);
        }
        Dialog::Discard => {
            let name = s.doc.path.as_deref().map(base_name).unwrap_or_default();
            let actions = buttons(vec![
                cancel(k),
                Button::new("Discard")
                    .control_size(ControlSize::Regular)
                    .on_press_tx(|s: &mut App, tx: &mut Tx| s.discard(false, tx))
                    .build(k)
                    .key("discard"),
                primary(k, "Save", false, |s, tx| s.discard(true, tx)),
            ]);
            sheet(
                k,
                layer,
                &format!("Save changes to {name}?"),
                vec![
                    note(k, "Your edits will be lost if you don't save them."),
                    actions,
                ],
            );
        }
        Dialog::Error { title, text } => {
            let actions = buttons(vec![primary(k, "OK", false, |s, _| s.close_dialog())]);
            sheet(k, layer, title, vec![note(k, text), actions]);
        }
    }
}

fn new_skill(
    s: &App,
    agent: skills::Agent,
    name: &TextEdit,
    desc: &TextEdit,
    k: &Kit,
    layer: &mut Vec<El>,
) {
    let scope = if s.scope.is_some() {
        Scope::Project
    } else {
        Scope::Global
    };
    let dirs: Vec<&str> = AGENTS.iter().map(|a| skills::rel_dir(*a, scope)).collect();
    let folder = SegmentedControl::new(&dirs, AGENTS.iter().position(|a| *a == agent))
        .label("Folder")
        .on_pick(|s: &mut App, i, _: &mut Tx| {
            if let Dialog::NewSkill { agent, .. } = &mut s.dialog {
                *agent = AGENTS[i];
            }
        })
        .build(k)
        .key("new-skill-folder");
    let name_field = input(k, name, "Name", "my-skill", |s| match &mut s.dialog {
        Dialog::NewSkill { name, .. } => name,
        _ => &mut s.scratch,
    })
    .on_submit(|s: &mut App, tx: &mut Tx| s.create_skill(tx))
    .build(k)
    .key("new-skill-name");
    let desc_field = TextArea::new(desc)
        .label("Description")
        .placeholder("What the skill does and when to use it.")
        .width(None)
        .rows(3)
        .on_edit(|s: &mut App| match &mut s.dialog {
            Dialog::NewSkill { desc, .. } => desc,
            _ => &mut s.scratch,
        })
        .build(k)
        .key("new-skill-description");
    let typed = name.text.trim();
    let err = s.new_skill_error(agent, typed);
    let mut body = vec![
        field(k, "Folder", folder),
        field(k, "Name", name_field),
        field(k, "Description", desc_field),
    ];
    if !typed.is_empty() {
        let at = s.location_dir(agent).join(typed);
        body.push(mono(k, &format!("Creates {}/SKILL.md", s.tilde(&at))).key("creates"));
        body.extend(err.as_deref().map(|e| problem(k, e)));
    }
    body.push(buttons(vec![
        cancel(k),
        primary(k, "Create", err.is_some(), |s, tx| s.create_skill(tx)),
    ]));
    let scope_name = s.scope().map(base_name).unwrap_or_else(|| "Global".into());
    sheet(k, layer, &format!("New Skill in {scope_name}"), body);
}

fn rename(s: &App, name: &TextEdit, meta: bool, relink: bool, k: &Kit, layer: &mut Vec<El>) {
    let (Some(sk), Some(snap)) = (s.skill(), &s.snap) else {
        return;
    };
    let name_field = input(k, name, "New name", "", |s| match &mut s.dialog {
        Dialog::Rename { name, .. } => name,
        _ => &mut s.scratch,
    })
    .on_submit(|s: &mut App, tx: &mut Tx| s.rename_skill(tx))
    .build(k)
    .key("rename-name");
    let mut body = vec![name_field];
    if sk.is_link {
        body.push(hint(
            k,
            "Renames the link only; the folder it points to keeps its name.",
        ));
    } else {
        if sk.has_skill_md && sk.meta.name == sk.name {
            let toggle = |s: &mut App| {
                if let Dialog::Rename { meta, .. } = &mut s.dialog {
                    *meta = !*meta;
                }
            };
            body.push(
                Checkbox::new(meta)
                    .label("Also change name: in SKILL.md")
                    .on_toggle(toggle)
                    .build(k)
                    .key("rename-meta"),
            );
        }
        let links = snap.links_to(sk.real_path.as_deref(), &sk.path).len();
        if links > 0 {
            let toggle = |s: &mut App| {
                if let Dialog::Rename { relink, .. } = &mut s.dialog {
                    *relink = !*relink;
                }
            };
            let label = format!(
                "Point the {} to this folder at its new name",
                plural(links, "link", "links")
            );
            body.push(
                Checkbox::new(relink)
                    .label(label)
                    .on_toggle(toggle)
                    .build(k)
                    .key("rename-relink"),
            );
        }
    }
    let typed = name.text.trim();
    let err = s.rename_error(typed);
    if let Some(e) = err
        .as_deref()
        .filter(|_| !typed.is_empty() && typed != sk.name)
    {
        body.push(problem(k, e));
    }
    body.push(buttons(vec![
        cancel(k),
        primary(k, "Rename", err.is_some(), |s, tx| s.rename_skill(tx)),
    ]));
    sheet(k, layer, &format!("Rename {}", sk.name), body);
}

/// The scopes a skill can be linked into: global, then every project.
fn scope_choices(s: &App) -> Vec<(String, Option<PathBuf>)> {
    let mut out = vec![("Global (~)".to_owned(), None)];
    for p in s.snap.iter().flat_map(|snap| &snap.projects) {
        out.push((
            format!("{} — {}", p.info.name, s.tilde(&p.info.root)),
            Some(p.info.root.clone()),
        ));
    }
    out
}

fn link_into(
    s: &App,
    scope: Option<&PathBuf>,
    agents: &[bool; 3],
    name: &TextEdit,
    k: &Kit,
    layer: &mut Vec<El>,
) {
    let Some(sk) = s.skill() else { return };
    let choices = scope_choices(s);
    let labels: Vec<&str> = choices.iter().map(|(l, _)| l.as_str()).collect();
    let current = choices
        .iter()
        .find(|(_, root)| root.as_ref() == scope)
        .map(|(l, _)| l.as_str());
    let scope_select = Select::new("link-scope", &labels)
        .label("Scope")
        .appearance(SelectAppearance::Field)
        .current(current)
        .width(488.0)
        .open(s.select_open)
        .on_toggle(|s: &mut App| s.select_open = !s.select_open)
        .on_close(|s: &mut App| s.select_open = false)
        .on_choose(|s: &mut App, picked: &str, _: &mut Tx| {
            let root = scope_choices(s)
                .into_iter()
                .find(|(l, _)| l == picked)
                .and_then(|(_, root)| root);
            s.select_open = false;
            if let Dialog::LinkInto { scope, agents, .. } = &mut s.dialog {
                (*scope, *agents) = (root, [false; 3]);
            }
        })
        .build(k, layer);
    let name_field = input(k, name, "Link name", "", |s| match &mut s.dialog {
        Dialog::LinkInto { name, .. } => name,
        _ => &mut s.scratch,
    })
    .build(k)
    .key("link-name");
    let typed = name.text.trim();
    let mut any = false;
    let mut boxes = col().key("link-folders").w_full().gap(6.0);
    for (i, l) in s
        .link_locations(scope.map(|p| p.as_path()))
        .iter()
        .enumerate()
    {
        let blocked = s.link_blocker(l, typed);
        let label = match blocked {
            Some("exists") => format!("{} — {typed} exists", l.label()),
            Some(why) => format!("{} — {why}", l.label()),
            None => format!("{}{}", l.label(), location_note(l)),
        };
        let checked = agents[i] && blocked.is_none();
        any |= checked;
        let toggle = move |s: &mut App| {
            if let Dialog::LinkInto { agents, .. } = &mut s.dialog {
                agents[i] = !agents[i];
            }
        };
        boxes = boxes.child(
            Checkbox::new(checked)
                .label(label)
                .disabled(blocked.is_some())
                .on_toggle(toggle)
                .build(k)
                .key(format!("link-into:{}", l.agent.name())),
        );
    }
    let real = sk
        .real_path
        .as_deref()
        .map(|p| s.tilde(p))
        .unwrap_or_default();
    let err = skills::check_name(typed).err();
    let mut body = vec![
        mono(k, &format!("Creates links to {real}")),
        field(k, "Scope", scope_select),
        field(k, "Name", name_field),
        boxes,
    ];
    body.extend(
        err.as_deref()
            .filter(|_| !typed.is_empty())
            .map(|e| problem(k, e)),
    );
    body.push(buttons(vec![
        cancel(k),
        primary(k, "Create Links", err.is_some() || !any, |s, tx| {
            s.create_links(tx)
        }),
    ]));
    sheet(k, layer, &format!("Link {} Into…", sk.name), body);
}

fn retarget(s: &App, path: &TextEdit, k: &Kit, layer: &mut Vec<El>) {
    let Some(sk) = s.skill() else { return };
    let field = input(
        k,
        path,
        "Target folder",
        "~/.agents/skills/…",
        |s| match &mut s.dialog {
            Dialog::Retarget { path } => path,
            _ => &mut s.scratch,
        },
    )
    .on_submit(|s: &mut App, tx: &mut Tx| s.retarget(tx))
    .build(k)
    .key("target");
    let choose = Button::new("Choose…")
        .control_size(ControlSize::Regular)
        .on_press_tx(|s: &mut App, tx: &mut Tx| s.pick_target(tx))
        .build(k)
        .key("choose-target");
    let checked = skills::check_target(&skills::clean(&s.expand(&path.text)));
    let now = format!(
        "{} → {}",
        s.tilde(&sk.path),
        sk.target()
            .map(|t| t.display().to_string())
            .unwrap_or_default()
    );
    let mut body = vec![
        mono(k, &now),
        row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(field.grow(1.0))
            .child(choose),
    ];
    match &checked {
        Err(e) => body.push(problem(k, e)),
        Ok(Some(warning)) => body.push(
            text(*warning)
                .key("warning")
                .font(k.ty.text_xsmall)
                .color(k.c.warning.fg),
        ),
        Ok(None) => {}
    }
    body.push(buttons(vec![
        cancel(k),
        primary(k, "Change", checked.is_err(), |s, tx| s.retarget(tx)),
    ]));
    sheet(k, layer, "Change Link Target", body);
}

fn convert(s: &App, dest: &PathBuf, k: &Kit, layer: &mut Vec<El>) {
    let Some(sk) = s.skill() else { return };
    let dests = s.convert_destinations();
    let mut labels: Vec<String> = dests.iter().map(|d| s.tilde(d)).collect();
    if !dests.contains(dest) {
        labels.push(s.tilde(dest));
    }
    let current = s.tilde(dest);
    let select = Select::new("move-to", &labels)
        .label("Move to")
        .appearance(SelectAppearance::Field)
        .current(Some(&current))
        .width(400.0)
        .open(s.select_open)
        .on_toggle(|s: &mut App| s.select_open = !s.select_open)
        .on_close(|s: &mut App| s.select_open = false)
        .on_choose(|s: &mut App, picked: &str, _: &mut Tx| {
            let chosen = s
                .convert_destinations()
                .into_iter()
                .find(|d| s.tilde(d) == picked);
            s.select_open = false;
            if let (Dialog::Convert { dest }, Some(chosen)) = (&mut s.dialog, chosen) {
                *dest = chosen;
            }
        })
        .build(k, layer);
    let other = Button::new("Other…")
        .control_size(ControlSize::Regular)
        .on_press_tx(|s: &mut App, tx: &mut Tx| s.pick_convert_dest(tx))
        .build(k)
        .key("other-folder");
    let (to, err) = s.convert_plan(dest).unwrap_or_default();
    let mut body = vec![
        note(
            k,
            "Moves the folder to a shared place and leaves a link to it here, so other projects and agents can link to the same skill.",
        ),
        field(
            k,
            "Move to",
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(select)
                .child(other),
        ),
        mono(
            k,
            &format!("{}  ←  link at {}", s.tilde(&to), s.tilde(&sk.path)),
        )
        .key("plan"),
    ];
    body.extend(err.as_deref().map(|e| problem(k, e)));
    body.push(buttons(vec![
        cancel(k),
        primary(k, "Move & Link", err.is_some(), |s, tx| s.convert(tx)),
    ]));
    sheet(k, layer, &format!("Move & Link {}", sk.name), body);
}

/// The folders to scan, with buttons to add and remove, and how deep to look.
pub(super) fn roots_editor(s: &App, k: &Kit, layer: &mut Vec<El>) -> El {
    let c = &k.c;
    let mut list = col().key("roots").w_full().gap(6.0);
    if s.cfg.roots.is_empty() {
        list = list.child(
            text("No folders chosen yet.")
                .key("no-roots")
                .font(k.ty.ui)
                .color(c.text_faint),
        );
    }
    for root in &s.cfg.roots {
        let shown = s.tilde(root);
        let gone = root.clone();
        let remove = IconButton::new(k.glyph.close)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Small)
            .label(format!("Remove {shown}"))
            .on_press(move |s: &mut App| s.cfg.remove_root(&gone))
            .build(k);
        list = list.child(
            row()
                .key(format!("root:{}", root.display()))
                .w_full()
                .h(32.0)
                .pl(10.0)
                .pr(6.0)
                .gap(8.0)
                .center_y()
                .radius(k.radius.md)
                .fill(c.layer_02)
                .child(icon(&icons::FOLDER, 14.0).color(c.icon_muted).a11y_hidden())
                .child(
                    text(shown)
                        .font(k.ty.ui)
                        .color(c.text_base)
                        .truncate_middle()
                        .grow(1.0)
                        .shrink(),
                )
                .child(remove),
        );
    }
    let depth = s.cfg.depth.to_string();
    let depth_select = Select::new("depth", &DEPTHS)
        .label("Depth")
        .appearance(SelectAppearance::Field)
        .current(Some(&depth))
        .width(64.0)
        .open(s.select_open)
        .on_toggle(|s: &mut App| s.select_open = !s.select_open)
        .on_close(|s: &mut App| s.select_open = false)
        .on_choose(|s: &mut App, picked: &str, _: &mut Tx| {
            s.select_open = false;
            s.cfg.depth = picked.parse().unwrap_or(s.cfg.depth);
        })
        .build(k, layer);
    let add = Button::new("Add Folder…")
        .control_size(ControlSize::Regular)
        .on_press_tx(|s: &mut App, tx: &mut Tx| s.pick_root(tx))
        .build(k)
        .key("add-folder");
    let controls = row()
        .key("roots-controls")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(add)
        .child(el().grow(1.0))
        .child(
            text("Levels deep")
                .font(k.ty.ui)
                .color(c.text_muted)
                .help("How many folders below each one to look for projects"),
        )
        .child(depth_select);
    col()
        .key("roots-editor")
        .w_full()
        .gap(10.0)
        .child(list)
        .child(controls)
}

// The parts dialogs are made of.

fn sheet(k: &Kit, layer: &mut Vec<El>, title: &str, body: Vec<El>) {
    let body = col()
        .key("dialog-body")
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .children(body);
    rune_kit::Dialog::new("dialog", title)
        .open(true)
        .width(520.0)
        .fit()
        .on_close(|s: &mut App| s.close_dialog())
        .child(body)
        .build(k, layer);
}

fn note(k: &Kit, said: &str) -> El {
    text(said.to_owned())
        .key("message")
        .font(k.ty.ui)
        .color(k.c.text_muted)
        .wrap()
        .selectable(true)
}

fn hint(k: &Kit, said: &str) -> El {
    text(said.to_owned())
        .font(k.ty.text_xsmall)
        .color(k.c.text_faint)
        .wrap()
}

fn mono(k: &Kit, said: &str) -> El {
    text(said.to_owned())
        .font(k.ty.mono)
        .color(k.c.text_muted)
        .wrap_anywhere()
}

fn problem(k: &Kit, said: &str) -> El {
    text(said.to_owned())
        .key("problem")
        .role("alert")
        .font(k.ty.text_xsmall)
        .color(k.c.danger.fg)
        .wrap()
}

fn field(k: &Kit, label: &str, control: El) -> El {
    col()
        .w_full()
        .gap(4.0)
        .child(
            text(label.to_owned())
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_muted)
                .a11y_hidden(),
        )
        .child(control)
}

fn input(
    k: &Kit,
    edit: &TextEdit,
    label: &str,
    placeholder: &str,
    access: fn(&mut App) -> &mut TextEdit,
) -> TextInput {
    let _ = k;
    TextInput::new(edit)
        .label(label)
        .placeholder(placeholder)
        .width(None)
        .on_edit(access)
}

fn buttons(items: Vec<El>) -> El {
    row()
        .key("dialog-buttons")
        .w_full()
        .gap(8.0)
        .pt(4.0)
        .justify_end()
        .children(items)
}

fn cancel(k: &Kit) -> El {
    Button::new("Cancel")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .on_press(|s: &mut App| s.close_dialog())
        .build(k)
        .key("cancel")
}

/// The button Return presses.
fn primary(k: &Kit, label: &str, disabled: bool, act: fn(&mut App, &mut Tx)) -> El {
    let b = Button::new(label)
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Regular)
        .disabled(disabled)
        .on_press_tx(act)
        .build(k)
        .key("confirm");
    if disabled { b } else { b.default_button() }
}

fn danger(k: &Kit, label: &str, act: fn(&mut App, &mut Tx)) -> El {
    Button::new(label)
        .variant(ButtonVariant::Danger)
        .control_size(ControlSize::Regular)
        .on_press_tx(act)
        .build(k)
        .key("confirm")
        .default_button()
}
