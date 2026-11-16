//! The window: projects, the skills of the one chosen, and the skill chosen among them.

mod detail;
mod dialogs;
mod files;
pub mod icons;
mod list;
mod sidebar;
mod welcome;

use rune::{Cursor, Cx, Drag, El, LiveRegion, Phase, Transition, Tx, col, el, row, text};
use rune_kit::shell::{SplitView, Toolbar};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Spinner};

use crate::app::{App, LIST_RANGE, LIST_WIDTH};

/// The tokens every part draws with.
pub fn kit(dark: bool) -> Kit {
    Kit::neutral(dark)
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let k = kit(cx.system_dark());
    let mut layer = Vec::new();
    let base = if s.welcome {
        welcome::view(s, &k, cx.titlebar_inset(), &mut layer)
    } else {
        shell(s, &k, cx, &mut layer)
    };
    dialogs::build(s, &k, &mut layer);
    if let Some(n) = &s.notice {
        layer.push(toast(&n.text, &k));
    }
    rune_kit::root(base, layer)
}

fn shell(s: &App, k: &Kit, cx: &mut Cx<App>, layer: &mut Vec<El>) -> El {
    let c = &k.c;
    // The divider between the list and the skill, wider to the pointer than it draws.
    let grip = el()
        .key("list-grip")
        .pin(Some(-4.0), Some(0.0), None, Some(0.0))
        .w(9.0)
        .cursor(Cursor::ResizeColumn)
        .label("Resize the skills list")
        .on_drag(|s: &mut App, d: &mut Drag, _: &mut Tx| match d.phase {
            Phase::Down => {
                d.data = if d.clicks >= 2 {
                    LIST_WIDTH
                } else {
                    s.list_width
                }
            }
            Phase::Move | Phase::Up => {
                s.list_width = (d.data + d.dx).clamp(LIST_RANGE.0, LIST_RANGE.1).round()
            }
        });
    let main = row()
        .key("columns")
        .w_full()
        .h_full()
        .min_h(0.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border_muted)
        .child(list::view(s, k).w(s.list_width).h_full().min_h(0.0))
        .child(
            el().key("list-divider")
                .w(1.0)
                .h_full()
                .fill(c.border_muted)
                .child(grip),
        )
        .child(
            detail::view(s, k, layer)
                .grow(1.0)
                .min_w(0.0)
                .h_full()
                .min_h(0.0),
        );
    SplitView::new(main)
        .sidebar(
            &s.sidebar,
            |s: &mut App| &mut s.sidebar,
            sidebar::view(s, k),
        )
        .toolbar(toolbar(s, k))
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .reduced_motion(cx.is_reduced_motion())
        .build(k)
}

/// The scope shown, and the one thing to do in it.
fn toolbar(s: &App, k: &Kit) -> Toolbar {
    let c = &k.c;
    let (title, path) = match s.scope().and_then(|root| s.snap.as_ref()?.project(root)) {
        Some(p) => (p.info.name.clone(), s.tilde(&p.info.root)),
        None => ("Global".to_owned(), "~".to_owned()),
    };
    let heading = row()
        .key("scope")
        .gap(8.0)
        .items_end()
        .min_w(0.0)
        .shrink()
        .child(
            text(title)
                .key("scope-title")
                .role("heading")
                .level(1)
                .font(k.ty.ui_medium)
                .color(c.text_base)
                .truncate(),
        )
        .child(
            text(path)
                .key("scope-path")
                .font(k.ty.mono)
                .color(c.text_faint)
                .truncate_middle()
                .shrink()
                .selectable(true)
                .selection_color(k.selection()),
        );
    let mut bar = Toolbar::new().leading(heading);
    if s.scanning {
        bar = bar.trailing(
            row()
                .key("scanning")
                .gap(6.0)
                .center_y()
                .pr(4.0)
                .child(Spinner::new().size(12.0).build(k))
                .child(text("Scanning…").font(k.ty.small).color(c.text_faint)),
        );
    }
    bar.trailing(
        Button::new("New Skill")
            .icon(&icons::PLUS)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Regular)
            .help("Create a skill here")
            .on_press(|s: &mut App| s.open_new_skill())
            .build(k)
            .key("new-skill"),
    )
}

/// What just happened, at the bottom of the window for a moment.
fn toast(message: &str, k: &Kit) -> El {
    let c = &k.c;
    let pill = row()
        .key("notice")
        .role("status")
        .live_region(LiveRegion::Polite)
        .px(14.0)
        .py(8.0)
        .radius(k.radius.lg)
        .fill(c.bg_inverse)
        .max_w(560.0)
        .child(
            text(message.to_owned())
                .font(k.ty.ui)
                .color(c.text_inverse)
                .max_lines(2),
        );
    let pill = k
        .elevate(pill, &k.elevation.floating)
        .transition(Transition::opacity());
    row()
        .key("notice-layer")
        .pin(Some(0.0), None, Some(0.0), Some(20.0))
        .center()
        .inert()
        .child(pill)
}

/// A muted line in the middle of a pane that has nothing else to show.
pub(crate) fn empty(k: &Kit, title: &str, hint: &str) -> El {
    let mut e = col()
        .w_full()
        .h_full()
        .center()
        .gap(4.0)
        .padding(24.0)
        .child(
            text(title.to_owned())
                .font(k.ty.ui_medium)
                .color(k.c.text_muted),
        );
    if !hint.is_empty() {
        e = e.child(
            text(hint.to_owned())
                .font(k.ty.ui)
                .color(k.c.text_faint)
                .wrap()
                .align(rune::Align::Center),
        );
    }
    e
}
