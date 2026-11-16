//! Paths as text: cleaning and relating them without asking the disk.

use std::path::{Component, Path, PathBuf};

/// `p` without `.` parts, and with each `..` taking back the part before it, as written: links
/// are not followed.
pub fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// Whether `p` is inside `dir`, and not `dir` itself.
pub fn within(dir: &Path, p: &Path) -> bool {
    let (dir, p) = (clean(dir), clean(p));
    p != dir && p.starts_with(&dir)
}

/// `target` as a path relative to `base`, both absolute, as written.
pub fn rel(base: &Path, target: &Path) -> Option<PathBuf> {
    let (base, target) = (clean(base), clean(target));
    if base.is_absolute() != target.is_absolute() {
        return None;
    }
    let b: Vec<Component> = base.components().collect();
    let t: Vec<Component> = target.components().collect();
    let same = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in same..b.len() {
        out.push("..");
    }
    for c in &t[same..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    Some(out)
}
