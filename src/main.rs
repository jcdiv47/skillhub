//! SkillHub manages agent skills across the projects of the machine: the skills of
//! `.agents/skills`, `.claude/skills` and pi, global and per project, with their links.

mod actions;
mod app;
mod config;
mod scenes;
mod skills;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
mod view;
mod watch;

use std::sync::OnceLock;

use rune::{AppHandle, AppSpec, Chrome, KeyEvent, Tx, Waker};
use rune_kit::shell;

use app::{App, Sys};

/// The window, for showing it again when it closed over edits not saved.
static WINDOW: OnceLock<AppHandle<App>> = OnceLock::new();

fn init() -> App {
    let Some(sys) = Sys::real() else {
        eprintln!("skillhub: no home folder");
        std::process::exit(1);
    };
    App::new(sys)
}

fn setup(s: &mut App, waker: Waker) {
    s.set_waker(waker);
}

fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    s.poll(clock, tx)
}

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if shell::is_sidebar_toggle(ev) {
        s.sidebar.toggle();
        return true;
    }
    // The dialogs keep the keyboard to themselves.
    if !matches!(s.dialog, app::Dialog::None) || ev.mods.shift || ev.mods.alt {
        return false;
    }
    // So does the editor, but for saving.
    if s.editing {
        if ev.is_command('s') {
            s.save(tx);
        }
        return ev.is_command('s');
    }
    if ev.is_command('n') && !s.welcome {
        s.open_new_skill();
    } else if ev.is_command('f') && !s.welcome {
        s.skill_query.focused = true;
        let all = s.skill_query.text.len();
        s.skill_query.set_selection(0, all);
    } else if ev.is_command('r') && !s.welcome {
        s.scan();
    } else {
        return false;
    }
    true
}

/// What changed while the app was in the background shows as it comes back.
fn on_focus(s: &mut App, focused: bool, _: &mut Tx) {
    if focused {
        s.reload_later();
    }
}

/// The window is closing. With edits not saved it comes back and asks; else the app quits.
fn on_close(s: &mut App, tx: &mut Tx) {
    s.save_config();
    if rune::quitting() {
        return;
    }
    if s.confirm_close() {
        tx.quit();
    } else if let Some(window) = WINDOW.get() {
        window.open();
    }
}

fn spec() -> AppSpec<App> {
    AppSpec {
        min_size: Some((900.0, 560.0)),
        scenes: scenes::SCENES,
        // Inter for the interface, the system's monospaced font for paths and code.
        families: ("Inter", "ui-monospace"),
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        ..AppSpec::new("SkillHub", (1240.0, 800.0), init, view::view)
    }
}

fn main() {
    rune::prewarm_gpu();
    // The traffic lights sit in the toolbar's row, over the sidebar.
    let chrome = Chrome {
        full_size_content: true,
        traffic_lights: Some((16.0, 25.0)),
        drag_regions: true,
    };
    let app = rune::App::new(spec())
        .chrome(chrome)
        .keep_running()
        .on_close(on_close)
        .on_focus(on_focus);
    let _ = WINDOW.set(app.handle());
    app.run();
}
