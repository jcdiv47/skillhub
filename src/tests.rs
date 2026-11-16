//! The app driven headless: the real view in a `Runtime`, pressed and typed into as a person
//! would, over a home folder made for each test.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Dialog, Sys};
use crate::config::Config;
use crate::testing::{symlink, temp_dir, write};
use crate::view;

fn md(name: &str, desc: &str, body: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {desc}\nallowed-tools:\n  - Read\n  - Bash\n---\n{body}"
    )
}

/// A home folder with global skills, two projects and links between them.
struct Fx {
    rt: Runtime<App>,
    home: PathBuf,
    code: PathBuf,
    config: PathBuf,
    trashed: Rc<RefCell<Vec<PathBuf>>>,
    revealed: Rc<RefCell<Vec<PathBuf>>>,
}

fn fixture(roots: bool) -> Fx {
    let tmp = temp_dir("app");
    let (home, trash_dir) = (tmp.join("home"), tmp.join("trash"));
    let code = home.join("Code");
    let shared = home.join(".agents/skills");
    write(
        &shared.join("pdf/SKILL.md"),
        &md(
            "pdf",
            "Fill and read PDF forms.",
            "# PDF\n\nUse **pdftk** for forms. See [the guide](references/guide.md).\n\n```bash\npdftk in.pdf dump_data\n```\n",
        ),
    );
    write(&shared.join("pdf/references/guide.md"), "# Guide\n");
    write(
        &shared.join("review/SKILL.md"),
        &md("review", "Review code changes.", "# Review\n"),
    );
    symlink(shared.join("pdf"), &home.join(".claude/skills/pdf"));
    write(
        &home.join(".claude/skills/notes/SKILL.md"),
        "no frontmatter here\n",
    );
    write(
        &home.join(".pi/agent/skills/pi-only/SKILL.md"),
        &md("pi-only", "Only for pi.", "# Pi\n"),
    );
    let web = code.join("web");
    write(&web.join("package.json"), "{}");
    write(
        &web.join(".claude/skills/deploy/SKILL.md"),
        &md("deploy", "Deploy the site.", "# Deploy\n"),
    );
    symlink(shared.join("pdf"), &web.join(".claude/skills/pdf"));
    symlink(home.join("gone"), &web.join(".claude/skills/old"));
    write(
        &web.join(".pi/skills/lint/SKILL.md"),
        &md("lint", "Lint the code.", "# Lint\n"),
    );
    write(
        &code.join("api/.agents/skills/review/SKILL.md"),
        &md("review", "Project review rules.", "# Review here\n"),
    );

    let config = tmp.join("config.json");
    if roots {
        Config {
            roots: vec![code.clone()],
            depth: 3,
            ..Config::default()
        }
        .save(&config)
        .unwrap();
    }
    let (trashed, revealed) = (
        Rc::new(RefCell::new(Vec::new())),
        Rc::new(RefCell::new(Vec::new())),
    );
    let sys = Sys {
        home: home.clone(),
        config_path: config.clone(),
        trash: Rc::new({
            let trashed = trashed.clone();
            move |p: &Path| {
                trashed.borrow_mut().push(p.to_path_buf());
                fs::create_dir_all(&trash_dir)?;
                fs::rename(p, trash_dir.join(p.file_name().unwrap()))
            }
        }),
        reveal: Rc::new({
            let revealed = revealed.clone();
            move |p: &Path| revealed.borrow_mut().push(p.to_path_buf())
        }),
        background: false,
    };
    let mut rt = Runtime::new(
        App::new(sys),
        view::view,
        (1240.0, 800.0),
        Env::new(false, 1.0),
    );
    rt.check_live_only(false);
    rt.frame(0.0);
    Fx {
        rt,
        home,
        code,
        config,
        trashed,
        revealed,
    }
}

impl Fx {
    fn settle(&mut self) {
        for _ in 0..40 {
            self.rt.frame(1.0 / 60.0);
        }
    }

    fn press_at(&mut self, (x, y): (f32, f32)) {
        self.rt.pointer_move(x, y);
        self.rt.pointer_down(x, y);
        self.rt.pointer_up(x, y);
        self.settle();
    }

    /// Presses the element keyed `key`.
    #[track_caller]
    fn click(&mut self, key: &str) {
        let at = self
            .rt
            .find(key)
            .unwrap_or_else(|| panic!("no element {key:?}; texts: {:?}", self.texts()));
        self.press_at(at);
    }

    /// Presses what shows `text`, or is labelled it.
    #[track_caller]
    fn press(&mut self, text: &str) {
        let hit = self.rt.elements().into_iter().find(|e| {
            (e.text.as_deref() == Some(text) || e.label.as_deref() == Some(text))
                && e.rect[2] > 0.0
                && e.opacity > 0.0
                && !e.exiting
        });
        let e = hit.unwrap_or_else(|| panic!("nothing shows {text:?}; texts: {:?}", self.texts()));
        self.press_at((e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0));
    }

    fn skill(&mut self, path: PathBuf) {
        self.click(&format!("skill:{}", path.display()));
    }

    fn project(&mut self, name: &str) {
        self.click(&format!("project:{}", self.code.join(name).display()));
    }

    /// Opens the skill's actions and chooses one.
    #[track_caller]
    fn action(&mut self, name: &str) {
        self.click("actions");
        self.press(name);
    }

    fn key(&mut self, key: Key, cmd: bool) {
        self.rt.key(KeyEvent {
            key,
            mods: Mods {
                cmd,
                ..Mods::default()
            },
        });
        self.settle();
    }

    /// A shortcut of the window, as the shell hands it to the app.
    fn command(&mut self, c: char) {
        let ev = KeyEvent {
            key: Key::Command(c),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        };
        self.rt.update(move |s, tx| {
            crate::on_key(s, &ev, tx);
        });
        self.settle();
    }

    fn type_text(&mut self, text: &str) {
        self.key(Key::Text(text.into()), false);
    }

    /// Replaces what the focused field holds.
    fn retype(&mut self, text: &str) {
        self.key(Key::Command('a'), true);
        self.type_text(text);
    }

    fn texts(&self) -> Vec<String> {
        self.rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| e.text)
            .collect()
    }

    #[track_caller]
    fn shows(&self, wanted: &[&str]) {
        let texts = self.texts();
        for w in wanted {
            assert!(texts.iter().any(|t| t == w), "no text {w:?} in {texts:?}");
        }
    }

    #[track_caller]
    fn hides(&self, unwanted: &[&str]) {
        let texts = self.texts();
        for w in unwanted {
            assert!(!texts.iter().any(|t| t == w), "unexpected text {w:?}");
        }
    }

    fn app(&self) -> &App {
        &self.rt.state
    }

    fn notice(&self) -> String {
        self.app()
            .notice
            .as_ref()
            .map(|n| n.text.clone())
            .unwrap_or_default()
    }

    /// Types into the editor, as edits not saved.
    fn edit(&mut self, text: &str) {
        let text = text.to_owned();
        self.rt
            .update(move |s, _| s.doc.edit = rune::TextEdit::new(text));
        self.settle();
    }

    fn shared(&self, name: &str) -> PathBuf {
        self.home.join(".agents/skills").join(name)
    }
}

fn is_link(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

#[test]
fn the_first_run_asks_for_folders_then_scans() {
    let mut f = fixture(false);
    f.shows(&["Welcome to SkillHub", "No folders chosen yet."]);
    f.click("add-folder");
    assert!(f.rt.file_dialog().is_some(), "a folder dialog opens");
    f.rt.resolve_file_dialog(Some(f.code.clone())).unwrap();
    f.settle();
    f.shows(&["~/Code"]);
    f.click("scan");
    f.hides(&["Welcome to SkillHub"]);
    f.shows(&["Global", "web", "api", "Projects"]);
    assert_eq!(Config::load(&f.config).unwrap().roots, [f.code.clone()]);
}

#[test]
fn skills_show_by_folder_with_only_what_stands_out() {
    let mut f = fixture(true);
    f.shows(&[
        ".agents/skills",
        ".claude/skills",
        ".pi/agent/skills",
        "pdf",
        "review",
        "notes",
        "pi-only",
        "Fill and read PDF forms.",
    ]);
    f.hides(&["×2"]); // pdf in .agents and its link in .claude are one skill
    f.hides(&["Not created", "Empty"]);

    f.project("web");
    f.shows(&[
        "deploy",
        "lint",
        "old",
        "Deploy the site.",
        "→ ~/gone · missing",
    ]);
    f.hides(&["pi-only", "notes", ".agents/skills"]);

    let web = f.code.join("web/.claude/skills");
    f.skill(web.join("old"));
    f.shows(&["missing"]);
    f.skill(web.join("pdf"));
    f.shows(&[
        ".claude/skills/pdf", // from the project's root
        "~/.agents/skills/pdf",
        "Fill and read PDF forms.",
        "Frontmatter · allowed-tools",
    ]);
    f.hides(&["- Read\n- Bash"]);
    f.press("Frontmatter · allowed-tools");
    f.shows(&["- Read\n- Bash"]);

    // Its links: where it points, and the other link to the same folder.
    f.press("Links 1");
    f.shows(&[
        "Points to",
        "~/.claude/skills/pdf",
        "Other links to the same folder",
    ]);
    f.click(&format!(
        "goto:{}",
        f.home.join(".claude/skills/pdf").display()
    ));
    assert!(f.app().scope.is_none());
    assert_eq!(f.app().skill_path, Some(f.home.join(".claude/skills/pdf")));

    f.action(if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else {
        "Show in Folder"
    });
    assert_eq!(*f.revealed.borrow(), [f.shared("pdf")]);
    f.action("Copy Real Path");
    assert_eq!(f.notice(), "Copied ~/.agents/skills/pdf");

    f.skill(f.home.join(".claude/skills/notes"));
    f.shows(&["SKILL.md has no frontmatter (--- name/description --- at the top)."]);

    f.click("skill-filter");
    f.type_text("pi");
    f.shows(&["pi-only"]);
    f.hides(&["review", "commit-messages"]);
}

#[test]
fn a_name_in_the_project_and_globally_counts_twice() {
    let mut f = fixture(true);
    f.project("api");
    f.shows(&["review", "×2"]);
}

#[test]
fn a_folder_linked_to_another_folds_until_asked_for() {
    let mut f = fixture(true);
    let api = f.code.join("api");
    symlink(api.join(".agents/skills"), &api.join(".claude/skills"));
    f.rt.update(|s, _| s.reload());
    f.settle();
    f.project("api");
    let twin = format!("skill:{}", api.join(".claude/skills/review").display());
    f.shows(&[".agents/skills", ".claude/skills → .agents/skills"]);
    assert!(
        f.rt.find(&twin).is_none(),
        "the linked folder lists nothing"
    );
    f.click("location:Claude");
    f.click(&twin);
    f.shows(&[".claude/skills/review", ".agents/skills/review"]);
    // SKILL.md alone shows without a list beside it.
    f.click("edit");
    assert!(f.rt.find("file-list").is_none() && f.rt.find("new-file").is_some());
    f.click("editor-close");

    // The link folds wherever it is listed: here the first folder links to the second.
    let web = f.code.join("web");
    symlink(web.join(".claude/skills"), &web.join(".agents/skills"));
    f.rt.update(|s, _| s.reload());
    f.project("web");
    f.shows(&[".agents/skills → .claude/skills", ".claude/skills"]);
    let real = format!("skill:{}", web.join(".claude/skills/deploy").display());
    assert!(
        f.rt.find(&real).is_some(),
        "the real folder lists its skills"
    );
}

#[test]
fn links_to_another_folder_of_the_name_count_once() {
    let mut f = fixture(true);
    symlink(f.shared("review"), &f.home.join(".claude/skills/review"));
    f.rt.update(|s, _| s.reload());
    f.project("api");
    f.shows(&["×2"]);
    f.hides(&["×3"]);
}

#[test]
fn a_preview_link_opens_the_file_and_stays_inside_the_skill() {
    let mut f = fixture(true);
    f.skill(f.shared("pdf"));
    f.rt.update(|s, tx| s.follow_link("references/guide.md", tx));
    f.settle();
    assert!(f.app().editing);
    assert_eq!(f.app().doc.path, Some(f.shared("pdf/references/guide.md")));
    f.rt.update(|s, tx| s.follow_link("../../../escape", tx));
    assert_eq!(f.app().doc.path, Some(f.shared("pdf/references/guide.md")));
}

#[test]
fn edits_save_through_a_link_and_follow_the_disk() {
    let mut f = fixture(true);
    f.project("web");
    f.skill(f.code.join("web/.claude/skills/pdf"));
    f.click("edit");
    f.shows(&["SKILL.md", "references", "guide.md"]);
    f.hides(&["Save", "Revert"]);
    assert!(f.app().doc.editable && !f.app().dirty());

    f.click("editor");
    f.type_text("x");
    assert!(f.app().dirty(), "typing reaches the editor");
    f.shows(&["Save", "Revert"]);
    let text = f.app().doc.edit.text.replace("# PDF", "# PDF tools");
    f.edit(&text);
    f.command('s');
    assert!(!f.app().dirty());
    assert!(
        fs::read_to_string(f.shared("pdf/SKILL.md"))
            .unwrap()
            .contains("# PDF tools")
    );
    assert!(
        is_link(&f.code.join("web/.claude/skills/pdf")),
        "the link stays a link"
    );
    assert_eq!(f.notice(), "Saved SKILL.md");

    // A file changed on disk shows in the editor when it has no edits.
    write(
        &f.shared("pdf/SKILL.md"),
        &md("pdf", "Changed elsewhere.", "# Elsewhere\n"),
    );
    f.rt.update(|s, _| s.reload());
    assert!(f.app().doc.edit.text.contains("Changed elsewhere.") && !f.app().dirty());
    // With edits, it says so instead.
    f.edit("mine");
    write(&f.shared("pdf/SKILL.md"), &md("pdf", "Again.", ""));
    f.rt.update(|s, _| s.reload());
    f.settle();
    f.shows(&["The file changed on disk."]);
    assert_eq!(f.app().doc.edit.text, "mine");
    f.click("load-from-disk");
    assert!(f.app().doc.edit.text.contains("Again."));
}

#[test]
fn edits_not_saved_are_asked_about_before_the_editor_closes() {
    let mut f = fixture(true);
    f.skill(f.shared("review"));
    f.click("edit");
    assert!(f.app().editing);
    f.edit("changed");
    f.click("editor-close");
    f.shows(&["Save changes to SKILL.md?"]);
    assert!(f.app().editing, "it stays until answered");
    f.click("cancel");
    assert!(f.app().dirty() && f.app().editing);
    f.click("editor-close");
    f.click("discard");
    assert!(!f.app().editing && !f.app().dirty());
    assert_ne!(
        fs::read_to_string(f.shared("review/SKILL.md")).unwrap(),
        "changed"
    );

    // Return saves, then closes. Escape asks as the button does.
    f.click("edit");
    f.edit(&md("review", "Saved from the guard.", ""));
    f.key(Key::Escape, false);
    f.shows(&["Save changes to SKILL.md?"]);
    f.key(Key::Enter, false);
    assert!(!f.app().editing);
    assert!(
        fs::read_to_string(f.shared("review/SKILL.md"))
            .unwrap()
            .contains("Saved from the guard.")
    );
    // With nothing to save it only closes.
    f.click("edit");
    f.click("editor-close");
    assert!(!f.app().editing && matches!(f.app().dialog, Dialog::None));
}

#[test]
fn a_new_skill_goes_to_the_folder_chosen() {
    let mut f = fixture(true);
    f.project("web");
    f.click("new-skill");
    f.shows(&[
        "New Skill in web",
        ".agents/skills",
        ".claude/skills",
        ".pi/skills",
    ]);
    f.press(".agents/skills"); // not created in web yet
    f.click("new-skill-name");
    f.type_text("data-viz");
    f.key(Key::Tab, false);
    f.type_text("Draw charts.");
    f.shows(&["Creates ~/Code/web/.agents/skills/data-viz/SKILL.md"]);
    f.click("confirm");
    let p = f.code.join("web/.agents/skills/data-viz");
    let m = crate::skills::parse_frontmatter(&fs::read_to_string(p.join("SKILL.md")).unwrap());
    assert_eq!(
        (m.name.as_str(), m.description.as_str()),
        ("data-viz", "Draw charts.")
    );
    assert!(
        f.app().skill_path == Some(p) && f.app().editing && f.app().doc.editable,
        "opened for editing"
    );

    f.click("editor-close");
    f.click("new-skill");
    let claude =
        f.rt.elements()
            .into_iter()
            .rfind(|e| e.text.as_deref() == Some(".claude/skills"))
            .unwrap();
    f.press_at((claude.rect[0] + 4.0, claude.rect[1] + 4.0));
    f.click("new-skill-name");
    f.type_text("deploy");
    f.shows(&["~/Code/web/.claude/skills/deploy already exists"]);
}

#[test]
fn a_skill_links_into_other_folders_and_projects() {
    let mut f = fixture(true);
    f.skill(f.shared("review"));
    f.action("Link Into…");
    f.shows(&[
        "Agents · .agents/skills — the skill is here",
        "Claude · .claude/skills",
        "Pi · .pi/agent/skills",
    ]);
    f.click("link-into:Claude");
    f.click("link-into:Pi");
    f.click("confirm");
    for p in [
        f.home.join(".claude/skills/review"),
        f.home.join(".pi/agent/skills/review"),
    ] {
        assert_eq!(fs::read_link(&p).unwrap(), f.shared("review"));
    }
    // Into a project: its scope in the dialog.
    f.action("Link Into…");
    f.click("link-scope-trigger");
    let web = format!("web — {}", "~/Code/web");
    f.press(&web);
    f.shows(&["Pi · .pi/skills"]);
    f.click("link-into:Pi");
    f.click("confirm");
    assert!(is_link(&f.code.join("web/.pi/skills/review")));
    f.press("Links 3");
    f.shows(&["~/Code/web/.pi/skills/review"]);
}

#[test]
fn a_broken_link_is_pointed_at_a_folder() {
    let mut f = fixture(true);
    f.project("web");
    let old = f.code.join("web/.claude/skills/old");
    f.skill(old.clone());
    f.action("Change Target…");
    f.shows(&["the folder does not exist"]);
    f.retype("~/.agents/skills/review");
    f.hides(&["the folder does not exist"]);
    f.click("confirm");
    assert_eq!(fs::read_link(&old).unwrap(), f.shared("review"));
    f.hides(&["missing"]);
    // Choosing the folder instead of typing it.
    f.action("Change Target…");
    f.click("choose-target");
    f.rt.resolve_file_dialog(Some(f.shared("pdf"))).unwrap();
    f.settle();
    f.click("confirm");
    assert_eq!(fs::read_link(&old).unwrap(), f.shared("pdf"));
}

#[test]
fn a_link_becomes_a_real_copy() {
    let mut f = fixture(true);
    f.project("web");
    let p = f.code.join("web/.claude/skills/pdf");
    f.skill(p.clone());
    f.action("Make Real Copy…");
    f.shows(&["Replace the link with a copy?"]);
    f.click("confirm");
    assert!(!is_link(&p) && p.join("references/guide.md").exists());
    f.click("actions");
    f.shows(&["Move & Link…", "Move to Trash…"]);
}

#[test]
fn a_folder_moves_to_a_shared_place_and_links_back() {
    let mut f = fixture(true);
    f.project("web");
    let p = f.code.join("web/.claude/skills/deploy");
    f.skill(p.clone());
    f.action("Move & Link…");
    f.shows(&["~/.agents/skills/deploy  ←  link at ~/Code/web/.claude/skills/deploy"]);
    f.click("confirm");
    assert_eq!(fs::read_link(&p).unwrap(), f.shared("deploy"));
    f.shows(&["~/.agents/skills/deploy"]);
    // Moving where a skill of the name is refused.
    f.click("scope-global");
    f.skill(f.home.join(".claude/skills/notes"));
    write(&f.shared("notes/SKILL.md"), "taken");
    f.action("Move & Link…");
    f.shows(&["~/.agents/skills/notes already exists"]);
}

#[test]
fn renaming_a_folder_updates_its_name_and_its_links() {
    let mut f = fixture(true);
    let relative = f.home.join(".pi/agent/skills/pdf");
    symlink("../../../.agents/skills/pdf", &relative);
    f.rt.update(|s, _| s.reload());
    f.skill(f.shared("pdf"));
    f.action("Rename…");
    f.shows(&[
        "Also change name: in SKILL.md",
        "Point the 3 links to this folder at its new name",
    ]);
    f.retype("pdf-forms");
    f.click("confirm");
    let np = f.shared("pdf-forms");
    assert!(
        fs::read_to_string(np.join("SKILL.md"))
            .unwrap()
            .contains("name: pdf-forms")
    );
    for l in [
        f.home.join(".claude/skills/pdf"),
        f.code.join("web/.claude/skills/pdf"),
    ] {
        assert_eq!(fs::read_link(&l).unwrap(), np);
    }
    assert_eq!(
        fs::read_link(&relative).unwrap(),
        Path::new("../../../.agents/skills/pdf-forms"),
        "relative links stay relative"
    );
    assert!(relative.join("SKILL.md").exists());
    assert_eq!(f.app().skill_path, Some(np));
    assert_eq!(f.notice(), "Renamed to pdf-forms");
}

#[test]
fn removing_unlinks_a_link_and_trashes_a_folder() {
    let mut f = fixture(true);
    f.project("web");
    let link = f.code.join("web/.claude/skills/pdf");
    f.skill(link.clone());
    f.action("Remove Link…");
    f.shows(&["Remove the link “pdf”?"]);
    f.key(Key::Enter, false);
    assert!(fs::symlink_metadata(&link).is_err() && f.shared("pdf/SKILL.md").exists());
    assert!(
        f.trashed.borrow().is_empty(),
        "a link never goes to the trash"
    );

    // A folder others link to: the dialog lists them, and it goes to the trash.
    f.click("scope-global");
    f.skill(f.shared("pdf"));
    f.action("Move to Trash…");
    f.shows(&["Move “pdf” to the Trash?"]);
    assert!(
        f.texts()
            .iter()
            .any(|t| t.contains("1 link points into it and will break")),
        "{:?}",
        f.texts()
    );
    f.click("confirm");
    assert_eq!(*f.trashed.borrow(), [f.shared("pdf")]);
    assert_eq!(f.notice(), "Moved pdf to the Trash");
    f.shows(&["No skill selected"]);
    let left = f.home.join(".claude/skills/pdf");
    assert!(
        f.app()
            .snap
            .as_ref()
            .unwrap()
            .find(&left)
            .is_some_and(|s| s.broken)
    );
}

#[test]
fn files_are_made_and_deleted_from_the_list() {
    let mut f = fixture(true);
    f.skill(f.shared("review"));
    f.click("edit");
    f.click("new-file");
    f.type_text("scripts/check.sh");
    f.click("confirm");
    let p = f.shared("review/scripts/check.sh");
    assert!(p.exists());
    assert_eq!(f.app().doc.path, Some(p.clone()));
    f.shows(&["check.sh", "scripts"]);

    let (x, y) = f.rt.find("file:scripts/check.sh").unwrap();
    f.rt.secondary_down(x, y);
    f.rt.secondary_up(x, y);
    f.settle();
    f.press("Delete…");
    f.shows(&["Move “scripts/check.sh” to the Trash?"]);
    f.key(Key::Enter, false);
    assert!(!p.exists());
    assert_eq!(
        f.app().doc.path,
        Some(f.shared("review/SKILL.md")),
        "SKILL.md opens in its place"
    );
}

#[test]
fn a_binary_file_is_not_edited() {
    let mut f = fixture(true);
    fs::write(f.shared("review/logo.png"), b"\x89PNG\x00\x00").unwrap();
    f.rt.update(|s, _| s.reload());
    f.skill(f.shared("review"));
    f.click("edit");
    f.click("file:logo.png");
    f.shows(&["A binary file (6 B)."]);
    assert!(!f.app().doc.editable);
}

#[test]
fn folders_to_scan_are_added_and_removed() {
    let mut f = fixture(true);
    let other = f.home.join("Work");
    write(&other.join("svc/.pi/skills/x/SKILL.md"), &md("x", "X.", ""));
    f.click("folders");
    f.shows(&["Folders to Scan", "~/Code"]);
    f.click("add-folder");
    f.rt.resolve_file_dialog(Some(other)).unwrap();
    f.settle();
    f.click("confirm");
    f.shows(&["svc", "web"]);
    assert_eq!(f.app().snap.as_ref().unwrap().projects.len(), 3);
    f.click("folders");
    f.press("Remove ~/Code");
    f.click("confirm");
    f.shows(&["svc"]);
    f.hides(&["web"]);
    assert_eq!(f.app().snap.as_ref().unwrap().projects.len(), 1);
}

#[test]
fn every_button_has_a_name() {
    let mut f = fixture(true);
    f.skill(f.shared("pdf"));
    f.click("edit");
    for e in f.rt.elements() {
        assert!(
            e.role != "button" || e.label.is_some_and(|l| !l.is_empty()),
            "the button {:?} has no name",
            e.path
        );
    }
}

#[test]
fn paths_under_a_linked_home_read_with_a_tilde() {
    let dir = temp_dir("tilde");
    let (real, linked) = (dir.join("real"), dir.join("linked"));
    fs::create_dir_all(&real).unwrap();
    symlink(&real, &linked);
    let sys = Sys {
        home: linked.clone(),
        config_path: dir.join("c.json"),
        trash: Rc::new(|_: &Path| Ok(())),
        reveal: Rc::new(|_: &Path| {}),
        background: false,
    };
    let a = App::new(sys);
    assert_eq!(a.tilde(&linked.join(".agents")), "~/.agents");
    assert_eq!(a.tilde(&real.join(".agents")), "~/.agents");
    assert_eq!(a.tilde(&linked), "~");
    assert_eq!(a.tilde(Path::new("/elsewhere")), "/elsewhere");
    assert_eq!(a.expand("~/x"), linked.join("x"));
}

#[test]
fn edits_stay_when_their_file_goes_from_the_disk() {
    let mut f = fixture(true);
    f.skill(f.shared("review"));
    f.click("edit");
    f.edit(&md("review", "Mine.", ""));
    let p = f.shared("review/SKILL.md");
    fs::remove_file(&p).unwrap();
    f.rt.update(|s, _| s.reload());
    f.settle();
    assert!(f.app().dirty() && f.app().doc.path == Some(p.clone()));
    f.shows(&["The file changed on disk."]);
    f.click("save");
    assert!(fs::read_to_string(&p).unwrap().contains("Mine."));
}

#[test]
fn a_new_file_and_closing_ask_about_edits_first() {
    let mut f = fixture(true);
    assert!(
        f.rt.state.confirm_close(),
        "nothing to save, nothing to ask"
    );
    f.skill(f.shared("review"));
    f.click("edit");
    f.edit("unsaved");
    f.click("new-file");
    f.shows(&["Save changes to SKILL.md?"]);
    assert!(matches!(f.app().dialog, Dialog::Discard));
    f.click("cancel");
    assert!(f.app().dirty());

    f.rt.update(|s, _| assert!(!s.confirm_close(), "the window stays for the answer"));
    f.settle();
    f.shows(&["Save changes to SKILL.md?"]);
    f.click("discard");
    assert!(!f.app().dirty() && matches!(f.app().dialog, Dialog::None));
}
