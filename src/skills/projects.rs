//! Projects: the folders under the scan roots that hold skills, and everything read of them.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::paths::within;
use super::{Location, Scope, Skill, has_any_location, read_scope};

/// A directory holding skills of at least one agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub name: String,
    /// The folder whose scan found it.
    pub scan_root: PathBuf,
}

/// Directories never searched for projects: dependencies, builds and caches, which are large
/// and hold no projects of the user's.
const SKIP_DIRS: [&str; 15] = [
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    "__pycache__",
    "venv",
    "Library",
    "Applications",
    "Pictures",
    "Music",
    "Movies",
    "go",
    "pkg",
];

/// Walks the scan roots, at most `depth` levels below each, for directories with a project
/// skills directory. It does not follow links to directories, nor go into hidden ones, nor
/// take `home` itself as a project (its skills are the global ones). Projects found under
/// several roots are listed once.
pub fn find_projects(roots: &[PathBuf], depth: usize, home: &Path) -> Vec<ProjectInfo> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let root = super::clean(root);
        if root.is_dir() {
            walk(&root, 0, depth, home, &root, &mut seen, &mut out);
        }
    }
    out.sort_by(|a, b| (a.name.to_lowercase(), &a.root).cmp(&(b.name.to_lowercase(), &b.root)));
    out
}

fn walk(
    dir: &Path,
    level: usize,
    depth: usize,
    home: &Path,
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<ProjectInfo>,
) {
    if dir != home && has_any_location(dir) {
        let key = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if seen.insert(key) {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(ProjectInfo {
                root: dir.to_path_buf(),
                name,
                scan_root: root.to_path_buf(),
            });
        }
    }
    if level >= depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !SKIP_DIRS.contains(&n.as_str()))
        .collect();
    names.sort();
    for name in names {
        walk(&dir.join(name), level + 1, depth, home, root, seen, out);
    }
}

/// A project with its skills read.
#[derive(Clone, Debug)]
pub struct Project {
    pub info: ProjectInfo,
    pub locations: Vec<Location>,
}

impl Project {
    /// The number of skills in the project's locations.
    pub fn count(&self) -> usize {
        count(&self.locations)
    }
}

fn count(ls: &[Location]) -> usize {
    ls.iter().map(|l| l.skills.len()).sum()
}

/// Everything the app knows: the global locations and the projects found, with their skills.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub global: Vec<Location>,
    pub projects: Vec<Project>,
}

/// Reads the global locations and every project's.
pub fn load(home: &Path, projects: Vec<ProjectInfo>) -> Snapshot {
    Snapshot {
        global: read_scope(Scope::Global, home),
        projects: projects
            .into_iter()
            .map(|info| Project {
                locations: read_scope(Scope::Project, &info.root),
                info,
            })
            .collect(),
    }
}

impl Snapshot {
    pub fn global_count(&self) -> usize {
        count(&self.global)
    }

    pub fn project(&self, root: &Path) -> Option<&Project> {
        self.projects.iter().find(|p| p.info.root == root)
    }

    /// The locations of a scope: global for `None`, else the project's.
    pub fn locations(&self, root: Option<&Path>) -> &[Location] {
        match root {
            None => &self.global,
            Some(root) => self.project(root).map_or(&[], |p| p.locations.as_slice()),
        }
    }

    /// Every skill known.
    pub fn all(&self) -> impl Iterator<Item = &Skill> {
        self.global
            .iter()
            .chain(self.projects.iter().flat_map(|p| &p.locations))
            .flat_map(|l| &l.skills)
    }

    /// The skill whose entry is at `path`.
    pub fn find(&self, path: &Path) -> Option<&Skill> {
        self.all().find(|s| s.path == path)
    }

    /// The links among the skills known whose real directory is `real`, other than the entry
    /// at `except`.
    pub fn links_to(&self, real: Option<&Path>, except: &Path) -> Vec<&Skill> {
        let Some(real) = real else { return Vec::new() };
        self.all()
            .filter(|s| s.is_link && s.path != except && s.real_path.as_deref() == Some(real))
            .collect()
    }

    /// The links known whose real directory is `real` or inside it, as those that break when
    /// it goes, other than the entry at `except`.
    pub fn links_into(&self, real: Option<&Path>, except: &Path) -> Vec<&Skill> {
        let Some(real) = real else { return Vec::new() };
        self.all()
            .filter(|s| s.is_link && s.path != except)
            .filter(|s| {
                s.real_path
                    .as_deref()
                    .is_some_and(|r| r == real || within(real, r))
            })
            .collect()
    }

    /// The other skills of the name in the scope of `root` (`None` for global) and the global
    /// scope, which an agent working there sees beside this one.
    pub fn same(&self, skill: &Skill, root: Option<&Path>) -> Vec<&Skill> {
        let scope = self.locations(root).iter();
        let global = if root.is_some() {
            self.global.iter()
        } else {
            [].iter()
        };
        scope
            .chain(global)
            .flat_map(|l| &l.skills)
            .filter(|o| o.path != skill.path && o.name == skill.name)
            .collect()
    }
}
