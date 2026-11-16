//! Actions on skills. Each opens its dialog, or changes the disk and reads it again.

use std::fs;
use std::path::{Path, PathBuf};

use rune::{FileDialogOptions, TextEdit, Tx};

use crate::app::{App, Dialog, Pending, Tab, base_name, focused, plural};
use crate::skills::{self, AGENTS, Agent, File, Location, Scope};

impl App {
    // Opening dialogs.

    pub fn open_new_skill(&mut self) {
        // Where the scope already keeps skills, else the shared folder.
        let agent = self
            .snap
            .as_ref()
            .and_then(|snap| {
                snap.locations(self.scope())
                    .iter()
                    .find(|l| l.exists)
                    .map(|l| l.agent)
            })
            .unwrap_or(Agent::Shared);
        self.dialog = Dialog::NewSkill {
            agent,
            name: focused(""),
            desc: TextEdit::new(""),
        };
    }

    pub fn open_rename(&mut self) {
        if let Some(name) = self.skill().map(|s| s.name.clone()) {
            self.dialog = Dialog::Rename {
                name: focused(&name),
                meta: true,
                relink: true,
            };
        }
    }

    pub fn open_link_into(&mut self) {
        if let Some(name) = self.skill().map(|s| s.name.clone()) {
            self.dialog = Dialog::LinkInto {
                scope: self.scope.clone(),
                agents: [false; 3],
                name: TextEdit::new(name),
            };
        }
    }

    pub fn open_retarget(&mut self) {
        let Some(s) = self.skill() else { return };
        // Where it leads now, or for a broken link where it says it does.
        let target = s.real_path.clone().or_else(|| {
            let t = s.target()?;
            Some(if t.is_absolute() {
                skills::clean(t)
            } else {
                skills::clean(&s.path.parent()?.join(t))
            })
        });
        let shown = target.map(|t| self.tilde(&t)).unwrap_or_default();
        self.dialog = Dialog::Retarget {
            path: focused(&shown),
        };
    }

    pub fn open_convert(&mut self) {
        let dests = self.convert_destinations();
        let here = self
            .skill()
            .and_then(|s| s.path.parent().map(Path::to_path_buf));
        // The first shared place the skill isn't in already.
        let dest = dests
            .iter()
            .find(|d| Some(*d) != here.as_ref())
            .unwrap_or(&dests[0])
            .clone();
        self.dialog = Dialog::Convert { dest };
    }

    pub fn open_new_file(&mut self, tx: &mut Tx) {
        self.guard(Pending::NewFile, tx);
    }

    /// Asks for a folder to scan.
    pub fn pick_root(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions {
            title: "Choose a folder to scan".into(),
            directory: Some(self.sys.home.clone()),
            ..Default::default()
        };
        tx.open_folder(options, |s: &mut App, picked, _| match picked {
            Ok(Some(path)) => s.cfg.add_root(&path),
            Ok(None) => {}
            Err(e) => s.fail("Couldn't open the folder dialog", e),
        });
    }

    /// Asks for the folder a link points to.
    pub fn pick_target(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions {
            title: "Choose the skill folder to link to".into(),
            directory: Some(self.sys.home.clone()),
            ..Default::default()
        };
        tx.open_folder(options, |s: &mut App, picked, _| match picked {
            Ok(Some(picked)) => {
                let shown = s.tilde(&picked);
                if let Dialog::Retarget { path } = &mut s.dialog {
                    *path = focused(&shown);
                }
            }
            Ok(None) => {}
            Err(e) => s.fail("Couldn't open the folder dialog", e),
        });
    }

    /// Asks for another folder to move a skill to.
    pub fn pick_convert_dest(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions {
            title: "Choose where to move the skill".into(),
            directory: Some(self.sys.home.clone()),
            ..Default::default()
        };
        tx.open_folder(options, |s: &mut App, picked, _| match picked {
            Ok(Some(picked)) => {
                if let Dialog::Convert { dest } = &mut s.dialog {
                    *dest = picked;
                }
            }
            Ok(None) => {}
            Err(e) => s.fail("Couldn't open the folder dialog", e),
        });
    }

    // Folders to scan.

    /// Leaves the first-run page, or the folders dialog: saves them and scans.
    pub fn save_folders(&mut self) {
        self.welcome = false;
        self.dialog = Dialog::None;
        self.save_config();
        self.scan();
    }

    // Skills.

    /// The folder a new skill of `agent` goes to in the scope shown.
    pub fn location_dir(&self, agent: Agent) -> PathBuf {
        match self.scope() {
            Some(root) => root.join(skills::rel_dir(agent, Scope::Project)),
            None => self.sys.home.join(skills::rel_dir(agent, Scope::Global)),
        }
    }

    pub fn create_skill(&mut self, tx: &mut Tx) {
        let Dialog::NewSkill { agent, name, desc } = &self.dialog else {
            return;
        };
        let (dir, name, desc) = (
            self.location_dir(*agent),
            name.text.trim().to_owned(),
            desc.text.trim().to_owned(),
        );
        let path = match skills::create_skill(&dir, &name, &desc) {
            Ok(p) => p,
            Err(e) => return self.fail("Couldn't create the skill", e),
        };
        self.dialog = Dialog::None;
        self.reload();
        self.show_skill(&path);
        self.tab = Tab::Files;
        self.doc.edit.focused = true;
        self.notify(tx, format!("Created {name}"));
    }

    pub fn rename_skill(&mut self, tx: &mut Tx) {
        let Dialog::Rename { name, meta, relink } = &self.dialog else {
            return;
        };
        let (name, meta, relink) = (name.text.trim().to_owned(), *meta, *relink);
        let (Some(s), Some(snap)) = (self.skill().cloned(), &self.snap) else {
            return;
        };
        // The links that name this folder, rather than reach it through another link.
        let mut dependents = Vec::new();
        if !s.is_link && relink {
            for l in snap.links_to(s.real_path.as_deref(), &s.path) {
                let Some(target) = l.target() else { continue };
                let abs = if target.is_absolute() {
                    target.to_path_buf()
                } else {
                    l.path.parent().unwrap_or(Path::new("/")).join(target)
                };
                let abs = skills::clean(&abs);
                if abs == s.path || Some(&abs) == s.real_path.as_ref() {
                    dependents.push((l.path.clone(), target.is_absolute()));
                }
            }
        }
        let new_path = match skills::rename(&s.path, &name) {
            Ok(p) => p,
            Err(e) => return self.fail("Couldn't rename the skill", e),
        };
        let mut errors = Vec::new();
        if !s.is_link && meta && s.has_skill_md {
            let md = new_path.join("SKILL.md");
            if let Some(doc) = fs::read_to_string(&md)
                .ok()
                .and_then(|doc| skills::set_name(&doc, &name))
            {
                errors.extend(skills::write_file(&md, doc.as_bytes()).err());
            }
        }
        for (link, absolute) in dependents {
            // Relative links stay relative.
            let relative = link
                .parent()
                .and_then(|dir| skills::rel(dir, &new_path))
                .filter(|_| !absolute);
            errors.extend(
                skills::retarget(&link, &relative.unwrap_or_else(|| new_path.clone())).err(),
            );
        }
        self.dialog = Dialog::None;
        self.close_file();
        self.skill_path = Some(new_path);
        self.reload();
        self.open_file(self.default_file().as_deref());
        if errors.is_empty() {
            self.notify(tx, format!("Renamed to {name}"));
        } else {
            let text = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            self.fail("Renamed, but not everything was updated", text);
        }
    }

    /// The locations a skill can be linked into from a scope.
    pub fn link_locations(&self, scope: Option<&Path>) -> &[Location] {
        self.snap.as_ref().map_or(&[], |snap| snap.locations(scope))
    }

    pub fn create_links(&mut self, tx: &mut Tx) {
        let Dialog::LinkInto {
            scope,
            agents,
            name,
        } = &self.dialog
        else {
            return;
        };
        let (name, agents) = (name.text.trim().to_owned(), *agents);
        let Some(real) = self.skill().and_then(|s| s.real_path.clone()) else {
            return;
        };
        let dirs: Vec<(String, PathBuf)> = self
            .link_locations(scope.as_deref())
            .iter()
            .map(|l| (l.label(), l.dir.clone()))
            .collect();
        let (mut made, mut errors) = (Vec::new(), Vec::new());
        for (i, (label, dir)) in dirs.iter().enumerate() {
            if !agents[i] {
                continue;
            }
            match skills::create_link(dir, &name, &real) {
                Ok(p) => made.push(self.tilde(&p)),
                Err(e) => errors.push(format!("{label}: {e}")),
            }
        }
        if made.is_empty() && errors.is_empty() {
            return self.fail("No link created", "choose where to link the skill");
        }
        self.dialog = Dialog::None;
        self.reload();
        if !made.is_empty() {
            self.notify(tx, format!("Linked into {}", made.join(", ")));
        }
        if !errors.is_empty() {
            self.fail("Couldn't create every link", errors.join("\n"));
        }
    }

    pub fn retarget(&mut self, tx: &mut Tx) {
        let Dialog::Retarget { path } = &self.dialog else {
            return;
        };
        let target = skills::clean(&self.expand(&path.text));
        let Some(link) = self.skill().map(|s| s.path.clone()) else {
            return;
        };
        if let Err(e) = skills::check_target(&target) {
            return self.fail("Couldn't change the link", e);
        }
        if let Err(e) = skills::retarget(&link, &target) {
            return self.fail("Couldn't change the link", e);
        }
        self.dialog = Dialog::None;
        self.reopen();
        let shown = self.tilde(&target);
        self.notify(tx, format!("The link now points to {shown}"));
    }

    /// Reads the skill shown again after its folder changed under it.
    fn reopen(&mut self) {
        self.close_file();
        self.reload();
        self.open_file(self.default_file().as_deref());
    }

    pub fn materialize(&mut self, tx: &mut Tx) {
        self.dialog = Dialog::None;
        let Some(s) = self.skill().cloned() else {
            return;
        };
        if let Err(e) = skills::materialize(&s.path) {
            return self.fail("Couldn't make a copy", e);
        }
        self.reopen();
        self.notify(tx, format!("{} is now a folder of its own", s.name));
    }

    /// The folders a skill can be moved to, to share.
    pub fn convert_destinations(&self) -> Vec<PathBuf> {
        AGENTS
            .iter()
            .map(|a| self.sys.home.join(skills::rel_dir(*a, Scope::Global)))
            .collect()
    }

    pub fn convert(&mut self, tx: &mut Tx) {
        let Dialog::Convert { dest } = &self.dialog else {
            return;
        };
        let dest = dest.clone();
        let Some(path) = self.skill().map(|s| s.path.clone()) else {
            return;
        };
        let moved = match skills::convert_to_link(&path, &dest) {
            Ok(p) => p,
            Err(e) => return self.fail("Couldn't move the skill", e),
        };
        self.dialog = Dialog::None;
        self.reopen();
        let shown = self.tilde(&moved);
        self.notify(tx, format!("Moved to {shown} and linked here"));
    }

    pub fn delete_skill(&mut self, tx: &mut Tx) {
        self.dialog = Dialog::None;
        let Some(s) = self.skill().cloned() else {
            return;
        };
        if let Err(e) = skills::remove(&s.path, Some(&*self.sys.trash)) {
            return self.fail(format!("Couldn't remove {}", s.name), e);
        }
        self.close_skill();
        self.reload();
        let what = if s.is_link {
            format!("Removed the link {}", s.name)
        } else {
            format!("Moved {} to the Trash", s.name)
        };
        self.notify(tx, what);
    }

    // Files of a skill.

    pub fn create_file(&mut self) {
        let Dialog::NewFile { rel } = &self.dialog else {
            return;
        };
        let Some(dir) = self.skill().map(|s| s.path.clone()) else {
            return;
        };
        let path = match skills::new_file(&dir, &rel.text) {
            Ok(p) => p,
            Err(e) => return self.fail("Couldn't create the file", e),
        };
        self.dialog = Dialog::None;
        self.refresh_files();
        self.open_file(Some(&path));
        self.doc.edit.focused = true;
    }

    pub fn remove_file(&mut self, tx: &mut Tx) {
        let Dialog::DeleteFile(f) = std::mem::replace(&mut self.dialog, Dialog::None) else {
            return;
        };
        let removed = if f.is_link {
            fs::remove_file(&f.path)
        } else {
            (self.sys.trash)(&f.path)
        };
        if let Err(e) = removed {
            return self.fail(format!("Couldn't remove {}", f.rel), e);
        }
        if self
            .doc
            .path
            .as_deref()
            .is_some_and(|open| open == f.path || skills::within(&f.path, open))
        {
            self.close_file();
        }
        self.reload();
        if self.doc.path.is_none() {
            self.open_file(self.default_file().as_deref());
        }
        self.notify(tx, format!("Removed {}", f.rel));
    }

    // What the dialogs say before they act.

    /// What stops a new skill of this name in the folder of `agent`.
    pub fn new_skill_error(&self, agent: Agent, name: &str) -> Option<String> {
        let name = name.trim();
        if let Err(e) = skills::check_name(name) {
            return Some(e);
        }
        let at = self.location_dir(agent).join(name);
        fs::symlink_metadata(&at)
            .is_ok()
            .then(|| format!("{} already exists", self.tilde(&at)))
    }

    /// What stops the rename.
    pub fn rename_error(&self, name: &str) -> Option<String> {
        let name = name.trim();
        if let Err(e) = skills::check_name(name) {
            return Some(e);
        }
        (self.skill().is_some_and(|s| s.name == name)).then(|| "enter a new name".into())
    }

    /// What a link named `name` in `l` would meet: the skill itself, or an entry of the name.
    pub fn link_blocker(&self, l: &Location, name: &str) -> Option<&'static str> {
        let real = self.skill()?.real_path.as_deref()?;
        let here = l
            .real_dir
            .as_deref()
            .is_some_and(|dir| real.parent() == Some(dir))
            && real.file_name().is_some_and(|n| n == name);
        if here {
            Some("the skill is here")
        } else if fs::symlink_metadata(l.dir.join(name)).is_ok() {
            Some("exists")
        } else {
            None
        }
    }

    /// Where the skill would move to, and what stops it.
    pub fn convert_plan(&self, dest: &Path) -> Option<(PathBuf, Option<String>)> {
        let s = self.skill()?;
        let to = dest.join(&s.name);
        let err = if fs::symlink_metadata(&to).is_ok() {
            Some(format!("{} already exists", self.tilde(&to)))
        } else if s.path.parent() == Some(skills::clean(dest).as_path()) {
            Some("the skill is already there".into())
        } else {
            None
        };
        Some((to, err))
    }

    /// What removing the skill shown does, for its confirmation.
    pub fn delete_summary(&self) -> Option<(String, String, &'static str)> {
        let (s, snap) = (self.skill()?, self.snap.as_ref()?);
        if s.is_link {
            let target = s
                .target()
                .map(|t| t.display().to_string())
                .unwrap_or_default();
            let text = format!(
                "Only the link at {} is removed. {target} stays.",
                self.tilde(&s.path)
            );
            return Some((
                format!("Remove the link “{}”?", s.name),
                text,
                "Remove Link",
            ));
        }
        let mut text = self.tilde(&s.path);
        let deps = snap.links_into(s.real_path.as_deref(), &s.path);
        if !deps.is_empty() {
            text.push_str(&format!(
                "\n\n{} into it and will break:",
                plural(deps.len(), "link points", "links point")
            ));
            for d in deps {
                text.push_str(&format!("\n{}", self.tilde(&d.path)));
            }
        }
        Some((
            format!("Move “{}” to the Trash?", s.name),
            text,
            "Move to Trash",
        ))
    }

    pub fn delete_file_summary(f: &File) -> (String, String, &'static str) {
        if f.is_link {
            let text = format!("Only the link is removed; {} stays.", f.target.display());
            (format!("Remove the link “{}”?", f.rel), text, "Remove Link")
        } else {
            (
                format!("Move “{}” to the Trash?", f.rel),
                String::new(),
                "Move to Trash",
            )
        }
    }

    /// The file open, as a path inside the skill.
    pub fn doc_rel(&self) -> String {
        let Some(path) = &self.doc.path else {
            return String::new();
        };
        let inside = self.skill().and_then(|s| path.strip_prefix(&s.path).ok());
        inside.map_or_else(
            || base_name(path),
            |r| r.to_string_lossy().replace('\\', "/"),
        )
    }
}
