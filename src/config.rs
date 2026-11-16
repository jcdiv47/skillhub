//! SkillHub's settings: the folders it scans for projects, and how deep.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::skills::clean;

/// How many levels below a scan folder projects are looked for, unless the user chooses
/// otherwise.
pub const DEFAULT_DEPTH: usize = 4;

/// What SkillHub remembers between runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// The folders scanned for projects.
    pub roots: Vec<PathBuf>,
    /// How many levels below each root projects are looked for.
    pub depth: usize,
    /// The panes' widths.
    #[serde(skip_serializing_if = "is_zero")]
    pub sidebar_width: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub list_width: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl Config {
    /// Reads the configuration at `path`; a missing file is an empty one.
    pub fn load(path: &Path) -> io::Result<Config> {
        let fresh = Config {
            depth: DEFAULT_DEPTH,
            ..Config::default()
        };
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(fresh),
            Err(e) => return Err(e),
        };
        let mut c: Config = serde_json::from_slice(&data).map_err(io::Error::other)?;
        if c.depth == 0 {
            c.depth = DEFAULT_DEPTH;
        }
        Ok(c)
    }

    /// Writes the configuration to `path`, creating its folder.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut data = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        data.push(b'\n');
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, data)?;
        fs::rename(&tmp, path)
    }

    /// Adds a folder to scan, unless it is already there.
    pub fn add_root(&mut self, path: &Path) {
        let p = clean(path);
        if !self.roots.contains(&p) {
            self.roots.push(p);
        }
    }

    /// Stops scanning a folder.
    pub fn remove_root(&mut self, path: &Path) {
        self.roots.retain(|r| r != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_empty_configuration() {
        let dir = crate::testing::temp_dir("config-missing");
        let c = Config::load(&dir.join("none.json")).unwrap();
        assert!(c.roots.is_empty());
        assert_eq!(c.depth, DEFAULT_DEPTH);
    }

    #[test]
    fn it_saves_and_loads_what_the_go_app_wrote() {
        let dir = crate::testing::temp_dir("config-roundtrip");
        let path = dir.join("nested/config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"roots":["/a/Code","/a/GitHub"],"depth":3,"sidebarWidth":270,"listWidth":330}"#,
        )
        .unwrap();
        let mut c = Config::load(&path).unwrap();
        assert_eq!(
            c.roots,
            [PathBuf::from("/a/Code"), PathBuf::from("/a/GitHub")]
        );
        assert_eq!((c.depth, c.sidebar_width, c.list_width), (3, 270.0, 330.0));
        c.add_root(Path::new("/a/GitHub/"));
        c.add_root(Path::new("/a/Work/./x"));
        c.remove_root(Path::new("/a/Code"));
        c.save(&path).unwrap();
        let again = Config::load(&path).unwrap();
        assert_eq!(
            again.roots,
            [PathBuf::from("/a/GitHub"), PathBuf::from("/a/Work/x")]
        );
        assert_eq!(again, c);
    }

    #[test]
    fn a_file_that_is_not_json_is_an_error() {
        let dir = crate::testing::temp_dir("config-bad");
        let path = dir.join("config.json");
        fs::write(&path, "{nope").unwrap();
        assert!(Config::load(&path).is_err());
    }
}
