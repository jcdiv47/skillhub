//! The first run: which folders hold the projects.

use rune::{El, Tx, col, row, text};
use rune_kit::shell::Page;
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::dialogs::roots_editor;
use crate::app::App;

pub fn view(s: &App, k: &Kit, titlebar_inset: f32, layer: &mut Vec<El>) -> El {
    let c = &k.c;
    let scan = Button::new("Scan")
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Regular)
        .disabled(s.cfg.roots.is_empty())
        .on_press_tx(|s: &mut App, _: &mut Tx| s.save_folders())
        .build(k)
        .key("scan");
    let card = col()
        .key("welcome")
        .w_full()
        .max_w(520.0)
        .gap(14.0)
        .child(text("Welcome to SkillHub").role("heading").level(1).font(rune::TextStyle::new(22.0, 600.0).lh(28.0)).color(c.text_base))
        .child(
            text("Choose the folders that hold your projects. SkillHub looks in them for .agents/skills, .claude/skills and .pi/skills. The skills of your home folder always show.")
                .font(k.ty.body)
                .color(c.text_muted)
                .wrap(),
        )
        .child(roots_editor(s, k, layer))
        .child(row().w_full().justify_end().child(scan));
    let body = col()
        .key("welcome-page")
        .w_full()
        .h_full()
        .padding(32.0)
        .center()
        .child(card);
    Page::new(body)
        .titlebar_inset(titlebar_inset)
        .frame_inset(8.0)
        .toolbar_in_panel()
        .build(k)
}
