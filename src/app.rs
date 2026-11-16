//! SkillHub's state, and what choosing things in it does. Changes to skills live in
//! `actions.rs`; nothing here draws.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rune::{Latest, PointerPress, TextEdit, Tx};
use rune_kit::PaneState;

use crate::config::Config;
use crate::skills::{self, Agent, File, Location, ProjectInfo, Skill, Snapshot};
use crate::watch::Watcher;

/// The largest file opened in the editor.
pub const MAX_EDIT_SIZE: u64 = 1 << 20;

/// How long a notice stays, in seconds.
const NOTICE_SECS: f64 = 3.5;

/// What the app needs of the system, replaced in tests.
#[derive(Clone)]
pub struct Sys {
    pub home: PathBuf,
    pub config_path: PathBuf,
    /// Moves a folder or file to the trash.
    pub trash: Rc<skills::Trasher<'static>>,
    /// Shows a path in the file manager.
    pub reveal: Rc<dyn Fn(&Path)>,
    /// Scans and reloads off the UI thread. Off in tests and headless runs, which read at once.
    pub background: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Preview,
    Files,
    Links,
}

/// The file of the skill open in the editor.
#[derive(Default)]
pub struct Doc {
    pub path: Option<PathBuf>,
    pub edit: TextEdit,
    /// Whether the file is text the editor shows; `note` says why not.
    pub editable: bool,
    pub note: String,
    /// What the file held on disk when read or saved.
    pub disk: String,
    /// The file changed on disk while it had edits.
    pub changed: bool,
}

/// The one dialog open, with only what it edits.
#[allow(clippy::large_enum_variant)]
pub enum Dialog {
    None,
    Folders,
    NewSkill {
        agent: Agent,
        name: TextEdit,
        desc: TextEdit,
    },
    Rename {
        name: TextEdit,
        meta: bool,
        relink: bool,
    },
    LinkInto {
        scope: Option<PathBuf>,
        agents: [bool; 3],
        name: TextEdit,
    },
    Retarget {
        path: TextEdit,
    },
    Convert {
        dest: PathBuf,
    },
    Materialize,
    DeleteSkill,
    NewFile {
        rel: TextEdit,
    },
    DeleteFile(File),
    /// Asks about edits not saved before `App::pending` runs.
    Discard,
    Error {
        title: String,
        text: String,
    },
}

/// What waits on the answer about edits not saved.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Scope(Option<PathBuf>),
    Skill(PathBuf),
    /// Another skill, in its own scope.
    GoTo(PathBuf),
    File(PathBuf),
    NewFile,
    Quit,
}

pub struct Notice {
    pub text: String,
    pub due: f64,
}

/// A scan or reload read off the UI thread.
pub struct Loaded {
    snap: Snapshot,
    generation: u64,
}

pub struct App {
    pub sys: Sys,
    pub cfg: Config,
    pub snap: Option<Snapshot>,
    pub scanning: bool,
    /// The first-run page, until the user scans.
    pub welcome: bool,

    /// The root of the project shown, `None` for the global skills.
    pub scope: Option<PathBuf>,
    pub skill_path: Option<PathBuf>,
    pub proj_query: TextEdit,
    pub skill_query: TextEdit,
    pub tab: Tab,

    pub files: Vec<File>,
    pub doc: Doc,

    pub sidebar: PaneState,
    pub list_width: f32,
    /// The frontmatter's other fields show under the description.
    pub front_open: bool,

    pub dialog: Dialog,
    pub pending: Option<Pending>,
    /// The dialog's select is open.
    pub select_open: bool,
    /// The skill's actions menu is open.
    pub menu_open: bool,
    /// The file whose menu is open, and where.
    pub file_menu: Option<(File, PointerPress)>,
    pub notice: Option<Notice>,
    /// Where a field's edits go once its dialog closed under it.
    pub scratch: TextEdit,

    loads: Latest<Loaded>,
    generation: u64,
    watcher: Watcher,
    reload_due: Option<f64>,
    real_home: PathBuf,
}

pub const SIDEBAR_WIDTH: f32 = 220.0;
pub const LIST_WIDTH: f32 = 280.0;
pub const LIST_RANGE: (f32, f32) = (200.0, 480.0);

impl App {
    pub fn new(sys: Sys) -> App {
        let (cfg, err) = match Config::load(&sys.config_path) {
            Ok(cfg) => (cfg, None),
            Err(e) => (
                Config {
                    depth: crate::config::DEFAULT_DEPTH,
                    ..Config::default()
                },
                Some(e),
            ),
        };
        let sidebar = PaneState::new(SIDEBAR_WIDTH).range(160.0, 400.0);
        let sidebar = if cfg.sidebar_width > 0.0 {
            sidebar.with_width(cfg.sidebar_width)
        } else {
            sidebar
        };
        let list_width = if cfg.list_width > 0.0 {
            cfg.list_width.clamp(LIST_RANGE.0, LIST_RANGE.1)
        } else {
            LIST_WIDTH
        };
        let real_home = fs::canonicalize(&sys.home).unwrap_or_else(|_| sys.home.clone());
        let mut a = App {
            sys,
            cfg,
            snap: None,
            scanning: false,
            welcome: false,
            scope: None,
            skill_path: None,
            proj_query: TextEdit::new(""),
            skill_query: TextEdit::new(""),
            tab: Tab::Preview,
            files: Vec::new(),
            doc: Doc::default(),
            sidebar,
            list_width,
            front_open: false,
            dialog: Dialog::None,
            pending: None,
            select_open: false,
            menu_open: false,
            file_menu: None,
            notice: None,
            scratch: TextEdit::new(""),
            loads: Latest::default(),
            generation: 0,
            watcher: Watcher::default(),
            reload_due: None,
            real_home,
        };
        if let Some(e) = err {
            a.fail("Couldn't read the settings", e);
        }
        if a.cfg.roots.is_empty() {
            a.welcome = true;
        } else {
            a.scan();
        }
        a
    }

    /// Wakes the window when a scan lands or a watched folder changes.
    pub fn set_waker(&mut self, waker: rune::Waker) {
        self.loads.set_waker(waker.clone());
        self.watcher.set_waker(waker);
        self.watch();
    }

    pub fn save_config(&mut self) {
        self.cfg.sidebar_width = self.sidebar.width();
        self.cfg.list_width = self.list_width;
        if let Err(e) = self.cfg.save(&self.sys.config_path) {
            self.fail("Couldn't save the settings", e);
        }
    }

    /// Finds the projects of the scan folders and reads every skill.
    pub fn scan(&mut self) {
        if self.scanning {
            return;
        }
        let (roots, depth, home) = (
            self.cfg.roots.clone(),
            self.cfg.depth,
            self.sys.home.clone(),
        );
        let read = move || skills::load(&home, skills::find_projects(&roots, depth, &home));
        if self.sys.background {
            self.scanning = true;
            let generation = self.generation;
            self.loads.request((), move |()| Loaded {
                snap: read(),
                generation,
            });
        } else {
            self.set_snapshot(read());
        }
    }

    /// The projects known that still hold skills, to read again.
    fn known(&self) -> Vec<ProjectInfo> {
        let Some(snap) = &self.snap else {
            return Vec::new();
        };
        snap.projects
            .iter()
            .filter(|p| skills::has_any_location(&p.info.root))
            .map(|p| p.info.clone())
            .collect()
    }

    /// Reads the skills of the projects known again, as after a change.
    pub fn reload(&mut self) {
        // What a reload started earlier read is older than this.
        self.generation += 1;
        let snap = skills::load(&self.sys.home, self.known());
        self.set_snapshot(snap);
    }

    /// Reads them again off the UI thread: something changed on disk, or the window came back.
    pub fn reload_later(&mut self) {
        if self.snap.is_none() || self.scanning {
            return;
        }
        if !self.sys.background {
            return self.reload();
        }
        let (home, known, generation) = (self.sys.home.clone(), self.known(), self.generation);
        self.loads.request((), move |()| Loaded {
            snap: skills::load(&home, known),
            generation,
        });
    }

    pub fn set_snapshot(&mut self, snap: Snapshot) {
        if self
            .scope
            .as_deref()
            .is_some_and(|root| snap.project(root).is_none())
        {
            self.scope = None;
        }
        let gone = self
            .skill_path
            .as_deref()
            .is_some_and(|p| snap.find(p).is_none());
        self.snap = Some(snap);
        if gone {
            self.close_skill();
        }
        self.refresh_files();
        self.watch();
    }

    /// Watches the skills folders shown, so what changes in them shows.
    fn watch(&mut self) {
        let Some(snap) = &self.snap else { return };
        let (mut dirs, mut roots) = (Vec::new(), Vec::new());
        let mut add = |base: &Path, ls: &[Location]| {
            roots.push(base.to_path_buf());
            for l in ls {
                if l.exists {
                    dirs.push(l.dir.clone());
                    dirs.extend(l.real_dir.clone().filter(|r| *r != l.dir));
                } else if let Some(parent) = l.dir.parent().filter(|p| p.is_dir()) {
                    roots.push(parent.to_path_buf());
                }
            }
        };
        add(&self.sys.home, &snap.global);
        if let Some(scope) = &self.scope {
            add(scope, snap.locations(Some(scope)));
        }
        dirs.extend(self.skill().and_then(|s| s.real_path.clone()));
        self.watcher.watch(dirs, roots);
    }

    /// Before each frame: takes in what was read off the UI thread, and lets a notice go.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        if let Some(loaded) = self.loads.take() {
            self.scanning = false;
            if loaded.generation == self.generation {
                self.set_snapshot(loaded.snap);
            }
            tx.instant();
            changed = true;
        }
        if self.notice.as_ref().is_some_and(|n| clock >= n.due) {
            self.notice = None;
            changed = true;
        }
        // A burst of changes settles before the folders are read again.
        if self.watcher.take_changed() {
            let due = clock + 0.3;
            self.reload_due = Some(due);
            tx.wake_at(due);
        }
        if self.reload_due.is_some_and(|due| clock >= due) {
            self.reload_due = None;
            self.reload_later();
        }
        changed
    }

    /// The skill shown.
    pub fn skill(&self) -> Option<&Skill> {
        self.snap.as_ref()?.find(self.skill_path.as_deref()?)
    }

    /// The project shown, `None` for the global skills.
    pub fn scope(&self) -> Option<&Path> {
        self.scope.as_deref()
    }

    /// Edits not saved.
    pub fn dirty(&self) -> bool {
        self.doc.editable && self.doc.edit.text != self.doc.disk
    }

    /// Runs `p`, after asking whether to discard edits not saved.
    pub fn guard(&mut self, p: Pending, tx: &mut Tx) {
        if self.dirty() {
            self.pending = Some(p);
            self.dialog = Dialog::Discard;
        } else {
            self.run(p, tx);
        }
    }

    pub fn run(&mut self, p: Pending, tx: &mut Tx) {
        match p {
            Pending::Scope(root) => self.show_scope(root),
            Pending::Skill(path) => self.show_skill(&path),
            Pending::GoTo(path) => self.show_elsewhere(&path),
            Pending::File(path) => {
                self.open_file(Some(&path));
                self.tab = Tab::Files;
            }
            Pending::NewFile => self.dialog = Dialog::NewFile { rel: focused("") },
            Pending::Quit => tx.quit(),
        }
    }

    /// The answer about edits not saved: discard them, or save them, then go on.
    pub fn discard(&mut self, save: bool, tx: &mut Tx) {
        self.dialog = Dialog::None;
        if save {
            self.save(tx);
            if self.dirty() {
                self.pending = None;
                return;
            }
        } else {
            let disk = self.doc.disk.clone();
            self.doc.edit = TextEdit::new(disk);
        }
        if let Some(p) = self.pending.take() {
            self.run(p, tx);
        }
    }

    pub fn close_dialog(&mut self) {
        self.dialog = Dialog::None;
        self.select_open = false;
        self.pending = None;
    }

    pub fn select_scope(&mut self, root: Option<PathBuf>, tx: &mut Tx) {
        if root == self.scope && self.skill_path.is_none() {
            return;
        }
        self.guard(Pending::Scope(root), tx);
    }

    pub fn show_scope(&mut self, root: Option<PathBuf>) {
        self.scope = root;
        self.skill_query = TextEdit::new("");
        self.close_skill();
        self.watch();
    }

    pub fn visible_projects(&self) -> Vec<&skills::Project> {
        let Some(snap) = &self.snap else {
            return Vec::new();
        };
        let q = self.proj_query.text.trim().to_lowercase();
        snap.projects
            .iter()
            .filter(|p| {
                q.is_empty()
                    || p.info.name.to_lowercase().contains(&q)
                    || p.info.root.to_string_lossy().to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Whether the skill filter lets a skill show.
    pub fn matches(&self, s: &Skill) -> bool {
        let q = self.skill_query.text.trim().to_lowercase();
        q.is_empty()
            || s.name.to_lowercase().contains(&q)
            || s.meta.description.to_lowercase().contains(&q)
    }

    pub fn select_skill(&mut self, path: &Path, tx: &mut Tx) {
        if self.skill_path.as_deref() != Some(path) {
            self.guard(Pending::Skill(path.to_path_buf()), tx);
        }
    }

    pub fn show_skill(&mut self, path: &Path) {
        self.close_skill();
        self.skill_path = Some(path.to_path_buf());
        self.refresh_files();
        self.open_file(self.default_file().as_deref());
        self.watch();
    }

    /// Shows another skill, in its scope.
    pub fn go_to(&mut self, path: &Path, tx: &mut Tx) {
        self.guard(Pending::GoTo(path.to_path_buf()), tx);
    }

    fn show_elsewhere(&mut self, path: &Path) {
        let Some(s) = self.snap.as_ref().and_then(|snap| snap.find(path)) else {
            return;
        };
        self.scope = (s.scope == skills::Scope::Project).then(|| s.base.clone());
        self.skill_query = TextEdit::new("");
        self.tab = Tab::Preview;
        self.show_skill(path);
    }

    pub fn close_skill(&mut self) {
        self.skill_path = None;
        self.files.clear();
        self.menu_open = false;
        self.file_menu = None;
        self.close_file();
    }

    pub fn close_file(&mut self) {
        self.doc = Doc::default();
    }

    pub fn default_file(&self) -> Option<PathBuf> {
        if let Some(s) = self.skill().filter(|s| s.has_skill_md) {
            return Some(s.skill_md());
        }
        self.files
            .iter()
            .find(|f| !f.is_dir)
            .map(|f| f.path.clone())
    }

    /// Lists the files of the skill shown again, and takes in what changed on disk in the file
    /// open.
    pub fn refresh_files(&mut self) {
        let Some(s) = self.skill() else {
            self.files.clear();
            return;
        };
        if s.real_path.is_none() {
            self.files.clear();
            self.close_file();
            return;
        }
        self.files = skills::list_files(&s.path).unwrap_or_default();
        let Some(path) = self.doc.path.clone() else {
            return;
        };
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(_) => {
                if self.dirty() {
                    self.doc.changed = true; // keep the edits; saving writes the file again
                } else {
                    self.close_file();
                }
                return;
            }
        };
        if !self.doc.editable {
            return;
        }
        let text = String::from_utf8_lossy(&data);
        if text == self.doc.disk {
            return;
        }
        if self.dirty() {
            self.doc.changed = true;
            return;
        }
        self.doc.disk = text.into_owned();
        self.doc.edit = keep_caret(&self.doc.edit, &self.doc.disk);
    }

    /// Shows a file of the skill in the editor.
    pub fn open_file(&mut self, path: Option<&Path>) {
        self.close_file();
        let Some(path) = path else { return };
        self.doc.path = Some(path.to_path_buf());
        match read_text(path) {
            Ok(text) => {
                self.doc.edit = TextEdit::new(text.clone());
                self.doc.edit.set_caret(0);
                self.doc.disk = text;
                self.doc.editable = true;
            }
            Err(note) => self.doc.note = note,
        }
    }

    /// Writes the editor's text to its file.
    pub fn save(&mut self, tx: &mut Tx) {
        let Some(path) = self.doc.path.clone().filter(|_| self.doc.editable) else {
            return;
        };
        let text = self.doc.edit.text.clone();
        if let Err(e) = skills::write_file(&path, text.as_bytes()) {
            return self.fail(format!("Couldn't save {}", base_name(&path)), e);
        }
        self.doc.disk = text;
        self.doc.changed = false;
        self.notify(tx, format!("Saved {}", base_name(&path)));
        self.reload();
    }

    /// Puts back what the file holds on disk.
    pub fn revert(&mut self) {
        if let Some(path) = self.doc.path.clone() {
            self.open_file(Some(&path));
        }
    }

    /// A link of the Markdown preview: web links open in the browser, links to the skill's
    /// files open them in the editor.
    pub fn follow_link(&mut self, url: &str, tx: &mut Tx) {
        if ["http://", "https://", "mailto:"]
            .iter()
            .any(|p| url.starts_with(p))
        {
            if let Err(e) = tx.open_url(url) {
                self.fail("Couldn't open the link", e);
            }
            return;
        }
        if url.starts_with('#') {
            return;
        }
        let Some(s) = self.skill().filter(|s| s.real_path.is_some()) else {
            return;
        };
        let rel = url.split('#').next().unwrap_or(url);
        let rel = rel.strip_prefix("./").unwrap_or(rel);
        let p = skills::clean(&s.path.join(rel));
        if !skills::within(&s.path, &p) {
            return;
        }
        if !p.exists() {
            return self.notify(tx, format!("No file {rel} in the skill"));
        }
        self.guard(Pending::File(p), tx);
    }

    pub fn fail(&mut self, title: impl Into<String>, err: impl std::fmt::Display) {
        self.pending = None;
        self.dialog = Dialog::Error {
            title: title.into(),
            text: err.to_string(),
        };
    }

    pub fn notify(&mut self, tx: &mut Tx, text: impl Into<String>) {
        let due = tx.clock() + NOTICE_SECS;
        self.notice = Some(Notice {
            text: text.into(),
            due,
        });
        tx.wake_at(due);
    }

    pub fn copy(&mut self, path: &Path, tx: &mut Tx) {
        tx.copy(path.to_string_lossy());
        let shown = self.tilde(path);
        self.notify(tx, format!("Copied {shown}"));
    }

    pub fn reveal(&self, path: &Path) {
        (self.sys.reveal)(path);
    }

    /// Writes a path under the home directory with `~`, the home directory as written or with
    /// its links resolved.
    pub fn tilde(&self, p: &Path) -> String {
        for home in [&self.sys.home, &self.real_home] {
            if let Ok(rest) = p.strip_prefix(home) {
                return if rest.as_os_str().is_empty() {
                    "~".into()
                } else {
                    format!("~/{}", rest.display())
                };
            }
        }
        p.display().to_string()
    }

    /// A path the user typed, with `~/` for the home directory.
    pub fn expand(&self, typed: &str) -> PathBuf {
        let typed = typed.trim();
        match typed.strip_prefix("~/") {
            Some(rest) => self.sys.home.join(rest),
            None if typed == "~" => self.sys.home.clone(),
            None => PathBuf::from(typed),
        }
    }

    /// The window is closing: with edits not saved, asks first and says to stay open.
    pub fn confirm_close(&mut self) -> bool {
        if !self.dirty() {
            return true;
        }
        self.pending = Some(Pending::Quit);
        self.dialog = Dialog::Discard;
        false
    }
}

/// A field that takes the keyboard when it shows.
pub fn focused(text: &str) -> TextEdit {
    let mut e = TextEdit::new(text);
    e.set_selection(0, text.len());
    e.focused = true;
    e
}

/// A new editor for `text`, with the caret and focus the old one had.
fn keep_caret(old: &TextEdit, text: &str) -> TextEdit {
    let mut e = TextEdit::new(text);
    let mut at = old.focus.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    e.set_caret(at);
    e.focused = old.focused;
    e
}

/// A file's text, or why it is not shown in the editor.
fn read_text(path: &Path) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return Err("A folder.".into());
    }
    if meta.len() > MAX_EDIT_SIZE {
        return Err(format!("Too large to edit here ({}).", size(meta.len())));
    }
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if !skills::is_text(&data) {
        return Err(format!("A binary file ({}).", size(meta.len())));
    }
    String::from_utf8(data).map_err(|e| e.to_string())
}

pub fn size(n: u64) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KB", n as f64 / 1024.0),
        _ => format!("{:.1} MB", n as f64 / 1_048_576.0),
    }
}

pub fn base_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The system's own hooks.
impl Sys {
    pub fn real() -> Option<Sys> {
        let dirs = rune::dirs::AppDirs::new("SkillHub")?;
        let home = std::env::var_os("SKILLHUB_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs.home().map(Path::to_path_buf))?;
        let config = std::env::var_os("SKILLHUB_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs.config().to_path_buf());
        Some(Sys {
            home,
            config_path: config.join("config.json"),
            trash: Rc::new(|p: &Path| trash::delete(p).map_err(io::Error::other)),
            reveal: Rc::new(reveal),
            background: !rune::is_headless(),
        })
    }
}

/// Shows a path in Finder, Explorer or the desktop's file manager.
fn reveal(path: &Path) {
    use std::process::Command;
    let spawned = if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else {
        Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
    };
    // The child is the file manager's launcher: it ends on its own.
    if let Ok(mut child) = spawned {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
