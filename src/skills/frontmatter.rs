//! The YAML between the `---` lines that start SKILL.md.

use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

use super::{Problem, Severity};

/// A key of the frontmatter and its value, written as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub value: String,
    /// A value that is a list, a map or text of several lines, shown as a block.
    pub multiline: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frontmatter {
    /// Whether the file starts with a frontmatter block.
    pub present: bool,
    /// The YAML between the delimiters.
    pub raw: String,
    /// Its top-level keys, in the order written.
    pub fields: Vec<Field>,
    pub name: String,
    pub description: String,
    /// Why the YAML could not be read, if it could not.
    pub err: Option<String>,
    /// The Markdown after the frontmatter.
    pub body: String,
    /// The line of the file `body` starts on, from 1.
    pub body_line: usize,
}

impl Frontmatter {
    /// The value of a top-level key.
    #[cfg(test)]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.as_str())
    }
}

/// Splits a document into its frontmatter's YAML and its body, with the line the body starts
/// on. `None` when it has no frontmatter, or it is never closed.
pub fn split_frontmatter(doc: &str) -> Option<(String, &str, usize)> {
    let doc = doc.strip_prefix('\u{feff}').unwrap_or(doc);
    let (first, mut rest) = doc.split_once('\n')?;
    if first.trim_end_matches([' ', '\t', '\r']) != "---" {
        return None;
    }
    let mut line = 2;
    let mut yaml = String::new();
    while !rest.is_empty() {
        let (l, r) = rest.split_once('\n').unwrap_or((rest, ""));
        let t = l.trim_end_matches([' ', '\t', '\r']);
        if t == "---" || t == "..." {
            return Some((yaml, r, line + 1));
        }
        yaml.push_str(l);
        yaml.push('\n');
        rest = r;
        line += 1;
    }
    None
}

/// Reads the frontmatter of a SKILL.md.
pub fn parse_frontmatter(doc: &str) -> Frontmatter {
    let Some((yaml, body, line)) = split_frontmatter(doc) else {
        let body = doc.strip_prefix('\u{feff}').unwrap_or(doc);
        return Frontmatter {
            body: body.to_owned(),
            body_line: 1,
            ..Default::default()
        };
    };
    let mut f = Frontmatter {
        present: true,
        raw: yaml,
        body: body.to_owned(),
        body_line: line,
        ..Default::default()
    };
    let docs = match YamlLoader::load_from_str(&f.raw) {
        Ok(d) => d,
        Err(e) => {
            f.err = Some(e.to_string());
            return f;
        }
    };
    let map = match docs.into_iter().next() {
        None | Some(Yaml::Null) => return f,
        Some(Yaml::Hash(map)) => map,
        Some(_) => {
            f.err = Some("the frontmatter is not a map of keys and values".into());
            return f;
        }
    };
    for (k, v) in map {
        let key = scalar(&k).unwrap_or_else(|| emit(&k));
        let (value, multiline) = match scalar(&v) {
            Some(s) => {
                let multiline = s.trim_end_matches('\n').contains('\n');
                (s, multiline)
            }
            None => (emit(&v), true),
        };
        match key.as_str() {
            "name" => f.name = value.trim().to_owned(),
            "description" => f.description = value.trim().to_owned(),
            _ => {}
        }
        f.fields.push(Field {
            key,
            value,
            multiline,
        });
    }
    f
}

/// A scalar as text, `None` for lists and maps.
fn scalar(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        Yaml::Null => Some(String::new()),
        _ => None,
    }
}

/// A value written as YAML, without the document marker.
fn emit(y: &Yaml) -> String {
    let mut out = String::new();
    if YamlEmitter::new(&mut out).dump(y).is_err() {
        return String::new();
    }
    let out = out.strip_prefix("---").unwrap_or(&out);
    out.trim_start_matches([' ', '\n'])
        .trim_end_matches('\n')
        .to_owned()
}

/// Lowercase letters, digits and single hyphens, as the Agent Skills specification names skills.
fn name_rule(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Checks a skill's frontmatter against the Agent Skills specification: a name of lowercase
/// letters, digits and single hyphens, at most 64 characters, the same as its directory's, and
/// a description of at most 1024 characters.
pub fn validate(dir_name: &str, f: &Frontmatter) -> Vec<Problem> {
    let mut ps = Vec::new();
    let mut add = |severity, message: String| ps.push(Problem { severity, message });
    if !f.present {
        add(
            Severity::Error,
            "SKILL.md has no frontmatter (--- name/description --- at the top).".into(),
        );
        return ps;
    }
    if let Some(err) = &f.err {
        add(
            Severity::Error,
            format!("The frontmatter is not valid YAML: {err}"),
        );
        return ps;
    }
    if f.name.is_empty() {
        add(Severity::Error, "The frontmatter has no name.".into());
    } else {
        if f.name != dir_name {
            add(
                Severity::Warning,
                format!(
                    "The name {:?} differs from the folder's name {:?}.",
                    f.name, dir_name
                ),
            );
        }
        if f.name.chars().count() > 64 {
            add(
                Severity::Warning,
                "The name is longer than 64 characters.".into(),
            );
        }
        if !name_rule(&f.name) {
            add(
                Severity::Warning,
                "The name should be lowercase letters, digits and single hyphens.".into(),
            );
        }
    }
    if f.description.is_empty() {
        add(
            Severity::Error,
            "The frontmatter has no description.".into(),
        );
    } else if f.description.chars().count() > 1024 {
        add(
            Severity::Warning,
            "The description is longer than 1024 characters.".into(),
        );
    }
    ps
}

/// `doc` with the frontmatter's top-level name set to `name`, `None` when it has no name line
/// to change.
pub fn set_name(doc: &str, name: &str) -> Option<String> {
    let (yaml, _, _) = split_frontmatter(doc)?;
    let start = doc.find(&yaml)?;
    let mut off = start;
    for l in yaml.split_inclusive('\n') {
        let trimmed = l.trim_end_matches(['\r', '\n']);
        if trimmed.starts_with("name:") {
            let end = off + trimmed.len();
            return Some(format!("{}name: {}{}", &doc[..off], name, &doc[end..]));
        }
        off += l.len();
    }
    None
}

/// A new SKILL.md for a skill.
pub fn template(name: &str, description: &str) -> String {
    let description = if description.is_empty() {
        "Describe what this skill does and when to use it."
    } else {
        description
    };
    let desc = emit(&Yaml::String(description.to_owned()));
    format!(
        "---\nname: {name}\ndescription: {desc}\n---\n\n# {}\n\n## Instructions\n\n",
        title(name)
    )
}

fn title(name: &str) -> String {
    let words: Vec<String> = name
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect();
    if words.is_empty() {
        name.to_owned()
    } else {
        words.join(" ")
    }
}
