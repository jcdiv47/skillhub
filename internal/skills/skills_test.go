package skills

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func write(t *testing.T, p, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func skillMD(name, desc string) string {
	return "---\nname: " + name + "\ndescription: " + desc + "\n---\n\n# " + name + "\n"
}

func symlink(t *testing.T, target, link string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, link); err != nil {
		t.Fatal(err)
	}
}

func real(t *testing.T, p string) string {
	t.Helper()
	r, err := filepath.EvalSymlinks(p)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func byName(l *Location, name string) *Skill {
	for _, s := range l.Skills {
		if s.Name == name {
			return s
		}
	}
	return nil
}

func TestRelDirs(t *testing.T) {
	cases := []struct {
		a     Agent
		scope Scope
		want  string
	}{
		{AgentShared, GlobalScope, ".agents/skills"},
		{AgentShared, ProjectScope, ".agents/skills"},
		{AgentClaude, GlobalScope, ".claude/skills"},
		{AgentClaude, ProjectScope, ".claude/skills"},
		{AgentPi, GlobalScope, ".pi/agent/skills"},
		{AgentPi, ProjectScope, ".pi/skills"},
	}
	for _, c := range cases {
		if got := filepath.ToSlash(RelDir(c.a, c.scope)); got != c.want {
			t.Errorf("RelDir(%v, %v) = %q, want %q", c.a, c.scope, got, c.want)
		}
	}
}

func TestReadLocationKinds(t *testing.T) {
	home := t.TempDir()
	shared := filepath.Join(home, ".agents", "skills")
	write(t, filepath.Join(shared, "pdf", "SKILL.md"), skillMD("pdf", "Read PDFs."))
	write(t, filepath.Join(shared, "pdf", "scripts", "x.py"), "print(1)\n")
	write(t, filepath.Join(shared, "README.md"), "not a skill\n")
	write(t, filepath.Join(shared, ".DS_Store"), "x")
	write(t, filepath.Join(shared, "empty", "notes.txt"), "no SKILL.md here")

	claude := filepath.Join(home, ".claude", "skills")
	symlink(t, filepath.Join(shared, "pdf"), filepath.Join(claude, "pdf"))
	symlink(t, filepath.Join(home, "nowhere"), filepath.Join(claude, "gone"))
	symlink(t, filepath.Join(claude, "pdf"), filepath.Join(claude, "pdf2")) // link to a link
	symlink(t, "../../.agents/skills/pdf", filepath.Join(claude, "rel"))    // relative

	l := ReadLocation(AgentShared, GlobalScope, home)
	if !l.Exists || len(l.Links) != 0 {
		t.Fatalf("shared location: %+v", l)
	}
	if got := len(l.Skills); got != 2 {
		t.Fatalf("shared skills %d, want pdf and empty", got)
	}
	pdf := byName(l, "pdf")
	if pdf == nil || pdf.IsLink || !pdf.HasSkillMD || pdf.Meta.Description != "Read PDFs." || len(pdf.Problems) != 0 {
		t.Fatalf("pdf: %+v", pdf)
	}
	empty := byName(l, "empty")
	if empty == nil || empty.HasSkillMD || len(empty.Problems) != 1 || !strings.Contains(empty.Problems[0].Message, "No SKILL.md") {
		t.Fatalf("empty: %+v", empty)
	}

	c := ReadLocation(AgentClaude, GlobalScope, home)
	link := byName(c, "pdf")
	if link == nil || !link.IsLink || link.Broken || link.RealPath != real(t, filepath.Join(shared, "pdf")) {
		t.Fatalf("link: %+v", link)
	}
	if link.Target() != filepath.Join(shared, "pdf") || len(link.Chain) != 1 || len(link.Problems) != 0 {
		t.Fatalf("link chain %+v problems %+v", link.Chain, link.Problems)
	}
	gone := byName(c, "gone")
	if gone == nil || !gone.Broken || gone.RealPath != "" {
		t.Fatalf("gone: %+v", gone)
	}
	if sev, ok := gone.Worst(); !ok || sev != Error || !strings.Contains(gone.Problems[0].Message, "Broken link") {
		t.Fatalf("gone problems: %+v", gone.Problems)
	}
	two := byName(c, "pdf2")
	if two == nil || len(two.Chain) != 2 || two.Broken || two.RealPath != link.RealPath {
		t.Fatalf("pdf2: %+v", two)
	}
	if sev, _ := two.Worst(); sev != Warning || !strings.Contains(two.Problems[0].Message, "another link") {
		t.Fatalf("pdf2 problems: %+v", two.Problems)
	}
	rel := byName(c, "rel")
	if rel == nil || rel.Broken || rel.RealPath != link.RealPath || rel.Target() != "../../.agents/skills/pdf" {
		t.Fatalf("relative link: %+v", rel)
	}
	// rel's name differs from the frontmatter's.
	if len(rel.Problems) != 1 || !strings.Contains(rel.Problems[0].Message, "differs") {
		t.Fatalf("rel problems: %+v", rel.Problems)
	}

	pi := ReadLocation(AgentPi, GlobalScope, home)
	if pi.Exists || len(pi.Skills) != 0 || pi.Err != "" {
		t.Fatalf("missing pi location: %+v", pi)
	}
	if pi.Dir != filepath.Join(home, ".pi", "agent", "skills") {
		t.Fatalf("pi dir %s", pi.Dir)
	}
}

func TestLinkLoop(t *testing.T) {
	dir := t.TempDir()
	skills := filepath.Join(dir, ".claude", "skills")
	symlink(t, filepath.Join(skills, "b"), filepath.Join(skills, "a"))
	symlink(t, filepath.Join(skills, "a"), filepath.Join(skills, "b"))
	l := ReadLocation(AgentClaude, ProjectScope, dir)
	a := byName(l, "a")
	if a == nil || !a.Broken || !strings.Contains(a.Problems[0].Message, "loops") {
		t.Fatalf("a: %+v", a)
	}
}

func TestLocationDirIsLink(t *testing.T) {
	home := t.TempDir()
	write(t, filepath.Join(home, ".agents", "skills", "x", "SKILL.md"), skillMD("x", "X."))
	// ~/.claude/skills -> ~/.agents/skills
	symlink(t, filepath.Join(home, ".agents", "skills"), filepath.Join(home, ".claude", "skills"))
	// project/.pi -> elsewhere/pi, with skills inside
	proj := filepath.Join(home, "code", "p")
	write(t, filepath.Join(home, "elsewhere", "pi", "skills", "y", "SKILL.md"), skillMD("y", "Y."))
	symlink(t, filepath.Join(home, "elsewhere", "pi"), filepath.Join(proj, ".pi"))

	c := ReadLocation(AgentClaude, GlobalScope, home)
	if len(c.Links) != 1 || c.Links[0].Link != filepath.Join(home, ".claude", "skills") {
		t.Fatalf("claude links %+v", c.Links)
	}
	if c.RealDir != real(t, filepath.Join(home, ".agents", "skills")) || len(c.Skills) != 1 {
		t.Fatalf("claude: %+v", c)
	}
	// The skill is a real folder seen through a linked location.
	if s := c.Skills[0]; s.IsLink || s.RealPath != real(t, filepath.Join(home, ".agents", "skills", "x")) {
		t.Fatalf("x through the linked dir: %+v", s)
	}

	p := ReadLocation(AgentPi, ProjectScope, proj)
	if !p.Exists || len(p.Links) != 1 || p.Links[0].Link != filepath.Join(proj, ".pi") || len(p.Skills) != 1 {
		t.Fatalf("pi project: %+v", p)
	}
}

func TestFindProjects(t *testing.T) {
	home := t.TempDir()
	code := filepath.Join(home, "Code")
	write(t, filepath.Join(code, "a", ".claude", "skills", "s", "SKILL.md"), skillMD("s", "S."))
	write(t, filepath.Join(code, "b", ".pi", "skills", ".keep"), "")
	write(t, filepath.Join(code, "c", ".pi", "agent", "skills", "s", "SKILL.md"), skillMD("s", "S.")) // not a project dir for pi
	write(t, filepath.Join(code, "group", "deep", ".agents", "skills", "s", "SKILL.md"), skillMD("s", "S."))
	write(t, filepath.Join(code, "a", "node_modules", "dep", ".claude", "skills", "s", "SKILL.md"), skillMD("s", "S."))
	write(t, filepath.Join(code, ".hidden", ".claude", "skills", "s", "SKILL.md"), skillMD("s", "S."))
	write(t, filepath.Join(code, "x", "y", "z", "w", ".claude", "skills", "s", "SKILL.md"), skillMD("s", "S."))
	write(t, filepath.Join(home, ".claude", "skills", "g", "SKILL.md"), skillMD("g", "G."))
	symlink(t, filepath.Join(code, "a"), filepath.Join(code, "alias")) // links are not followed

	got := FindProjects([]string{code, code}, 3, home)
	var names []string
	for _, p := range got {
		names = append(names, p.Name)
	}
	if strings.Join(names, ",") != "a,b,deep" {
		t.Fatalf("projects %v", names)
	}
	if got[2].Root != filepath.Join(code, "group", "deep") || got[2].ScanRoot != code {
		t.Fatalf("deep: %+v", got[2])
	}
	// Deeper scans reach x/y/z/w; home is never a project.
	got = FindProjects([]string{home}, 6, home)
	names = nil
	for _, p := range got {
		names = append(names, p.Name)
	}
	if strings.Join(names, ",") != "a,b,deep,w" {
		t.Fatalf("projects from home %v", names)
	}
	if len(FindProjects([]string{filepath.Join(home, "missing")}, 3, home)) != 0 {
		t.Fatal("a missing root found projects")
	}
}

func TestSnapshotQueries(t *testing.T) {
	home := t.TempDir()
	shared := filepath.Join(home, ".agents", "skills")
	write(t, filepath.Join(shared, "pdf", "SKILL.md"), skillMD("pdf", "P."))
	write(t, filepath.Join(shared, "pdf", "sub", "SKILL.md"), skillMD("sub", "Nested."))
	symlink(t, filepath.Join(shared, "pdf"), filepath.Join(home, ".claude", "skills", "pdf"))
	proj := filepath.Join(home, "Code", "p")
	symlink(t, filepath.Join(shared, "pdf"), filepath.Join(proj, ".claude", "skills", "pdf"))
	symlink(t, filepath.Join(shared, "pdf", "sub"), filepath.Join(proj, ".claude", "skills", "sub"))
	write(t, filepath.Join(proj, ".agents", "skills", "pdf", "SKILL.md"), skillMD("pdf", "Own copy."))

	snap := Load(home, FindProjects([]string{filepath.Join(home, "Code")}, 3, home))
	if len(snap.Projects) != 1 || snap.Projects[0].Count() != 3 || snap.GlobalCount() != 2 {
		t.Fatalf("snapshot: %d projects, global %d", len(snap.Projects), snap.GlobalCount())
	}
	orig := snap.Find(filepath.Join(shared, "pdf"))
	if orig == nil {
		t.Fatal("pdf not found")
	}
	links := snap.LinksTo(orig.RealPath, orig.Path)
	if len(links) != 2 {
		t.Fatalf("links to pdf: %d", len(links))
	}
	into := snap.LinksInto(orig.RealPath, orig.Path)
	if len(into) != 3 {
		t.Fatalf("links into pdf: %d", len(into))
	}
	// Seen from the project: its own copy, its link and the two global ones.
	own := snap.Find(filepath.Join(proj, ".agents", "skills", "pdf"))
	same := snap.Same(own, proj)
	if len(same) != 3 {
		t.Fatalf("same name from the project: %d", len(same))
	}
	if len(snap.Same(orig, "")) != 1 {
		t.Fatalf("same name globally: %d", len(snap.Same(orig, "")))
	}
	if snap.Locations("") == nil || snap.Locations(proj) == nil || snap.Locations("/nope") != nil {
		t.Fatal("Locations")
	}
}

func TestFrontmatter(t *testing.T) {
	doc := "---\nname: my-skill\ndescription: >\n  Folded text\n  over lines.\nallowed-tools:\n  - Read\n  - Bash\nmetadata:\n  version: 2\n---\n# Body\n"
	f := ParseFrontmatter(doc)
	if !f.Present || f.Err != "" || f.Name != "my-skill" || f.Description != "Folded text over lines." {
		t.Fatalf("frontmatter: %+v", f)
	}
	if len(f.Fields) != 4 || f.Fields[2].Key != "allowed-tools" || !f.Fields[2].Multiline || f.Fields[2].Value != "- Read\n- Bash" {
		t.Fatalf("fields: %+v", f.Fields)
	}
	if v, _ := f.Get("metadata"); v != "version: 2" {
		t.Fatalf("metadata %q", v)
	}
	if f.Body != "# Body\n" || f.BodyLine != 12 {
		t.Fatalf("body %q line %d", f.Body, f.BodyLine)
	}
	if ps := Validate("my-skill", f); len(ps) != 0 {
		t.Fatalf("problems %+v", ps)
	}

	crlf := ParseFrontmatter("---\r\nname: a\r\ndescription: b\r\n---\r\nbody")
	if crlf.Name != "a" || crlf.Description != "b" || crlf.Body != "body" {
		t.Fatalf("crlf: %+v", crlf)
	}
	bom := ParseFrontmatter("\ufeff---\nname: a\ndescription: b\n---\n")
	if bom.Name != "a" {
		t.Fatalf("bom: %+v", bom)
	}
	none := ParseFrontmatter("# Just markdown\n")
	if none.Present || none.Body != "# Just markdown\n" {
		t.Fatalf("none: %+v", none)
	}
	unclosed := ParseFrontmatter("---\nname: a\n")
	if unclosed.Present {
		t.Fatalf("unclosed: %+v", unclosed)
	}
	bad := ParseFrontmatter("---\nname: [a\n---\n")
	if !bad.Present || bad.Err == "" {
		t.Fatalf("bad yaml: %+v", bad)
	}
	list := ParseFrontmatter("---\n- a\n---\n")
	if list.Err == "" {
		t.Fatal("a list frontmatter was accepted")
	}
}

func TestValidate(t *testing.T) {
	msgs := func(ps []Problem) string {
		var s []string
		for _, p := range ps {
			s = append(s, p.Message)
		}
		return strings.Join(s, " | ")
	}
	cases := []struct {
		dir, doc, want string
	}{
		{"x", "no frontmatter", "no frontmatter"},
		{"x", "---\nname: [\n---\n", "not valid YAML"},
		{"x", "---\ndescription: d\n---\n", "no name"},
		{"x", "---\nname: x\n---\n", "no description"},
		{"x", "---\nname: y\ndescription: d\n---\n", "differs"},
		{"Bad_Name", "---\nname: Bad_Name\ndescription: d\n---\n", "lowercase"},
		{"x", "---\nname: x\ndescription: " + strings.Repeat("a", 1025) + "\n---\n", "1024"},
		{strings.Repeat("a", 65), "---\nname: " + strings.Repeat("a", 65) + "\ndescription: d\n---\n", "64"},
	}
	for _, c := range cases {
		got := msgs(Validate(c.dir, ParseFrontmatter(c.doc)))
		if !strings.Contains(got, c.want) {
			t.Errorf("Validate(%q) = %q, want %q", c.doc[:min(len(c.doc), 30)], got, c.want)
		}
	}
}

func TestSetNameAndTemplate(t *testing.T) {
	doc := "---\nname: old\ndescription: d\n---\nname: body stays\n"
	got, ok := SetName(doc, "new")
	if !ok || got != "---\nname: new\ndescription: d\n---\nname: body stays\n" {
		t.Fatalf("SetName: %q %v", got, ok)
	}
	if _, ok := SetName("---\ndescription: d\n---\n", "n"); ok {
		t.Fatal("SetName without a name line")
	}
	if _, ok := SetName("plain", "n"); ok {
		t.Fatal("SetName without frontmatter")
	}
	crlf, ok := SetName("---\r\nname: old\r\ndescription: d\r\n---\r\n", "new")
	if !ok || crlf != "---\r\nname: new\r\ndescription: d\r\n---\r\n" {
		t.Fatalf("SetName crlf %q", crlf)
	}
	tpl := Template("pdf-tools", "Work with PDFs: forms, text.")
	f := ParseFrontmatter(tpl)
	if f.Name != "pdf-tools" || f.Description != "Work with PDFs: forms, text." || !strings.Contains(f.Body, "# Pdf Tools") {
		t.Fatalf("template: %q -> %+v", tpl, f)
	}
	if ps := Validate("pdf-tools", f); len(ps) != 0 {
		t.Fatalf("template problems %+v", ps)
	}
}

func TestCheckName(t *testing.T) {
	for _, bad := range []string{"", " ", "a/b", `a\b`, ".", "..", ".hidden", " a"} {
		if CheckName(bad) == nil {
			t.Errorf("CheckName(%q) accepted", bad)
		}
	}
	for _, good := range []string{"a", "my-skill", "Skill 2"} {
		if err := CheckName(good); err != nil {
			t.Errorf("CheckName(%q): %v", good, err)
		}
	}
}

func TestCreateSkill(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "proj", ".pi", "skills") // created on the way
	p, err := CreateSkill(dir, "new-one", "Does things.")
	if err != nil {
		t.Fatal(err)
	}
	l := ReadLocation(AgentPi, ProjectScope, filepath.Dir(filepath.Dir(dir)))
	s := byName(l, "new-one")
	if s == nil || s.Path != p || len(s.Problems) != 0 {
		t.Fatalf("created: %+v", s)
	}
	if _, err := CreateSkill(dir, "new-one", ""); err == nil {
		t.Fatal("created over an existing skill")
	}
	if _, err := CreateSkill(dir, "a/b", ""); err == nil {
		t.Fatal("created with a slash")
	}
}

func TestWriteFileThroughLinks(t *testing.T) {
	dir := t.TempDir()
	write(t, filepath.Join(dir, "shared", "s", "SKILL.md"), "old")
	os.Chmod(filepath.Join(dir, "shared", "s", "SKILL.md"), 0o600)
	link := filepath.Join(dir, "proj", "skills", "s")
	symlink(t, filepath.Join(dir, "shared", "s"), link)
	// A file that is itself a link.
	write(t, filepath.Join(dir, "elsewhere.md"), "outside")
	symlink(t, filepath.Join(dir, "elsewhere.md"), filepath.Join(dir, "shared", "s", "ref.md"))

	if err := WriteFile(filepath.Join(link, "SKILL.md"), []byte("new")); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(filepath.Join(dir, "shared", "s", "SKILL.md"))
	if string(got) != "new" {
		t.Fatalf("content %q", got)
	}
	if fi, _ := os.Lstat(link); fi.Mode()&os.ModeSymlink == 0 {
		t.Fatal("the link was replaced")
	}
	if fi, _ := os.Stat(filepath.Join(dir, "shared", "s", "SKILL.md")); fi.Mode().Perm() != 0o600 {
		t.Fatalf("mode %v", fi.Mode().Perm())
	}
	if err := WriteFile(filepath.Join(link, "ref.md"), []byte("changed")); err != nil {
		t.Fatal(err)
	}
	if fi, _ := os.Lstat(filepath.Join(dir, "shared", "s", "ref.md")); fi.Mode()&os.ModeSymlink == 0 {
		t.Fatal("the file link was replaced")
	}
	if got, _ := os.ReadFile(filepath.Join(dir, "elsewhere.md")); string(got) != "changed" {
		t.Fatalf("linked file %q", got)
	}
	// A new file through the link.
	if err := WriteFile(filepath.Join(link, "new.md"), []byte("n")); err != nil {
		t.Fatal(err)
	}
	if got, _ := os.ReadFile(filepath.Join(dir, "shared", "s", "new.md")); string(got) != "n" {
		t.Fatalf("new file %q", got)
	}
	// No temporary files left.
	entries, _ := os.ReadDir(filepath.Join(dir, "shared", "s"))
	for _, e := range entries {
		if strings.Contains(e.Name(), "skillhub") {
			t.Fatalf("left %s", e.Name())
		}
	}
}

func TestCreateLinkAndRetarget(t *testing.T) {
	dir := t.TempDir()
	a := filepath.Join(dir, "lib", "a")
	b := filepath.Join(dir, "lib", "b")
	write(t, filepath.Join(a, "SKILL.md"), skillMD("a", "A."))
	write(t, filepath.Join(b, "SKILL.md"), skillMD("b", "B."))
	dest := filepath.Join(dir, "proj", ".claude", "skills")

	p, err := CreateLink(dest, "a", a)
	if err != nil {
		t.Fatal(err)
	}
	if tgt, _ := os.Readlink(p); tgt != a {
		t.Fatalf("target %q", tgt)
	}
	if _, err := CreateLink(dest, "a", a); err == nil {
		t.Fatal("linked over an existing entry")
	}
	if _, err := CreateLink(dest, "rel", "lib/a"); err == nil {
		t.Fatal("linked to a relative path")
	}
	if _, err := CreateLink(filepath.Join(a, "inner"), "self", a); err == nil {
		t.Fatal("linked a skill into itself")
	}

	if err := Retarget(p, b); err != nil {
		t.Fatal(err)
	}
	if tgt, _ := os.Readlink(p); tgt != b {
		t.Fatalf("retargeted to %q", tgt)
	}
	if err := Retarget(a, b); err == nil {
		t.Fatal("retargeted a folder")
	}
	if err := Retarget(p, "../../../lib/a"); err != nil {
		t.Fatal(err)
	}
	if tgt, _ := os.Readlink(p); tgt != "../../../lib/a" || real(t, p) != real(t, a) {
		t.Fatalf("relative retarget %q", tgt)
	}
	// A broken link can be retargeted.
	broken := filepath.Join(dest, "broken")
	symlink(t, filepath.Join(dir, "missing"), broken)
	if err := Retarget(broken, a); err != nil {
		t.Fatal(err)
	}
	if real(t, broken) != real(t, a) {
		t.Fatal("broken link not fixed")
	}

	if w, err := CheckTarget(a); w != "" || err != nil {
		t.Fatalf("CheckTarget(a) %q %v", w, err)
	}
	if w, err := CheckTarget(filepath.Join(dir, "lib")); w == "" || err != nil {
		t.Fatalf("CheckTarget(lib) %q %v", w, err)
	}
	if _, err := CheckTarget(filepath.Join(dir, "nope")); err == nil {
		t.Fatal("CheckTarget(missing)")
	}
	if _, err := CheckTarget(filepath.Join(a, "SKILL.md")); err == nil {
		t.Fatal("CheckTarget(file)")
	}
	if _, err := CheckTarget("rel"); err == nil {
		t.Fatal("CheckTarget(relative)")
	}
}

func TestMaterialize(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "lib", "a")
	write(t, filepath.Join(src, "SKILL.md"), skillMD("a", "A."))
	write(t, filepath.Join(src, "scripts", "run.sh"), "#!/bin/sh\n")
	os.Chmod(filepath.Join(src, "scripts", "run.sh"), 0o755)
	symlink(t, "../SKILL.md", filepath.Join(src, "refs", "skill.md"))
	link := filepath.Join(dir, "proj", "skills", "a")
	symlink(t, src, link)

	if err := Materialize(link); err != nil {
		t.Fatal(err)
	}
	fi, err := os.Lstat(link)
	if err != nil || !fi.IsDir() {
		t.Fatalf("not a folder: %v %v", fi, err)
	}
	if got, _ := os.ReadFile(filepath.Join(link, "SKILL.md")); string(got) != skillMD("a", "A.") {
		t.Fatalf("copied SKILL.md %q", got)
	}
	if fi, _ := os.Stat(filepath.Join(link, "scripts", "run.sh")); fi.Mode().Perm() != 0o755 {
		t.Fatalf("mode %v", fi.Mode())
	}
	if tgt, _ := os.Readlink(filepath.Join(link, "refs", "skill.md")); tgt != "../SKILL.md" {
		t.Fatalf("inner link %q", tgt)
	}
	// The source is untouched and separate.
	write(t, filepath.Join(link, "SKILL.md"), "changed")
	if got, _ := os.ReadFile(filepath.Join(src, "SKILL.md")); string(got) != skillMD("a", "A.") {
		t.Fatal("the source changed")
	}
	if err := Materialize(link); err == nil {
		t.Fatal("materialized a folder")
	}
	broken := filepath.Join(dir, "proj", "skills", "b")
	symlink(t, filepath.Join(dir, "missing"), broken)
	if err := Materialize(broken); err == nil {
		t.Fatal("materialized a broken link")
	}
	if fi, _ := os.Lstat(broken); fi.Mode()&os.ModeSymlink == 0 {
		t.Fatal("the broken link went")
	}
}

func TestConvertToLink(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "proj", ".claude", "skills", "a")
	write(t, filepath.Join(p, "SKILL.md"), skillMD("a", "A."))
	shared := filepath.Join(dir, "home", ".agents", "skills")

	dest, err := ConvertToLink(p, shared)
	if err != nil {
		t.Fatal(err)
	}
	if dest != filepath.Join(shared, "a") {
		t.Fatalf("dest %s", dest)
	}
	if tgt, _ := os.Readlink(p); tgt != dest {
		t.Fatalf("link target %q", tgt)
	}
	if got, _ := os.ReadFile(filepath.Join(p, "SKILL.md")); string(got) != skillMD("a", "A.") {
		t.Fatal("content through the link")
	}
	if _, err := ConvertToLink(p, shared); err == nil {
		t.Fatal("converted a link")
	}
	q := filepath.Join(dir, "proj", ".claude", "skills", "q")
	write(t, filepath.Join(q, "SKILL.md"), skillMD("q", "Q."))
	write(t, filepath.Join(shared, "q", "SKILL.md"), "taken")
	if _, err := ConvertToLink(q, shared); err == nil {
		t.Fatal("moved over an existing folder")
	}
	if _, err := ConvertToLink(q, filepath.Dir(q)); err == nil {
		t.Fatal("moved to where it is")
	}
	if _, err := ConvertToLink(q, filepath.Join(q, "inner")); err == nil {
		t.Fatal("moved into itself")
	}
	if fi, _ := os.Lstat(q); !fi.IsDir() {
		t.Fatal("q was changed by a failed move")
	}
}

func TestRenameAndRemove(t *testing.T) {
	dir := t.TempDir()
	skills := filepath.Join(dir, "skills")
	write(t, filepath.Join(skills, "a", "SKILL.md"), skillMD("a", "A."))
	write(t, filepath.Join(skills, "b", "SKILL.md"), skillMD("b", "B."))
	symlink(t, filepath.Join(skills, "a"), filepath.Join(skills, "l"))

	np, err := Rename(filepath.Join(skills, "a"), "c")
	if err != nil || np != filepath.Join(skills, "c") || !exists(np) {
		t.Fatalf("rename: %s %v", np, err)
	}
	if _, err := Rename(np, "b"); err == nil {
		t.Fatal("renamed over b")
	}
	if _, err := Rename(np, "x/y"); err == nil {
		t.Fatal("renamed with a slash")
	}
	if p, err := Rename(np, "c"); err != nil || p != np {
		t.Fatal("renaming to the same name")
	}
	// Names differing in case are other entries where the disk tells case.
	write(t, filepath.Join(skills, "Up", "SKILL.md"), skillMD("Up", "U."))
	// On a case-insensitive disk "up" is "Up", and the link is not made.
	if os.Symlink(filepath.Join(skills, "c"), filepath.Join(skills, "up")) == nil {
		if _, err := Rename(filepath.Join(skills, "Up"), "up"); err == nil {
			t.Fatal("renamed over another entry differing in case")
		}
		if !isLinkT(filepath.Join(skills, "up")) {
			t.Fatal("the link was replaced")
		}
	}

	var trashed []string
	trash := func(p string) error { trashed = append(trashed, p); return os.RemoveAll(p) }
	if err := Remove(filepath.Join(skills, "l"), trash); err != nil {
		t.Fatal(err)
	}
	if len(trashed) != 0 || exists(filepath.Join(skills, "l")) || !exists(np) {
		t.Fatal("removing a link trashed something, or left the link")
	}
	if err := Remove(np, trash); err != nil {
		t.Fatal(err)
	}
	if len(trashed) != 1 || trashed[0] != np {
		t.Fatalf("trashed %v", trashed)
	}
	if err := Remove(filepath.Join(skills, "b"), func(string) error { return errors.New("no trash") }); err == nil {
		t.Fatal("trash error ignored")
	}
	if !exists(filepath.Join(skills, "b")) {
		t.Fatal("b went though the trash failed")
	}
	if err := Remove(filepath.Join(skills, "b"), nil); err == nil {
		t.Fatal("removed a folder without a trash")
	}
}

func TestFiles(t *testing.T) {
	dir := t.TempDir()
	s := filepath.Join(dir, "lib", "s")
	write(t, filepath.Join(s, "SKILL.md"), "x")
	write(t, filepath.Join(s, "a.md"), "a")
	write(t, filepath.Join(s, "scripts", "run.py"), "print()")
	write(t, filepath.Join(s, ".git", "HEAD"), "ref")
	symlink(t, filepath.Join(dir, "nowhere"), filepath.Join(s, "dead.md"))
	symlink(t, filepath.Join(s, "a.md"), filepath.Join(s, "alias.md"))
	link := filepath.Join(dir, "skills", "s")
	symlink(t, s, link)

	files, err := ListFiles(link)
	if err != nil {
		t.Fatal(err)
	}
	var rels []string
	for _, f := range files {
		rels = append(rels, f.Rel)
	}
	if strings.Join(rels, ",") != "SKILL.md,a.md,alias.md,dead.md,scripts,scripts/run.py" {
		t.Fatalf("files %v", rels)
	}
	if files[0].Path != filepath.Join(link, "SKILL.md") {
		t.Fatalf("paths are not through the link: %s", files[0].Path)
	}
	if f := files[2]; !f.IsLink || f.Broken || f.Target != filepath.Join(s, "a.md") {
		t.Fatalf("alias %+v", f)
	}
	if f := files[3]; !f.IsLink || !f.Broken {
		t.Fatalf("dead %+v", f)
	}
	if f := files[5]; f.Depth != 1 || f.IsDir {
		t.Fatalf("run.py %+v", f)
	}

	p, err := NewFile(link, "references/guide.md")
	if err != nil || !exists(filepath.Join(s, "references", "guide.md")) || p != filepath.Join(link, "references", "guide.md") {
		t.Fatalf("NewFile %s %v", p, err)
	}
	for _, bad := range []string{"", "../escape.md", "/abs.md", "a.md"} {
		if _, err := NewFile(link, bad); err == nil {
			t.Errorf("NewFile(%q) accepted", bad)
		}
	}
	if !IsText([]byte("hello")) || IsText([]byte{'a', 0, 'b'}) {
		t.Fatal("IsText")
	}
}

func isLinkT(p string) bool {
	fi, err := os.Lstat(p)
	return err == nil && fi.Mode()&os.ModeSymlink != 0
}
