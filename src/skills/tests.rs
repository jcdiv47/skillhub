use std::cell::RefCell;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::*;
use crate::testing::{symlink, temp_dir, write};

fn md(name: &str, desc: &str) -> String {
    format!("---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n")
}

fn by_name<'a>(l: &'a Location, name: &str) -> &'a Skill {
    l.skills
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no skill {name}"))
}

fn is_link(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

fn names(ps: &[ProjectInfo]) -> String {
    ps.iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn each_agent_has_its_folder_in_each_scope() {
    assert_eq!(rel_dir(Agent::Shared, Scope::Global), ".agents/skills");
    assert_eq!(rel_dir(Agent::Claude, Scope::Project), ".claude/skills");
    assert_eq!(rel_dir(Agent::Pi, Scope::Global), ".pi/agent/skills");
    assert_eq!(rel_dir(Agent::Pi, Scope::Project), ".pi/skills");
}

#[test]
fn paths_clean_and_relate_as_written() {
    assert_eq!(clean(Path::new("/a/./b/../c")), Path::new("/a/c"));
    assert_eq!(clean(Path::new("/../a")), Path::new("/a"));
    assert!(
        within(Path::new("/a"), Path::new("/a/b"))
            && !within(Path::new("/a"), Path::new("/a"))
            && !within(Path::new("/a"), Path::new("/ab"))
    );
    assert_eq!(
        rel(
            Path::new("/h/.pi/agent/skills"),
            Path::new("/h/.agents/skills/x")
        )
        .unwrap(),
        Path::new("../../../.agents/skills/x")
    );
}

#[test]
fn a_location_reads_folders_links_and_what_is_wrong_with_them() {
    let home = temp_dir("kinds");
    let shared = home.join(".agents/skills");
    write(&shared.join("pdf/SKILL.md"), &md("pdf", "Read PDFs."));
    write(&shared.join("README.md"), "not a skill\n");
    write(&shared.join(".DS_Store"), "x");
    write(&shared.join("empty/notes.txt"), "no SKILL.md here");
    let claude = home.join(".claude/skills");
    symlink(shared.join("pdf"), &claude.join("pdf"));
    symlink(home.join("nowhere"), &claude.join("gone"));
    symlink(claude.join("pdf"), &claude.join("pdf2"));
    symlink("../../.agents/skills/pdf", &claude.join("rel"));

    let l = read_location(Agent::Shared, Scope::Global, &home);
    assert!(l.exists && l.links.is_empty());
    assert_eq!(l.skills.len(), 2, "pdf and empty");
    let pdf = by_name(&l, "pdf");
    assert!(
        !pdf.is_link
            && pdf.has_skill_md
            && pdf.meta.description == "Read PDFs."
            && pdf.problems.is_empty()
    );
    let empty = by_name(&l, "empty");
    assert!(!empty.has_skill_md && empty.problems[0].message.contains("No SKILL.md"));

    let c = read_location(Agent::Claude, Scope::Global, &home);
    let link = by_name(&c, "pdf");
    assert!(
        link.is_link
            && !link.broken
            && link.real_path.as_deref() == Some(shared.join("pdf").as_path())
    );
    assert_eq!(link.target(), Some(shared.join("pdf").as_path()));
    assert!(link.chain.len() == 1 && link.problems.is_empty());
    let gone = by_name(&c, "gone");
    assert!(gone.broken && gone.real_path.is_none() && gone.worst() == Some(Severity::Error));
    assert!(gone.problems[0].message.contains("Broken link"));
    let two = by_name(&c, "pdf2");
    assert!(two.chain.len() == 2 && !two.broken && two.real_path == link.real_path);
    assert!(
        two.worst() == Some(Severity::Warning) && two.problems[0].message.contains("another link")
    );
    let relative = by_name(&c, "rel");
    assert!(
        !relative.broken
            && relative.real_path == link.real_path
            && relative.target() == Some(Path::new("../../.agents/skills/pdf"))
    );
    assert!(relative.problems.len() == 1 && relative.problems[0].message.contains("differs"));

    let pi = read_location(Agent::Pi, Scope::Global, &home);
    assert!(!pi.exists && pi.skills.is_empty() && pi.err.is_none());
    assert_eq!(pi.dir, home.join(".pi/agent/skills"));
}

#[test]
fn links_that_loop_are_broken() {
    let dir = temp_dir("loop");
    let skills = dir.join(".claude/skills");
    symlink(skills.join("b"), &skills.join("a"));
    symlink(skills.join("a"), &skills.join("b"));
    let l = read_location(Agent::Claude, Scope::Project, &dir);
    let a = by_name(&l, "a");
    assert!(a.broken && a.problems[0].message.contains("loops"));
}

#[test]
fn a_skills_folder_that_is_a_link_says_so() {
    let home = temp_dir("dirlink");
    write(&home.join(".agents/skills/x/SKILL.md"), &md("x", "X."));
    symlink(home.join(".agents/skills"), &home.join(".claude/skills"));
    let proj = home.join("code/p");
    write(&home.join("elsewhere/pi/skills/y/SKILL.md"), &md("y", "Y."));
    symlink(home.join("elsewhere/pi"), &proj.join(".pi"));

    let c = read_location(Agent::Claude, Scope::Global, &home);
    assert!(c.links.len() == 1 && c.links[0].link == home.join(".claude/skills"));
    assert!(
        c.real_dir.as_deref() == Some(home.join(".agents/skills").as_path()) && c.skills.len() == 1
    );
    assert!(
        !c.skills[0].is_link
            && c.skills[0].real_path.as_deref() == Some(home.join(".agents/skills/x").as_path())
    );
    let p = read_location(Agent::Pi, Scope::Project, &proj);
    assert!(
        p.exists
            && p.links.len() == 1
            && p.links[0].link == proj.join(".pi")
            && p.skills.len() == 1
    );
}

#[test]
fn projects_are_the_folders_with_skills_under_the_roots() {
    let home = temp_dir("projects");
    let code = home.join("Code");
    write(&code.join("a/.claude/skills/s/SKILL.md"), &md("s", "S."));
    write(&code.join("b/.pi/skills/.keep"), "");
    write(&code.join("c/.pi/agent/skills/s/SKILL.md"), &md("s", "S."));
    write(
        &code.join("group/deep/.agents/skills/s/SKILL.md"),
        &md("s", "S."),
    );
    write(
        &code.join("a/node_modules/dep/.claude/skills/s/SKILL.md"),
        &md("s", "S."),
    );
    write(
        &code.join(".hidden/.claude/skills/s/SKILL.md"),
        &md("s", "S."),
    );
    write(
        &code.join("x/y/z/w/.claude/skills/s/SKILL.md"),
        &md("s", "S."),
    );
    write(&home.join(".claude/skills/g/SKILL.md"), &md("g", "G."));
    symlink(code.join("a"), &code.join("alias"));

    let got = find_projects(&[code.clone(), code.clone()], 3, &home);
    assert_eq!(names(&got), "a,b,deep");
    assert!(got[2].root == code.join("group/deep") && got[2].scan_root == code);
    assert_eq!(
        names(&find_projects(std::slice::from_ref(&home), 6, &home)),
        "a,b,deep,w",
        "home is never a project"
    );
    assert!(find_projects(&[home.join("missing")], 3, &home).is_empty());
}

#[test]
fn a_snapshot_answers_what_links_where() {
    let home = temp_dir("snapshot");
    let shared = home.join(".agents/skills");
    write(&shared.join("pdf/SKILL.md"), &md("pdf", "P."));
    write(&shared.join("pdf/sub/SKILL.md"), &md("sub", "Nested."));
    symlink(shared.join("pdf"), &home.join(".claude/skills/pdf"));
    let proj = home.join("Code/p");
    symlink(shared.join("pdf"), &proj.join(".claude/skills/pdf"));
    symlink(shared.join("pdf/sub"), &proj.join(".claude/skills/sub"));
    write(
        &proj.join(".agents/skills/pdf/SKILL.md"),
        &md("pdf", "Own copy."),
    );

    let snap = load(&home, find_projects(&[home.join("Code")], 3, &home));
    assert!(snap.projects.len() == 1 && snap.projects[0].count() == 3 && snap.global_count() == 2);
    let orig = snap.find(&shared.join("pdf")).unwrap();
    assert_eq!(
        snap.links_to(orig.real_path.as_deref(), &orig.path).len(),
        2
    );
    assert_eq!(
        snap.links_into(orig.real_path.as_deref(), &orig.path).len(),
        3
    );
    let own = snap.find(&proj.join(".agents/skills/pdf")).unwrap();
    assert_eq!(
        snap.same(own, Some(&proj)).len(),
        3,
        "its link and the two global ones"
    );
    assert_eq!(snap.same(orig, None).len(), 1);
    assert!(
        !snap.locations(None).is_empty()
            && !snap.locations(Some(&proj)).is_empty()
            && snap.locations(Some(Path::new("/nope"))).is_empty()
    );
}

#[test]
fn frontmatter_is_read_as_written() {
    let doc = "---\nname: my-skill\ndescription: >\n  Folded text\n  over lines.\nallowed-tools:\n  - Read\n  - Bash\nmetadata:\n  version: 2\n---\n# Body\n";
    let f = parse_frontmatter(doc);
    assert!(f.present && f.err.is_none(), "{f:?}");
    assert_eq!(
        (f.name.as_str(), f.description.as_str()),
        ("my-skill", "Folded text over lines.")
    );
    assert_eq!(f.fields.len(), 4);
    assert!(f.fields[2].key == "allowed-tools" && f.fields[2].multiline);
    assert_eq!(f.fields[2].value, "- Read\n- Bash");
    assert_eq!(f.get("metadata"), Some("version: 2"));
    assert_eq!((f.body.as_str(), f.body_line), ("# Body\n", 12));
    assert!(validate("my-skill", &f).is_empty());

    let crlf = parse_frontmatter("---\r\nname: a\r\ndescription: b\r\n---\r\nbody");
    assert_eq!(
        (
            crlf.name.as_str(),
            crlf.description.as_str(),
            crlf.body.as_str()
        ),
        ("a", "b", "body")
    );
    assert_eq!(
        parse_frontmatter("\u{feff}---\nname: a\ndescription: b\n---\n").name,
        "a"
    );
    let none = parse_frontmatter("# Just markdown\n");
    assert!(!none.present && none.body == "# Just markdown\n");
    assert!(!parse_frontmatter("---\nname: a\n").present, "never closed");
    let bad = parse_frontmatter("---\nname: [a\n---\n");
    assert!(bad.present && bad.err.is_some());
    assert!(
        parse_frontmatter("---\n- a\n---\n").err.is_some(),
        "a list is no frontmatter"
    );
}

#[test]
fn validation_follows_the_agent_skills_rules() {
    let long = "a".repeat(65);
    let cases = [
        ("x", "no frontmatter".to_owned(), "no frontmatter"),
        ("x", "---\nname: [\n---\n".to_owned(), "not valid YAML"),
        ("x", "---\ndescription: d\n---\n".to_owned(), "no name"),
        ("x", "---\nname: x\n---\n".to_owned(), "no description"),
        (
            "x",
            "---\nname: y\ndescription: d\n---\n".to_owned(),
            "differs",
        ),
        (
            "Bad_Name",
            "---\nname: Bad_Name\ndescription: d\n---\n".to_owned(),
            "lowercase",
        ),
        (
            "x",
            format!("---\nname: x\ndescription: {}\n---\n", "a".repeat(1025)),
            "1024",
        ),
        (
            long.as_str(),
            format!("---\nname: {long}\ndescription: d\n---\n"),
            "64",
        ),
    ];
    for (dir, doc, want) in &cases {
        let got: Vec<String> = validate(dir, &parse_frontmatter(doc))
            .into_iter()
            .map(|p| p.message)
            .collect();
        assert!(got.join(" | ").contains(want), "{want}: {got:?}");
    }
}

#[test]
fn the_name_line_changes_and_a_template_is_valid() {
    let doc = "---\nname: old\ndescription: d\n---\nname: body stays\n";
    assert_eq!(
        set_name(doc, "new").unwrap(),
        "---\nname: new\ndescription: d\n---\nname: body stays\n"
    );
    assert!(
        set_name("---\ndescription: d\n---\n", "n").is_none() && set_name("plain", "n").is_none()
    );
    assert_eq!(
        set_name("---\r\nname: old\r\ndescription: d\r\n---\r\n", "new").unwrap(),
        "---\r\nname: new\r\ndescription: d\r\n---\r\n"
    );
    let f = parse_frontmatter(&template("pdf-tools", "Work with PDFs: forms, text."));
    assert_eq!(
        (f.name.as_str(), f.description.as_str()),
        ("pdf-tools", "Work with PDFs: forms, text.")
    );
    assert!(f.body.contains("# Pdf Tools") && validate("pdf-tools", &f).is_empty());
}

#[test]
fn names_are_checked() {
    for bad in ["", " ", "a/b", "a\\b", ".", "..", ".hidden", " a"] {
        assert!(check_name(bad).is_err(), "{bad:?}");
    }
    for good in ["a", "my-skill", "Skill 2"] {
        assert!(check_name(good).is_ok(), "{good:?}");
    }
}

#[test]
fn a_skill_is_created_with_its_folder() {
    let dir = temp_dir("create").join("proj/.pi/skills");
    let p = create_skill(&dir, "new-one", "Does things.").unwrap();
    let l = read_location(
        Agent::Pi,
        Scope::Project,
        dir.parent().unwrap().parent().unwrap(),
    );
    let s = by_name(&l, "new-one");
    assert!(s.path == p && s.problems.is_empty());
    assert!(create_skill(&dir, "new-one", "").is_err() && create_skill(&dir, "a/b", "").is_err());
}

#[test]
fn writing_goes_through_links_and_keeps_them() {
    let dir = temp_dir("write");
    let real = dir.join("shared/s");
    write(&real.join("SKILL.md"), "old");
    fs::set_permissions(real.join("SKILL.md"), fs::Permissions::from_mode(0o600)).unwrap();
    let link = dir.join("proj/skills/s");
    symlink(&real, &link);
    write(&dir.join("elsewhere.md"), "outside");
    symlink(dir.join("elsewhere.md"), &real.join("ref.md"));

    write_file(&link.join("SKILL.md"), b"new").unwrap();
    assert_eq!(fs::read_to_string(real.join("SKILL.md")).unwrap(), "new");
    assert!(is_link(&link));
    assert_eq!(
        fs::metadata(real.join("SKILL.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    write_file(&link.join("ref.md"), b"changed").unwrap();
    assert!(is_link(&real.join("ref.md")));
    assert_eq!(
        fs::read_to_string(dir.join("elsewhere.md")).unwrap(),
        "changed"
    );
    write_file(&link.join("new.md"), b"n").unwrap();
    assert_eq!(fs::read_to_string(real.join("new.md")).unwrap(), "n");
    for e in fs::read_dir(&real).unwrap() {
        assert!(
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .contains("skillhub"),
            "a temporary file was left"
        );
    }
}

#[test]
fn links_are_made_and_pointed_elsewhere() {
    let dir = temp_dir("links");
    let (a, b) = (dir.join("lib/a"), dir.join("lib/b"));
    write(&a.join("SKILL.md"), &md("a", "A."));
    write(&b.join("SKILL.md"), &md("b", "B."));
    let dest = dir.join("proj/.claude/skills");

    let p = create_link(&dest, "a", &a).unwrap();
    assert_eq!(fs::read_link(&p).unwrap(), a);
    assert!(create_link(&dest, "a", &a).is_err(), "over an entry");
    assert!(
        create_link(&dest, "rel", Path::new("lib/a")).is_err(),
        "to a relative path"
    );
    assert!(
        create_link(&a.join("inner"), "self", &a).is_err(),
        "into itself"
    );

    retarget(&p, &b).unwrap();
    assert_eq!(fs::read_link(&p).unwrap(), b);
    assert!(retarget(&a, &b).is_err(), "a folder is no link");
    retarget(&p, Path::new("../../../lib/a")).unwrap();
    assert_eq!(fs::read_link(&p).unwrap(), Path::new("../../../lib/a"));
    assert_eq!(fs::canonicalize(&p).unwrap(), a);
    let broken = dest.join("broken");
    symlink(dir.join("missing"), &broken);
    retarget(&broken, &a).unwrap();
    assert_eq!(fs::canonicalize(&broken).unwrap(), a);

    assert_eq!(check_target(&a), Ok(None));
    assert!(
        matches!(check_target(&dir.join("lib")), Ok(Some(_))),
        "no SKILL.md is a warning"
    );
    assert!(
        check_target(&dir.join("nope")).is_err()
            && check_target(&a.join("SKILL.md")).is_err()
            && check_target(Path::new("rel")).is_err()
    );
}

#[test]
fn a_link_becomes_a_copy_of_its_folder() {
    let dir = temp_dir("materialize");
    let src = dir.join("lib/a");
    write(&src.join("SKILL.md"), &md("a", "A."));
    write(&src.join("scripts/run.sh"), "#!/bin/sh\n");
    fs::set_permissions(
        src.join("scripts/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    symlink("../SKILL.md", &src.join("refs/skill.md"));
    let link = dir.join("proj/skills/a");
    symlink(&src, &link);

    materialize(&link).unwrap();
    assert!(!is_link(&link) && link.is_dir());
    assert_eq!(
        fs::read_to_string(link.join("SKILL.md")).unwrap(),
        md("a", "A.")
    );
    assert_eq!(
        fs::metadata(link.join("scripts/run.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        fs::read_link(link.join("refs/skill.md")).unwrap(),
        Path::new("../SKILL.md")
    );
    write(&link.join("SKILL.md"), "changed");
    assert_eq!(
        fs::read_to_string(src.join("SKILL.md")).unwrap(),
        md("a", "A."),
        "the source is its own"
    );
    assert!(materialize(&link).is_err(), "a folder is no link");
    let broken = dir.join("proj/skills/b");
    symlink(dir.join("missing"), &broken);
    assert!(materialize(&broken).is_err() && is_link(&broken));
}

#[test]
fn a_folder_moves_and_leaves_a_link() {
    let dir = temp_dir("convert");
    let p = dir.join("proj/.claude/skills/a");
    write(&p.join("SKILL.md"), &md("a", "A."));
    let shared = dir.join("home/.agents/skills");

    let dest = convert_to_link(&p, &shared).unwrap();
    assert_eq!(dest, shared.join("a"));
    assert_eq!(fs::read_link(&p).unwrap(), dest);
    assert_eq!(
        fs::read_to_string(p.join("SKILL.md")).unwrap(),
        md("a", "A.")
    );
    assert!(convert_to_link(&p, &shared).is_err(), "a link is no folder");
    let q = dir.join("proj/.claude/skills/q");
    write(&q.join("SKILL.md"), &md("q", "Q."));
    write(&shared.join("q/SKILL.md"), "taken");
    assert!(convert_to_link(&q, &shared).is_err(), "over a folder");
    assert!(
        convert_to_link(&q, q.parent().unwrap()).is_err(),
        "to where it is"
    );
    assert!(
        convert_to_link(&q, &q.join("inner")).is_err(),
        "into itself"
    );
    assert!(q.is_dir() && !is_link(&q));
}

#[test]
fn entries_are_renamed_and_removed() {
    let dir = temp_dir("rename");
    let skills = dir.join("skills");
    write(&skills.join("a/SKILL.md"), &md("a", "A."));
    write(&skills.join("b/SKILL.md"), &md("b", "B."));
    symlink(skills.join("a"), &skills.join("l"));

    let np = rename(&skills.join("a"), "c").unwrap();
    assert!(np == skills.join("c") && np.exists());
    assert!(rename(&np, "b").is_err() && rename(&np, "x/y").is_err());
    assert_eq!(rename(&np, "c").unwrap(), np);

    let trashed: RefCell<Vec<PathBuf>> = RefCell::default();
    let trash = |p: &Path| {
        trashed.borrow_mut().push(p.to_path_buf());
        fs::remove_dir_all(p)
    };
    remove(&skills.join("l"), Some(&trash)).unwrap();
    assert!(
        trashed.borrow().is_empty() && !is_link(&skills.join("l")) && np.exists(),
        "a link is only unlinked"
    );
    remove(&np, Some(&trash)).unwrap();
    assert_eq!(trashed.borrow().as_slice(), std::slice::from_ref(&np));
    let fails = |_: &Path| Err(io::Error::other("no trash"));
    assert!(remove(&skills.join("b"), Some(&fails)).is_err() && skills.join("b").exists());
    assert!(
        remove(&skills.join("b"), None).is_err(),
        "a folder needs a trash"
    );
}

#[test]
fn files_of_a_skill_are_listed_through_its_link() {
    let dir = temp_dir("files");
    let s = dir.join("lib/s");
    write(&s.join("SKILL.md"), "x");
    write(&s.join("a.md"), "a");
    write(&s.join("scripts/run.py"), "print()");
    write(&s.join(".git/HEAD"), "ref");
    symlink(dir.join("nowhere"), &s.join("dead.md"));
    symlink(s.join("a.md"), &s.join("alias.md"));
    let link = dir.join("skills/s");
    symlink(&s, &link);

    let files = list_files(&link).unwrap();
    let rels: Vec<&str> = files.iter().map(|f| f.rel.as_str()).collect();
    assert_eq!(
        rels,
        [
            "SKILL.md",
            "scripts",
            "scripts/run.py",
            "a.md",
            "alias.md",
            "dead.md"
        ],
        "folders first, their files under them"
    );
    assert_eq!(
        files[0].path,
        link.join("SKILL.md"),
        "paths go through the link"
    );
    assert!(files[4].is_link && !files[4].broken && files[4].target == s.join("a.md"));
    assert!(files[5].is_link && files[5].broken);
    assert!(files[2].depth == 1 && !files[2].is_dir);

    let p = new_file(&link, "references/guide.md").unwrap();
    assert!(s.join("references/guide.md").exists() && p == link.join("references/guide.md"));
    for bad in ["", "../escape.md", "/abs.md", "a.md"] {
        assert!(new_file(&link, bad).is_err(), "{bad:?}");
    }
    assert!(is_text(b"hello") && !is_text(&[b'a', 0, b'b']));
}
