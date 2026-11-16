//! The files of a skill, beside the editor of the one open, in a dialog over the window.

use rune::{
    El, Key, KeyEvent, PointerPress, ScrollSpec, TextEdit, Tx, col, el, row, text, text_input,
};
use rune_kit::shell::{Sidebar, SidebarRow};
use rune_kit::syntax::{self, SyntaxTheme};
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, Menu, MenuItem,
    file_icon, glyphs,
};

use super::{empty, icons};
use crate::app::{App, Dialog, Pending};
use crate::skills::File;

/// Past this much text the editor leaves syntax colours out, so typing stays quick.
const MAX_HIGHLIGHT: usize = 200 * 1024;

/// The editor over the window: the files of the skill shown, beside the one open.
pub fn modal(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(sk) = s.skill().filter(|_| s.editing) else {
        return;
    };
    let c = &k.c;
    let close = IconButton::new(k.glyph.close)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .label("Close")
        .on_press(|s: &mut App| s.close_editor())
        .build(k)
        .key("editor-close");
    let head = row()
        .key("editor-head")
        .w_full()
        .pl(16.0)
        .pr(10.0)
        .py(10.0)
        .gap(8.0)
        .center_y()
        .child(
            text(sk.name.clone())
                .font(k.ty.ui_medium)
                .color(c.text_base)
                .truncate()
                .shrink(),
        )
        .child(
            text(s.short(&sk.path))
                .font(k.ty.mono)
                .color(c.text_faint)
                .truncate_middle()
                .shrink(),
        )
        .child(el().grow(1.0))
        .child(close);
    let panel = col()
        .key("editor-dialog")
        .role("dialog")
        .label(format!("Edit {}", sk.name))
        .w_full()
        .max_w(1040.0)
        .h_full()
        .max_h(760.0)
        .radius(8.0)
        .fill(c.layer_01)
        .child(head)
        .child(el().w_full().h(1.0).fill(c.border_muted))
        .child(view(s, k).grow(1.0).min_h(0.0))
        .token_motion(k.motion.overlay);
    let panel = k
        .elevate(panel, &k.elevation.overlay)
        .modal(true, |s: &mut App| s.close_editor());
    layer.push(
        el().key("editor-scrim")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(c.scrim)
            .token_motion(k.motion.overlay)
            .a11y_hidden(),
    );
    layer.push(
        el().key("editor-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(28.0)
            .center()
            .child(panel),
    );
    if let Some((f, at)) = &s.file_menu {
        file_menu(f, *at, k, layer);
    }
}

fn view(s: &App, k: &Kit) -> El {
    // SKILL.md alone needs no list beside it: the editor's bar names it.
    let alone =
        matches!(s.files.as_slice(), [f] if s.doc.path.as_deref() == Some(f.path.as_path()));
    let mut pane = row().key("files").w_full().h_full().min_h(0.0);
    if !alone {
        pane = pane.child(list(s, k).w(208.0).h_full().min_h(0.0)).child(
            el().key("files-divider")
                .w(1.0)
                .h_full()
                .fill(k.c.border_muted),
        );
    }
    pane.child(editor(s, k).grow(1.0).min_w(0.0).h_full().min_h(0.0))
}

fn list(s: &App, k: &Kit) -> El {
    let mut sb = Sidebar::new().label("Files");
    for f in &s.files {
        sb = sb.child(file_row(s, f, k));
    }
    sb.build(k).key("file-list").pt(6.0)
}

fn file_row(s: &App, f: &File, k: &Kit) -> El {
    let name = f.rel.rsplit('/').next().unwrap_or(&f.rel).to_owned();
    let glyph = if f.broken {
        &icons::BROKEN
    } else if f.is_link {
        &icons::LINK
    } else if f.is_dir {
        &glyphs::FOLDER
    } else {
        file_icon(&name)
    };
    let mut r = SidebarRow::new(name.clone())
        .icon(glyph)
        .depth(f.depth.min(12) as u8)
        .selected(s.doc.path.as_deref() == Some(f.path.as_path()));
    if f.is_dir {
        r = r.select_only();
    } else {
        let path = f.path.clone();
        r = r.on_select(move |s: &mut App, tx: &mut Tx| {
            if s.doc.path.as_deref() != Some(path.as_path()) {
                s.guard(Pending::File(path.clone()), tx);
            }
        });
    }
    let menu = f.clone();
    let mut e = r
        .build(k)
        .key(format!("file:{}", f.rel))
        .on_secondary_press(move |s: &mut App, at: PointerPress, _: &mut Tx| {
            s.file_menu = Some((menu.clone(), at))
        });
    if f.is_link {
        e = e.help(format!("{name} → {}", f.target.display()));
    }
    e
}

/// What a right-click on a file offers.
fn file_menu(f: &File, at: PointerPress, k: &Kit, layer: &mut Vec<El>) {
    let (copy, show, gone) = (f.path.clone(), f.path.clone(), f.clone());
    // The menu opens where the press landed, so its trigger is nothing to draw.
    let _trigger = Menu::new("file-menu")
        .open(true)
        .at(at.x, at.y)
        .on_close(|s: &mut App| s.file_menu = None)
        .item(
            MenuItem::new("Copy Path").on_select_tx(move |s: &mut App, tx: &mut Tx| {
                s.file_menu = None;
                s.copy(&copy, tx);
            }),
        )
        .item(
            MenuItem::new(if cfg!(target_os = "macos") {
                "Reveal in Finder"
            } else {
                "Show in Folder"
            })
            .on_select(move |s: &mut App| {
                s.file_menu = None;
                s.reveal(&show);
            }),
        )
        .separator()
        .item(
            MenuItem::new("Delete…")
                .danger(true)
                .disabled(f.rel == "SKILL.md")
                .on_select(move |s: &mut App| {
                    s.file_menu = None;
                    s.dialog = Dialog::DeleteFile(gone.clone());
                }),
        )
        .build(k, layer);
}

fn editor(s: &App, k: &Kit) -> El {
    let c = &k.c;
    let rel = s.doc_rel();
    let dirty = s.dirty();
    let mut bar = row()
        .key("editor-bar")
        .w_full()
        .h(36.0)
        .pl(12.0)
        .pr(6.0)
        .gap(8.0)
        .center_y()
        .child(
            text(rel.clone())
                .key("file-name")
                .font(k.ty.mono)
                .color(c.text_muted)
                .truncate_middle()
                .shrink(),
        );
    if dirty {
        bar = bar.child(
            el().key("edited")
                .size(7.0, 7.0)
                .radius(3.5)
                .fill(c.bg_accent)
                .label("Edited")
                .help("Edited"),
        );
    }
    bar = bar.child(el().grow(1.0));
    if dirty {
        // Saving shows only once there is something to save.
        bar = bar
            .child(
                Button::new("Revert")
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(|s: &mut App| s.revert())
                    .build(k)
                    .key("revert"),
            )
            .child(
                Button::new("Save")
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::Small)
                    .shortcut(if cfg!(target_os = "macos") {
                        "⌘S"
                    } else {
                        "Ctrl+S"
                    })
                    .on_press_tx(|s: &mut App, tx: &mut Tx| s.save(tx))
                    .build(k)
                    .key("save"),
            );
    }
    bar = bar.child(
        IconButton::new(&icons::PLUS)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Small)
            .label("New file")
            .help("New file")
            .on_press_tx(|s: &mut App, tx: &mut Tx| s.open_new_file(tx))
            .build(k)
            .key("new-file"),
    );
    let mut pane = col()
        .key("editor-pane")
        .w_full()
        .child(bar)
        .child(el().w_full().h(1.0).fill(c.border_muted));
    if s.doc.path.is_none() {
        return pane.child(
            empty(k, "No file open", "Choose a file to edit.")
                .key("no-file")
                .grow(1.0),
        );
    }
    if s.doc.changed {
        pane = pane.child(
            row()
                .key("changed")
                .role("alert")
                .w_full()
                .px(12.0)
                .py(6.0)
                .gap(8.0)
                .center_y()
                .fill(c.warning.bg)
                .child(
                    text("The file changed on disk.")
                        .font(k.ty.text_xsmall)
                        .color(c.text_base)
                        .grow(1.0),
                )
                .child(
                    Button::new("Load From Disk")
                        .control_size(ControlSize::Small)
                        .on_press(|s: &mut App| s.revert())
                        .build(k)
                        .key("load-from-disk"),
                ),
        );
    }
    if !s.doc.editable {
        return pane.child(empty(k, &s.doc.note, "").key("not-editable").grow(1.0));
    }
    let code = &s.doc.edit.text;
    let spans = match syntax::lang_for_path(&rel) {
        Some(lang) if code.len() <= MAX_HIGHLIGHT => {
            SyntaxTheme::from_kit(k).spans(&syntax::highlight(Some(lang), code), 0)
        }
        _ => Vec::new(),
    };
    let unit = indent_unit(&rel);
    let field = text_input(&s.doc.edit, k.ty.mono)
        .key("editor")
        .label(format!("Edit {rel}"))
        .w_full()
        .min_h(240.0)
        .color(c.text_base)
        .caret(c.text_base, k.selection())
        .spans(spans)
        .on_edit(|s: &mut App| &mut s.doc.edit)
        // Return breaks the line, keeping its indent; a field would submit on it instead.
        .on_submit(|s: &mut App, _: &mut Tx| {
            let break_ = line_break(&s.doc.edit);
            s.doc.edit.insert(&break_);
        })
        // Tab indents, as in an editor; Shift-Tab still leaves the field.
        .on_input_key(move |s: &mut App, ev: &KeyEvent, _: &mut Tx| {
            let plain = !(ev.mods.shift || ev.mods.cmd || ev.mods.alt || ev.mods.ctrl);
            if ev.key == Key::Tab && plain {
                s.doc.edit.insert(unit);
                return true;
            }
            false
        });
    let viewport = col()
        .key("editor-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().w_full().px(14.0).py(10.0).child(field));
    pane.child(k.scroll_bar(viewport).grow(1.0).min_h(0.0))
}

/// What Return types at the caret: a line break, then the indent the line has before it.
fn line_break(edit: &TextEdit) -> String {
    let (at, _) = edit.range();
    let before = &edit.text[..at];
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    format!("\n{}", &line[..indent])
}

/// What Tab types in a file: a tab where the language asks for one, else spaces.
fn indent_unit(rel: &str) -> &'static str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rsplit_once('.').map(|(_, ext)| ext) {
        _ if name == "Makefile" => "\t",
        Some("go" | "mk" | "tsv") => "\t",
        Some("py" | "rs" | "swift" | "java" | "kt" | "c" | "h" | "cpp") => "    ",
        _ => "  ",
    }
}
