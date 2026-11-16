//! Finds, reads and changes agent skills on disk: the skill directories of `.agents/skills`,
//! `.claude/skills` and pi's skills, in the home directory (global scope) and in projects
//! (project scope).

mod frontmatter;
mod ops;
mod paths;
mod projects;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

pub use frontmatter::{Frontmatter, parse_frontmatter, set_name, template, validate};
pub use ops::{
    File, Trasher, check_name, check_target, convert_to_link, create_link, create_skill, is_text,
    list_files, materialize, new_file, remove, rename, retarget, write_file,
};
pub use paths::{clean, rel, within};
pub use projects::{Project, ProjectInfo, Snapshot, find_projects, load};

/// The tool whose skills directory a location is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Agent {
    /// `.agents/skills`, shared by agents that read it.
    Shared,
    /// Claude Code's `.claude/skills`.
    Claude,
    /// pi's `~/.pi/agent/skills` (global) and `.pi/skills` (project).
    Pi,
}

/// Every agent, in the order the app shows them.
pub const AGENTS: [Agent; 3] = [Agent::Shared, Agent::Claude, Agent::Pi];

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::Shared => "Agents",
            Agent::Claude => "Claude",
            Agent::Pi => "Pi",
        }
    }
}

/// Where a location's skills apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Global,
    Project,
}

/// The skills directory of an agent relative to the home directory (global) or a project's root.
pub fn rel_dir(a: Agent, s: Scope) -> &'static str {
    match (a, s) {
        (Agent::Shared, _) => ".agents/skills",
        (Agent::Claude, _) => ".claude/skills",
        (Agent::Pi, Scope::Global) => ".pi/agent/skills",
        (Agent::Pi, Scope::Project) => ".pi/skills",
    }
}

/// A symbolic link met on the way to a path: the link and what it holds, as readlink returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub link: PathBuf,
    pub target: PathBuf,
}

/// A skills directory: one agent's, in one scope.
#[derive(Clone, Debug)]
pub struct Location {
    pub agent: Agent,
    pub scope: Scope,
    /// The home directory (global) or the project's root.
    pub base: PathBuf,
    /// `base` joined with the agent's directory, as written.
    pub dir: PathBuf,
    /// Whether `dir` is a directory (through links).
    pub exists: bool,
    /// The links between `base` and `dir`, as `.claude` being a link, or `skills` itself.
    pub links: Vec<Hop>,
    /// `dir` with every link resolved, when it exists.
    pub real_dir: Option<PathBuf>,
    /// Why the directory could not be read, if it could not.
    pub err: Option<String>,
    pub skills: Vec<Skill>,
}

impl Location {
    /// Names the location as the app lists it, as "Claude · .claude/skills".
    pub fn label(&self) -> String {
        format!(
            "{} · {}",
            self.agent.name(),
            rel_dir(self.agent, self.scope)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// Something wrong with a skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub severity: Severity,
    pub message: String,
}

/// An entry of a skills directory: a directory, or a link to one.
#[derive(Clone, Debug)]
pub struct Skill {
    /// The entry's name, the directory's name.
    pub name: String,
    /// The entry in its location's directory.
    pub path: PathBuf,
    pub agent: Agent,
    pub scope: Scope,
    /// The location's base: the home directory or the project's root.
    pub base: PathBuf,
    /// Whether the entry itself is a symbolic link.
    pub is_link: bool,
    /// Every link from `path` to the real directory, the first being `path` itself when
    /// `is_link`: more than one hop is a link to a link.
    pub chain: Vec<Hop>,
    /// A link whose target does not exist, or loops.
    pub broken: bool,
    /// The directory with every link resolved, `None` when broken.
    pub real_path: Option<PathBuf>,
    pub has_skill_md: bool,
    pub meta: Frontmatter,
    pub problems: Vec<Problem>,
}

impl Skill {
    /// What the entry links to, as written.
    pub fn target(&self) -> Option<&Path> {
        if !self.is_link {
            return None;
        }
        self.chain.first().map(|h| h.target.as_path())
    }

    /// The path of the skill's SKILL.md, through the entry.
    pub fn skill_md(&self) -> PathBuf {
        self.path.join("SKILL.md")
    }

    /// The worst severity among its problems.
    pub fn worst(&self) -> Option<Severity> {
        self.problems.iter().map(|p| p.severity).max()
    }

    fn problem(&mut self, severity: Severity, message: impl Into<String>) {
        self.problems.push(Problem {
            severity,
            message: message.into(),
        });
    }
}

/// Reads the skills directory of an agent under `base`.
pub fn read_location(agent: Agent, scope: Scope, base: &Path) -> Location {
    let rel = rel_dir(agent, scope);
    let mut l = Location {
        agent,
        scope,
        base: base.to_path_buf(),
        dir: base.join(rel),
        exists: false,
        links: links_between(base, rel),
        real_dir: None,
        err: None,
        skills: Vec::new(),
    };
    let meta = match fs::metadata(&l.dir) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                l.err = Some(e.to_string());
            }
            return l;
        }
    };
    if !meta.is_dir() {
        l.err = Some("not a directory".into());
        return l;
    }
    l.exists = true;
    l.real_dir = fs::canonicalize(&l.dir).ok();
    let entries = match fs::read_dir(&l.dir) {
        Ok(e) => e,
        Err(e) => {
            l.err = Some(e.to_string());
            return l;
        }
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue; // .DS_Store, .git and other hidden entries
        }
        if let Some(s) = read_skill(&l, &name) {
            l.skills.push(s);
        }
    }
    l.skills.sort_by(|a, b| a.name.cmp(&b.name));
    l
}

/// Reads the three locations of a scope under `base`.
pub fn read_scope(scope: Scope, base: &Path) -> Vec<Location> {
    AGENTS
        .iter()
        .map(|a| read_location(*a, scope, base))
        .collect()
}

/// Whether `dir` holds a project skills directory of any agent.
pub fn has_any_location(dir: &Path) -> bool {
    AGENTS
        .iter()
        .any(|a| dir.join(rel_dir(*a, Scope::Project)).is_dir())
}

/// Reads one entry, `None` when it is not a skill (a plain file).
fn read_skill(l: &Location, name: &str) -> Option<Skill> {
    let p = l.dir.join(name);
    let meta = fs::symlink_metadata(&p).ok()?;
    let mut s = Skill {
        name: name.to_owned(),
        path: p.clone(),
        agent: l.agent,
        scope: l.scope,
        base: l.base.clone(),
        is_link: false,
        chain: Vec::new(),
        broken: false,
        real_path: None,
        has_skill_md: false,
        meta: Frontmatter::default(),
        problems: Vec::new(),
    };
    if meta.file_type().is_symlink() {
        s.is_link = true;
        let (chain, last, ok) = follow_chain(&p);
        let hops = chain.len();
        s.chain = chain;
        if !ok {
            s.broken = true;
            if hops >= MAX_HOPS {
                s.problem(
                    Severity::Error,
                    "The link loops, or goes through too many links.",
                );
            } else {
                s.problem(
                    Severity::Error,
                    format!("Broken link: {} does not exist.", last.display()),
                );
            }
            return Some(s);
        }
        if hops > 1 {
            s.problem(
                Severity::Warning,
                format!("Links to another link; it goes through {hops} links."),
            );
        }
    }
    let real = match fs::canonicalize(&p) {
        Ok(r) => r,
        Err(e) => {
            s.broken = s.is_link;
            s.problem(Severity::Error, e.to_string());
            return Some(s);
        }
    };
    match fs::metadata(&real) {
        Ok(m) if !m.is_dir() => return None, // a file in the skills directory, not a skill
        Ok(_) => {}
        Err(e) => {
            s.problem(Severity::Error, e.to_string());
            return Some(s);
        }
    }
    let data = fs::read(real.join("SKILL.md"));
    s.real_path = Some(real);
    match data {
        Ok(data) => {
            s.has_skill_md = true;
            s.meta = parse_frontmatter(&String::from_utf8_lossy(&data));
            s.problems.extend(validate(name, &s.meta));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            s.problem(Severity::Error, "No SKILL.md in the folder.")
        }
        Err(e) => s.problem(Severity::Error, format!("Cannot read SKILL.md: {e}")),
    }
    Some(s)
}

pub const MAX_HOPS: usize = 40;

/// Follows the links from `p`, one hop at a time, returning each link met and the last path
/// reached; the flag is false when that path does not exist or the links loop. A link in a
/// parent directory of a hop is followed by the system, not listed.
pub fn follow_chain(p: &Path) -> (Vec<Hop>, PathBuf, bool) {
    let mut chain = Vec::new();
    let mut cur = p.to_path_buf();
    for _ in 0..MAX_HOPS {
        let Ok(meta) = fs::symlink_metadata(&cur) else {
            return (chain, cur, false);
        };
        if !meta.file_type().is_symlink() {
            return (chain, cur, true);
        }
        let Ok(target) = fs::read_link(&cur) else {
            return (chain, cur, false);
        };
        chain.push(Hop {
            link: cur.clone(),
            target: target.clone(),
        });
        let next = if target.is_absolute() {
            target
        } else {
            cur.parent().unwrap_or(Path::new("/")).join(target)
        };
        cur = clean(&next);
    }
    (chain, cur, false)
}

/// The links among the directories from `base` down to `base/rel`, not counting `base` itself.
fn links_between(base: &Path, rel: &str) -> Vec<Hop> {
    let mut hops = Vec::new();
    let mut cur = base.to_path_buf();
    for part in rel.split('/') {
        cur.push(part);
        let Ok(meta) = fs::symlink_metadata(&cur) else {
            break;
        };
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&cur).unwrap_or_default();
            hops.push(Hop {
                link: cur.clone(),
                target,
            });
        }
    }
    hops
}
