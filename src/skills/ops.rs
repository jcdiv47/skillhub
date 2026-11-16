//! Changes to skills on disk. Each is one step where the file system allows, and puts back
//! what it moved when a later step fails.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::paths::{clean, within};

/// Moves a file or directory to the system's trash.
pub type Trasher<'a> = dyn Fn(&Path) -> io::Result<()> + 'a;

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

/// Whether `name` can be a skill's folder or a file's name: not empty, no separators, not
/// hidden.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("enter a name".into())
    } else if name != name.trim() {
        Err("the name starts or ends with spaces".into())
    } else if name.contains(['/', '\\']) {
        Err("the name cannot contain / or \\".into())
    } else if name == "." || name == ".." {
        Err("that name is reserved".into())
    } else if name.starts_with('.') {
        Err("hidden names are not listed as skills".into())
    } else {
        Ok(())
    }
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn is_link(p: &Path) -> io::Result<bool> {
    Ok(fs::symlink_metadata(p)?.file_type().is_symlink())
}

fn parent(p: &Path) -> &Path {
    p.parent().unwrap_or(Path::new("/"))
}

fn base_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

/// Removes a link, which on Windows is a directory entry.
fn remove_link(p: &Path) -> io::Result<()> {
    fs::remove_file(p).or_else(|e| {
        if cfg!(windows) {
            fs::remove_dir(p)
        } else {
            Err(e)
        }
    })
}

/// Makes a skill folder named `name` in `dir`, creating `dir` when it does not exist, with a
/// SKILL.md for it. Returns the folder's path.
pub fn create_skill(dir: &Path, name: &str, description: &str) -> io::Result<PathBuf> {
    check_name(name).map_err(err)?;
    let p = dir.join(name);
    if exists(&p) {
        return Err(err(format!("{} already exists", p.display())));
    }
    fs::create_dir_all(&p)?;
    fs::write(p.join("SKILL.md"), super::template(name, description))?;
    Ok(p)
}

static TEMP: AtomicU32 = AtomicU32::new(0);

/// Writes `data` to `path` as one change: to a new file beside it, renamed over it. A path
/// that is a link, or under one, is written at its real place, so the link stays a link. The
/// file keeps its mode.
pub fn write_file(path: &Path, data: &[u8]) -> io::Result<()> {
    let real = match fs::canonicalize(path) {
        Ok(r) => r,
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        Err(_) => match fs::canonicalize(parent(path)) {
            Ok(dir) => dir.join(path.file_name().unwrap_or_default()),
            Err(_) => path.to_path_buf(),
        },
    };
    let perms = fs::metadata(&real).ok().map(|m| m.permissions());
    let tmp = parent(&real).join(format!(
        ".{}.skillhub-{}-{}",
        base_name(&real),
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let write = || -> io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(data)?;
        if let Some(perms) = perms {
            f.set_permissions(perms)?;
        }
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &real)
    };
    write().inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Makes a link named `name` in `dir` to `target`, an absolute path, creating `dir` when it
/// does not exist. Returns the link's path.
pub fn create_link(dir: &Path, name: &str, target: &Path) -> io::Result<PathBuf> {
    check_name(name).map_err(err)?;
    if !target.is_absolute() {
        return Err(err("the target must be an absolute path"));
    }
    let p = dir.join(name);
    if exists(&p) {
        return Err(err(format!("{} already exists", p.display())));
    }
    if same(dir, target) || within(target, dir) {
        return Err(err("a skill cannot be linked into itself"));
    }
    fs::create_dir_all(dir)?;
    symlink(target, &p)?;
    Ok(p)
}

fn same(a: &Path, b: &Path) -> bool {
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// What is wrong with a directory as a skill's target: `Ok(None)` for a directory with
/// SKILL.md, a warning for a directory without, an error otherwise.
pub fn check_target(target: &Path) -> Result<Option<&'static str>, String> {
    if !target.is_absolute() {
        return Err("enter an absolute path".into());
    }
    match fs::metadata(target) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err("the folder does not exist".into()),
        Err(e) => Err(e.to_string()),
        Ok(m) if !m.is_dir() => Err("not a folder".into()),
        Ok(_) if !exists(&target.join("SKILL.md")) => Ok(Some("The folder has no SKILL.md.")),
        Ok(_) => Ok(None),
    }
}

/// Points the link at `path` to `target`, replacing it as one change. A relative target is
/// relative to the link's folder, as in a link.
pub fn retarget(path: &Path, target: &Path) -> io::Result<()> {
    if !is_link(path)? {
        return Err(err("not a link"));
    }
    let dir = parent(path);
    let abs = if target.is_absolute() {
        target.to_path_buf()
    } else {
        clean(&dir.join(target))
    };
    if within(&abs, dir) || same(dir, &abs) {
        return Err(err("a link cannot point to the folder it is in"));
    }
    let tmp = dir.join(format!(".{}.skillhub-link", base_name(path)));
    let _ = remove_link(&tmp);
    symlink(target, &tmp)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = remove_link(&tmp);
    })
}

/// Replaces the link at `path` with a copy of the directory it points to. Links inside the
/// directory are copied as links.
pub fn materialize(path: &Path) -> io::Result<()> {
    if !is_link(path)? {
        return Err(err("not a link"));
    }
    let target = fs::read_link(path)?;
    let real = fs::canonicalize(path).map_err(|e| err(format!("the link is broken: {e}")))?;
    let tmp = parent(path).join(format!(".{}.skillhub-copy", base_name(path)));
    let _ = fs::remove_dir_all(&tmp);
    if let Err(e) = copy_dir(&real, &tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = remove_link(path) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = symlink(&target, path); // put the link back
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Moves the directory at `path` to `dir`, under the same name, and leaves a link to it at
/// `path`. Returns the directory's new path.
pub fn convert_to_link(path: &Path, dir: &Path) -> io::Result<PathBuf> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(err("only a real folder can be moved and linked"));
    }
    if !dir.is_absolute() {
        return Err(err("the destination must be an absolute path"));
    }
    if same(parent(path), dir) {
        return Err(err("the folder is already there"));
    }
    if within(path, dir) || same(path, dir) {
        return Err(err("cannot move a folder into itself"));
    }
    let dest = dir.join(path.file_name().unwrap_or_default());
    if exists(&dest) {
        return Err(err(format!("{} already exists", dest.display())));
    }
    fs::create_dir_all(dir)?;
    move_dir(path, &dest)?;
    if let Err(e) = symlink(&dest, path) {
        let _ = move_dir(&dest, path); // put the folder back
        return Err(e);
    }
    Ok(dest)
}

/// Renames, or copies and removes across file systems.
fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    let Err(e) = fs::rename(from, to) else {
        return Ok(());
    };
    if copy_dir(from, to).is_err() {
        let _ = fs::remove_dir_all(to);
        return Err(e);
    }
    fs::remove_dir_all(from)
}

#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_: &fs::Metadata, _: &fs::Metadata) -> bool {
    false
}

/// Renames the entry at `path`, a folder or a link, to `name` in the same directory. Returns
/// the new path.
pub fn rename(path: &Path, name: &str) -> io::Result<PathBuf> {
    check_name(name).map_err(err)?;
    let np = parent(path).join(name);
    if np == path {
        return Ok(np);
    }
    if let Ok(there) = fs::symlink_metadata(&np) {
        // Only a change of case on a case-insensitive disk finds the entry itself.
        let own = fs::symlink_metadata(path);
        if !own.is_ok_and(|own| same_file(&there, &own)) {
            return Err(err(format!("{} already exists", np.display())));
        }
    }
    fs::rename(path, &np)?;
    Ok(np)
}

/// Removes a skill's entry: only the link when it is one, otherwise the folder, to the trash.
pub fn remove(path: &Path, trash: Option<&Trasher<'_>>) -> io::Result<()> {
    if is_link(path)? {
        return remove_link(path);
    }
    match trash {
        Some(trash) => trash(path),
        None => Err(err("no trash to move the folder to")),
    }
}

/// Copies the directory `from` to `to`, which must not exist. Links are copied as links, files
/// with their modes.
pub fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        return symlink(&fs::read_link(from)?, to);
    }
    if meta.is_dir() {
        fs::create_dir(to)?;
        let mut entries: Vec<_> = fs::read_dir(from)?.collect::<io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            copy_dir(&e.path(), &to.join(e.file_name()))?;
        }
        return owner_can_use(to, &meta);
    }
    if meta.is_file() {
        if exists(to) {
            return Err(err(format!("{} already exists", to.display())));
        }
        fs::copy(from, to)?;
    }
    Ok(()) // sockets, devices: left out
}

/// Gives a copied directory its source's mode, with the owner able to read, write and enter it.
#[cfg(unix)]
fn owner_can_use(dir: &Path, source: &fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        dir,
        fs::Permissions::from_mode(source.permissions().mode() & 0o777 | 0o700),
    )
}

#[cfg(not(unix))]
fn owner_can_use(_: &Path, _: &fs::Metadata) -> io::Result<()> {
    Ok(())
}

/// A file inside a skill's folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct File {
    /// Its path in the folder, with `/` between parts.
    pub rel: String,
    pub path: PathBuf,
    pub is_link: bool,
    /// What a link holds.
    pub target: PathBuf,
    /// A link that points nowhere.
    pub broken: bool,
    pub is_dir: bool,
    pub size: u64,
    pub depth: usize,
}

/// The most files listed of one skill.
pub const MAX_FILES: usize = 2000;

/// Lists the files and folders inside a skill's folder, through its link: SKILL.md first, then
/// each folder's own folders and files by name, a folder's contents right under it. Links
/// inside are listed, not followed. `.git` and `node_modules` are left out.
pub fn list_files(dir: &Path) -> io::Result<Vec<File>> {
    let root = fs::canonicalize(dir)?;
    let mut files = Vec::new();
    walk(&root, dir, "", 0, &mut files);
    Ok(files)
}

fn walk(real: &Path, shown: &Path, prefix: &str, depth: usize, out: &mut Vec<File>) {
    let Ok(entries) = fs::read_dir(real) else {
        return;
    };
    let mut items = Vec::new();
    for e in entries.flatten() {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let Ok(kind) = e.file_type() else { continue };
        if name == ".DS_Store" || (kind.is_dir() && (name == ".git" || name == "node_modules")) {
            continue;
        }
        let mut f = File {
            rel: format!("{prefix}{name}"),
            path: shown.join(&name),
            is_dir: kind.is_dir(),
            depth,
            ..File::default()
        };
        if kind.is_symlink() {
            f.is_link = true;
            f.target = fs::read_link(e.path()).unwrap_or_default();
            match fs::metadata(e.path()) {
                Ok(m) => {
                    f.size = m.len();
                    f.is_dir = m.is_dir();
                }
                Err(_) => f.broken = true,
            }
        } else if !kind.is_dir() {
            f.size = e.metadata().map(|m| m.len()).unwrap_or(0);
        }
        items.push((kind.is_dir(), name, f));
    }
    // SKILL.md, then folders, then files, each by name.
    items.sort_by(|a, b| {
        let rank = |(dir, name, f): &(bool, String, File)| {
            (
                f.rel != "SKILL.md",
                !*dir,
                name.to_lowercase(),
                name.clone(),
            )
        };
        rank(a).cmp(&rank(b))
    });
    for (descend, name, f) in items {
        if out.len() >= MAX_FILES {
            return;
        }
        let rel = format!("{}/", f.rel);
        out.push(f);
        if descend {
            walk(&real.join(&name), &shown.join(&name), &rel, depth + 1, out);
        }
    }
}

/// Makes an empty file at `rel` inside the skill's folder `dir`, creating the folders on its
/// way. Returns its path.
pub fn new_file(dir: &Path, rel: &str) -> io::Result<PathBuf> {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim();
    if rel.is_empty() {
        return Err(err("enter a file name"));
    }
    let cleaned = clean(Path::new(rel));
    if cleaned.is_absolute() || cleaned == Path::new(".") || cleaned.starts_with("..") {
        return Err(err("the file must be inside the skill's folder"));
    }
    let p = dir.join(&cleaned);
    if exists(&p) {
        return Err(err(format!("{rel} already exists")));
    }
    fs::create_dir_all(parent(&p))?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&p)?;
    Ok(p)
}

/// Whether `data` reads as text: UTF-8 without NUL bytes.
pub fn is_text(data: &[u8]) -> bool {
    !data.iter().take(8192).any(|b| *b == 0) && std::str::from_utf8(data).is_ok()
}
