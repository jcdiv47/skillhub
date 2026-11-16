package main

import (
	"image/png"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"github.com/egoist/mygo/ui"
	"github.com/egoist/mygo/ui/editor"

	"skillhub/internal/config"
	"skillhub/internal/skills"
)

// fixture is a home folder with global skills, two projects and links
// between them.
type fixture struct {
	home, code, trashDir string
	trashed              []string
	revealed             []string
	pick                 []string
	a                    *app
	tt                   *ui.Tester
}

func put(t *testing.T, p, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func link(t *testing.T, target, p string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(target, p); err != nil {
		t.Fatal(err)
	}
}

func md(name, desc, body string) string {
	return "---\nname: " + name + "\ndescription: " + desc + "\nallowed-tools:\n  - Read\n  - Bash\n---\n" + body
}

func newFixture(t *testing.T, roots bool) *fixture {
	t.Helper()
	tmp, err := filepath.EvalSymlinks(t.TempDir()) // /tmp is a link on macOS
	if err != nil {
		t.Fatal(err)
	}
	f := &fixture{home: filepath.Join(tmp, "home"), trashDir: filepath.Join(tmp, "trash")}
	f.code = filepath.Join(f.home, "Code")
	h := f.home
	put(t, filepath.Join(h, ".agents", "skills", "pdf", "SKILL.md"), md("pdf", "Fill and read PDF forms.", "# PDF\n\nUse **pdftk** for forms. See [the guide](references/guide.md).\n\n```bash\npdftk in.pdf dump_data\n```\n"))
	put(t, filepath.Join(h, ".agents", "skills", "pdf", "references", "guide.md"), "# Guide\n")
	put(t, filepath.Join(h, ".agents", "skills", "review", "SKILL.md"), md("review", "Review code changes.", "# Review\n"))
	link(t, filepath.Join(h, ".agents", "skills", "pdf"), filepath.Join(h, ".claude", "skills", "pdf"))
	put(t, filepath.Join(h, ".claude", "skills", "notes", "SKILL.md"), "no frontmatter here\n")
	put(t, filepath.Join(h, ".pi", "agent", "skills", "pi-only", "SKILL.md"), md("pi-only", "Only for pi.", "# Pi\n"))

	web := filepath.Join(f.code, "web")
	put(t, filepath.Join(web, "package.json"), "{}")
	put(t, filepath.Join(web, ".claude", "skills", "deploy", "SKILL.md"), md("deploy", "Deploy the site.", "# Deploy\n"))
	link(t, filepath.Join(h, ".agents", "skills", "pdf"), filepath.Join(web, ".claude", "skills", "pdf"))
	link(t, filepath.Join(h, "gone"), filepath.Join(web, ".claude", "skills", "old"))
	put(t, filepath.Join(web, ".pi", "skills", "lint", "SKILL.md"), md("lint", "Lint the code.", "# Lint\n"))

	api := filepath.Join(f.code, "api")
	put(t, filepath.Join(api, ".agents", "skills", "review", "SKILL.md"), md("review", "Project review rules.", "# Review here\n"))

	if roots {
		c := config.Config{Roots: []string{f.code}, Depth: 3}
		if err := c.Save(filepath.Join(tmp, "config.json")); err != nil {
			t.Fatal(err)
		}
	}
	f.a = newApp(env{
		home:       h,
		configPath: filepath.Join(tmp, "config.json"),
		pickFolders: func(title string, multiple bool, done func([]string)) {
			done(f.pick)
		},
		trash: func(p string) error {
			f.trashed = append(f.trashed, p)
			os.MkdirAll(f.trashDir, 0o755)
			return os.Rename(p, filepath.Join(f.trashDir, filepath.Base(p)))
		},
		reveal:     func(p string) { f.revealed = append(f.revealed, p) },
		background: func(work func() func()) { work()() },
	})
	f.tt = ui.NewTester(f.a.view, 1320, 840)
	return f
}

func (f *fixture) click(t *testing.T, label string) {
	t.Helper()
	if err := f.tt.Click(label); err != nil {
		t.Fatalf("click %q: %v\ntexts: %q", label, err, f.tt.Texts())
	}
}

func (f *fixture) want(t *testing.T, texts ...string) {
	t.Helper()
	for _, s := range texts {
		if !f.tt.HasText(s) {
			t.Errorf("no text %q in %q", s, f.tt.Texts())
		}
	}
}

func (f *fixture) wantNot(t *testing.T, texts ...string) {
	t.Helper()
	for _, s := range texts {
		if f.tt.HasText(s) {
			t.Errorf("unexpected text %q", s)
		}
	}
}

// shot saves the frame, for looking at the app as it draws.
func (f *fixture) shot(t *testing.T, name string) {
	dir := os.Getenv("SKILLHUB_SHOTS")
	if dir == "" {
		return
	}
	os.MkdirAll(dir, 0o755)
	out, err := os.Create(filepath.Join(dir, name+".png"))
	if err != nil {
		t.Fatal(err)
	}
	defer out.Close()
	png.Encode(out, f.tt.Image())
}

func isLink(p string) bool {
	fi, err := os.Lstat(p)
	return err == nil && fi.Mode()&os.ModeSymlink != 0
}

func TestLandingChoosesFolders(t *testing.T) {
	f := newFixture(t, false)
	f.want(t, "Welcome to SkillHub", "No folders chosen yet.")
	f.shot(t, "landing")
	f.pick = []string{f.code}
	f.click(t, "Choose Folders…")
	f.want(t, "~/Code")
	f.click(t, "Scan")
	f.wantNot(t, "Welcome to SkillHub")
	f.want(t, "Global skills", "web", "api", "PROJECTS · 2")
	c, err := config.Load(f.a.env.configPath)
	if err != nil || !slices.Equal(c.Roots, []string{f.code}) {
		t.Fatalf("saved config %+v %v", c, err)
	}
	f.shot(t, "global")
}

func TestBrowseGlobalAndProjects(t *testing.T) {
	f := newFixture(t, true)
	f.want(t, "Global skills", "AGENTS", "CLAUDE", "PI", "pdf", "review", "notes", "pi-only", "Fill and read PDF forms.")
	// pdf is in .agents and linked from .claude: two of the name globally.
	f.want(t, "×2")

	f.click(t, "web")
	f.want(t, "deploy", "lint", "old", "Deploy the site.")
	f.wantNot(t, "pi-only", "notes")
	f.shot(t, "project")

	// The broken link.
	f.click(t, "old")
	f.want(t, "The link points to nothing.", "Links to", "Remove Link", "Change Target…")
	// The link to the global pdf skill: where it points, and its real path.
	f.click(t, "pdf")
	f.want(t, "Links to", "Real path", "~/Code/web/.claude/skills/pdf", "~/.agents/skills/pdf", "Make Real Copy")
	f.want(t, "allowed-tools", "- Read\n- Bash", "Use ", "pdftk")
	f.shot(t, "preview-link")

	// Its links: the two other links to ~/.agents/skills/pdf.
	f.click(t, "Links (1)")
	f.want(t, "~/.claude/skills/pdf", "The real folder:")
	f.click(t, "Show ~/.claude/skills/pdf")
	if f.a.scope != "" || f.a.skillPath != filepath.Join(f.home, ".claude", "skills", "pdf") {
		t.Fatalf("show went to %q %q", f.a.scope, f.a.skillPath)
	}

	// Copy and reveal the real path.
	f.click(t, "Reveal real path")
	if len(f.revealed) != 1 || f.revealed[0] != filepath.Join(f.home, ".agents", "skills", "pdf") {
		t.Fatalf("revealed %v", f.revealed)
	}
	f.click(t, "Copy real path")
	if got := f.tt.Clipboard(); got != filepath.Join(f.home, ".agents", "skills", "pdf") {
		t.Fatalf("clipboard %q", got)
	}

	// A skill without frontmatter shows its problem.
	f.click(t, "notes")
	f.want(t, "SKILL.md has no frontmatter (--- name/description --- at the top).")

	// Filters.
	f.tt.Click("Filter projects")
	f.tt.Type("ap")
	f.want(t, "api")
	f.wantNot(t, "web")
}

func TestDuplicateAcrossScopes(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "api")
	f.want(t, "review", "×2")
	if same := f.a.snap.Same(f.a.snap.Find(filepath.Join(f.code, "api", ".agents", "skills", "review")), f.a.scope); len(same) != 1 {
		t.Fatalf("same %d", len(same))
	}
}

func TestPreviewLinkOpensFile(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "pdf")
	f.a.followLink("references/guide.md")
	f.tt.Frame()
	if f.a.tab != tabFiles || f.a.filePath != filepath.Join(f.home, ".agents", "skills", "pdf", "references", "guide.md") {
		t.Fatalf("tab %d file %s", f.a.tab, f.a.filePath)
	}
	f.a.followLink("../../../escape")
	if strings.Contains(f.a.filePath, "escape") {
		t.Fatal("followed a link out of the skill")
	}
}

func TestEditAndSaveThroughLink(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "web")
	f.click(t, "pdf")
	f.click(t, "Files")
	f.want(t, "SKILL.md", "references", "guide.md", "Save", "Revert")
	if f.a.edit == nil || f.a.dirty() {
		t.Fatal("SKILL.md not open, or dirty")
	}
	f.a.edit.Buffer.Edit(editor.Edit{Start: 0, End: 0, Text: ""})
	text := f.a.edit.Buffer.String()
	f.a.edit.Buffer.Set(strings.Replace(text, "# PDF", "# PDF tools", 1))
	f.tt.Frame()
	f.want(t, "Edited")
	f.shot(t, "editor")
	f.tt.Key(ui.Cmd, ui.KeyS)
	if f.a.dirty() {
		t.Fatal("still dirty after Cmd+S")
	}
	got, _ := os.ReadFile(filepath.Join(f.home, ".agents", "skills", "pdf", "SKILL.md"))
	if !strings.Contains(string(got), "# PDF tools") {
		t.Fatalf("saved %q", got)
	}
	if !isLink(filepath.Join(f.code, "web", ".claude", "skills", "pdf")) {
		t.Fatal("the link was replaced by the save")
	}
	f.want(t, "Saved SKILL.md")

	// A file changed on disk shows in the editor when it has no edits.
	put(t, filepath.Join(f.home, ".agents", "skills", "pdf", "SKILL.md"), md("pdf", "Changed elsewhere.", "# Elsewhere\n"))
	f.a.reload()
	f.tt.Frame()
	if !strings.Contains(f.a.edit.Buffer.String(), "Changed elsewhere.") || f.a.dirty() {
		t.Fatal("the editor kept the old text")
	}
	// With edits, it says so instead.
	f.a.edit.Buffer.Set("mine")
	put(t, filepath.Join(f.home, ".agents", "skills", "pdf", "SKILL.md"), md("pdf", "Again.", ""))
	f.a.reload()
	f.tt.Frame()
	f.want(t, "The file changed on disk.")
	if f.a.edit.Buffer.String() != "mine" {
		t.Fatal("edits lost")
	}
	f.click(t, "Load From Disk")
	if !strings.Contains(f.a.edit.Buffer.String(), "Again.") {
		t.Fatal("not loaded from disk")
	}
}

func TestUnsavedGuard(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "review")
	f.click(t, "Files")
	f.a.edit.Buffer.Set("changed")
	f.tt.Frame()
	f.click(t, "pi-only")
	f.want(t, "Save changes to SKILL.md?")
	if f.a.skillPath != filepath.Join(f.home, ".agents", "skills", "review") {
		t.Fatal("moved on with edits")
	}
	f.click(t, "Cancel")
	if !f.a.dirty() {
		t.Fatal("edits lost on cancel")
	}
	f.click(t, "pi-only")
	f.click(t, "Discard")
	if f.a.skillPath != filepath.Join(f.home, ".pi", "agent", "skills", "pi-only") {
		t.Fatalf("at %s", f.a.skillPath)
	}
	got, _ := os.ReadFile(filepath.Join(f.home, ".agents", "skills", "review", "SKILL.md"))
	if string(got) == "changed" {
		t.Fatal("discarded edits were saved")
	}
	// Save from the guard.
	f.click(t, "Files")
	f.a.edit.Buffer.Set(md("pi-only", "Saved from the guard.", ""))
	f.tt.Frame()
	f.click(t, "web")
	f.want(t, "Save changes to SKILL.md?")
	f.tt.Key(0, ui.KeyEnter) // Save, the default button
	if f.a.scope != filepath.Join(f.code, "web") {
		t.Fatalf("scope %q after saving", f.a.scope)
	}
	got, _ = os.ReadFile(filepath.Join(f.home, ".pi", "agent", "skills", "pi-only", "SKILL.md"))
	if !strings.Contains(string(got), "Saved from the guard.") {
		t.Fatalf("saved %q", got)
	}
}

func TestNewSkill(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "web")
	// .agents/skills does not exist in web yet.
	f.want(t, "Not created")
	f.click(t, "New skill in Agents · .agents/skills")
	f.want(t, "New Skill")
	f.tt.Type("data-viz")
	f.tt.Key(0, ui.KeyTab)
	f.tt.Type("Draw charts.")
	f.want(t, "Creates ~/Code/web/.agents/skills/data-viz/SKILL.md")
	f.shot(t, "new-skill")
	f.click(t, "Create")
	p := filepath.Join(f.code, "web", ".agents", "skills", "data-viz")
	data, err := os.ReadFile(filepath.Join(p, "SKILL.md"))
	if err != nil {
		t.Fatal(err)
	}
	if m := skills.ParseFrontmatter(string(data)); m.Name != "data-viz" || m.Description != "Draw charts." {
		t.Fatalf("frontmatter %+v", m)
	}
	if f.a.skillPath != p || f.a.tab != tabFiles || f.a.edit == nil {
		t.Fatalf("not opened for editing: %s tab %d", f.a.skillPath, f.a.tab)
	}
	// An existing name is refused.
	f.click(t, "New skill in Claude · .claude/skills")
	f.tt.Type("deploy")
	f.want(t, "~/Code/web/.claude/skills/deploy already exists")
}

func TestLinkInto(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "review")
	f.click(t, "Link Into…")
	// Globally, the skill is in .agents already.
	f.want(t, "Agents · .agents/skills — the skill is here", "Claude · .claude/skills", "Pi · .pi/agent/skills")
	f.click(t, "Claude · .claude/skills")
	f.click(t, "Pi · .pi/agent/skills")
	f.shot(t, "link-into")
	f.click(t, "Create Links")
	for _, p := range []string{
		filepath.Join(f.home, ".claude", "skills", "review"),
		filepath.Join(f.home, ".pi", "agent", "skills", "review"),
	} {
		if tgt, err := os.Readlink(p); err != nil || tgt != filepath.Join(f.home, ".agents", "skills", "review") {
			t.Fatalf("%s -> %q %v", p, tgt, err)
		}
	}
	// Into a project: choose its scope in the dialog.
	f.a.linkScope = filepath.Join(f.code, "web")
	f.click(t, "Link Into…")
	f.a.linkScope = filepath.Join(f.code, "web")
	f.tt.Frame()
	f.want(t, "Pi · .pi/skills")
	f.click(t, "Pi · .pi/skills")
	f.click(t, "Create Links")
	if !isLink(filepath.Join(f.code, "web", ".pi", "skills", "review")) {
		t.Fatal("no link in web/.pi/skills")
	}
	f.click(t, "Links (3)")
	f.want(t, "~/Code/web/.pi/skills/review")
}

func TestRetargetBrokenLink(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "web")
	f.click(t, "old")
	f.click(t, "Change Target…")
	f.want(t, "the folder does not exist")
	f.a.retargetPath = ""
	f.tt.Frame()
	f.click(t, "Target folder")
	f.tt.Type("~/.agents/skills/review")
	f.tt.Frame()
	f.wantNot(t, "the folder does not exist")
	f.click(t, "Change")
	p := filepath.Join(f.code, "web", ".claude", "skills", "old")
	if tgt, _ := os.Readlink(p); tgt != filepath.Join(f.home, ".agents", "skills", "review") {
		t.Fatalf("target %q", tgt)
	}
	f.wantNot(t, "The link points to nothing.")
	// Choosing the folder in the dialog.
	f.click(t, "Change Target…")
	f.pick = []string{filepath.Join(f.home, ".agents", "skills", "pdf")}
	f.click(t, "Choose…")
	if f.a.retargetPath != f.pick[0] {
		t.Fatalf("picked %q", f.a.retargetPath)
	}
	f.click(t, "Change")
	if tgt, _ := os.Readlink(p); tgt != f.pick[0] {
		t.Fatalf("target %q", tgt)
	}
}

func TestMakeRealCopy(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "web")
	f.click(t, "pdf")
	f.click(t, "Make Real Copy")
	f.want(t, "Replace the link with a copy?")
	f.click(t, "Make Copy")
	p := filepath.Join(f.code, "web", ".claude", "skills", "pdf")
	if isLink(p) {
		t.Fatal("still a link")
	}
	if _, err := os.Stat(filepath.Join(p, "references", "guide.md")); err != nil {
		t.Fatal(err)
	}
	f.want(t, "Move & Link…", "Move to Trash")
}

func TestMoveAndLink(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "web")
	f.click(t, "deploy")
	f.click(t, "Move & Link…")
	f.want(t, "~/.agents/skills/deploy  ←  link at ~/Code/web/.claude/skills/deploy")
	f.click(t, "Move & Link")
	p := filepath.Join(f.code, "web", ".claude", "skills", "deploy")
	if tgt, _ := os.Readlink(p); tgt != filepath.Join(f.home, ".agents", "skills", "deploy") {
		t.Fatalf("target %q", tgt)
	}
	f.want(t, "Links to", "Real path")
	// Moving where a skill of the name is refused.
	f.click(t, "Global")
	f.click(t, "notes")
	f.click(t, "Move & Link…")
	put(t, filepath.Join(f.home, ".agents", "skills", "notes", "SKILL.md"), "taken")
	f.tt.Frame()
	f.want(t, "~/.agents/skills/notes already exists")
}

func TestRenameUpdatesLinks(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "pdf") // the global real folder, linked from two places
	if f.a.skillPath != filepath.Join(f.home, ".agents", "skills", "pdf") {
		t.Fatalf("chose %s", f.a.skillPath)
	}
	f.click(t, "Rename…")
	f.want(t, "Also change name: in SKILL.md", "Point the 2 links to this folder at its new name")
	f.tt.Key(ui.Cmd, ui.KeyA)
	f.tt.Type("pdf-forms")
	f.click(t, "Rename")
	np := filepath.Join(f.home, ".agents", "skills", "pdf-forms")
	data, err := os.ReadFile(filepath.Join(np, "SKILL.md"))
	if err != nil || !strings.Contains(string(data), "name: pdf-forms") {
		t.Fatalf("SKILL.md %q %v", data, err)
	}
	for _, l := range []string{filepath.Join(f.home, ".claude", "skills", "pdf"), filepath.Join(f.code, "web", ".claude", "skills", "pdf")} {
		if tgt, _ := os.Readlink(l); tgt != np {
			t.Fatalf("%s -> %q", l, tgt)
		}
	}
	if f.a.skillPath != np {
		t.Fatalf("selection %s", f.a.skillPath)
	}
	f.want(t, "Renamed to pdf-forms")
}

func TestRemove(t *testing.T) {
	f := newFixture(t, true)
	// Removing a link removes only the link.
	f.click(t, "web")
	f.click(t, "pdf")
	f.click(t, "Remove Link")
	f.want(t, "Remove the link “pdf”?")
	f.tt.Key(0, ui.KeyEnter)
	if _, err := os.Lstat(filepath.Join(f.code, "web", ".claude", "skills", "pdf")); err == nil {
		t.Fatal("the link stayed")
	}
	if _, err := os.Stat(filepath.Join(f.home, ".agents", "skills", "pdf", "SKILL.md")); err != nil {
		t.Fatal("the target went with the link")
	}
	if len(f.trashed) != 0 {
		t.Fatal("a link went to the trash")
	}
	// A folder others link to: the alert lists them, and it goes to the trash.
	f.click(t, "Global")
	f.click(t, "pdf")
	f.click(t, "Move to Trash")
	f.want(t, "Move “pdf” to the Trash?")
	if !strings.Contains(strings.Join(f.tt.Texts(), "\n"), "1 link points into it and will break") {
		t.Errorf("no warning of links: %q", f.tt.Texts())
	}
	f.shot(t, "delete")
	f.click(t, "Move to Trash")
	if len(f.trashed) != 1 || f.trashed[0] != filepath.Join(f.home, ".agents", "skills", "pdf") {
		t.Fatalf("trashed %v", f.trashed)
	}
	f.want(t, "Moved pdf to the Trash", "No skill selected")
	// The link left behind is now broken.
	if s := f.a.snap.Find(filepath.Join(f.home, ".claude", "skills", "pdf")); s == nil || !s.Broken {
		t.Fatalf("the global link: %+v", s)
	}
}

func TestFilesNewAndDelete(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "review")
	f.click(t, "Files")
	f.click(t, "New file")
	f.tt.Type("scripts/check.sh")
	f.click(t, "Create")
	p := filepath.Join(f.home, ".agents", "skills", "review", "scripts", "check.sh")
	if _, err := os.Stat(p); err != nil {
		t.Fatal(err)
	}
	if f.a.filePath != p {
		t.Fatalf("open %s", f.a.filePath)
	}
	f.want(t, "check.sh", "scripts")
	if err := f.tt.RightClick("scripts/check.sh"); err != nil {
		t.Fatal(err)
	}
	if err := f.tt.ChooseMenuItem("Delete…"); err != nil {
		t.Fatal(err)
	}
	f.want(t, "Move “scripts/check.sh” to the Trash?")
	f.tt.Key(0, ui.KeyEnter)
	if _, err := os.Stat(p); err == nil {
		t.Fatal("the file stayed")
	}
	if f.a.filePath != filepath.Join(f.home, ".agents", "skills", "review", "SKILL.md") {
		t.Fatalf("open after delete: %s", f.a.filePath)
	}
}

func TestBinaryFile(t *testing.T) {
	f := newFixture(t, true)
	put(t, filepath.Join(f.home, ".agents", "skills", "review", "logo.png"), "\x89PNG\x00\x00")
	f.a.reload()
	f.click(t, "review")
	f.click(t, "Files")
	f.click(t, "logo.png")
	f.want(t, "A binary file (6 B).")
	if f.a.edit != nil {
		t.Fatal("a binary file in the editor")
	}
}

func TestSettingsRescan(t *testing.T) {
	f := newFixture(t, true)
	other := filepath.Join(f.home, "Work")
	put(t, filepath.Join(other, "svc", ".pi", "skills", "x", "SKILL.md"), md("x", "X.", ""))
	f.click(t, "Folders…")
	f.want(t, "Folders to Scan", "~/Code")
	f.pick = []string{other}
	f.click(t, "Choose Folders…")
	f.click(t, "Save & Rescan")
	f.want(t, "svc", "PROJECTS · 3")
	f.click(t, "Folders…")
	f.click(t, "Remove ~/Code")
	f.click(t, "Save & Rescan")
	f.want(t, "svc", "PROJECTS · 1")
	f.wantNot(t, "web")
}

func TestNoAccessibilityGaps(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "pdf")
	for _, a := range f.tt.AccessAll() {
		if a.Role == ui.RoleButton && a.Name == "" {
			t.Errorf("a button at %v has no name", a.Bounds)
		}
	}
}

func TestTildeThroughLinkedHome(t *testing.T) {
	dir, _ := filepath.EvalSymlinks(t.TempDir())
	realHome := filepath.Join(dir, "real")
	os.MkdirAll(realHome, 0o755)
	linked := filepath.Join(dir, "linked")
	os.Symlink(realHome, linked)
	a := &app{env: env{home: linked}}
	for in, want := range map[string]string{
		filepath.Join(linked, ".agents"):   "~/.agents",
		filepath.Join(realHome, ".agents"): "~/.agents",
		linked:                             "~",
		"/elsewhere":                       "/elsewhere",
		linked + "x":                       linked + "x",
	} {
		if got := a.tilde(in); got != want {
			t.Errorf("tilde(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestEditsKeptWhenFileRemovedOnDisk(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "review")
	f.click(t, "Files")
	f.a.edit.Buffer.Set(md("review", "Mine.", ""))
	p := filepath.Join(f.home, ".agents", "skills", "review", "SKILL.md")
	os.Remove(p)
	f.a.reload()
	f.tt.Frame()
	if !f.a.dirty() || f.a.filePath != p {
		t.Fatal("the edits went with the file")
	}
	f.want(t, "The file changed on disk.")
	f.tt.Key(ui.Cmd, ui.KeyS)
	if got, _ := os.ReadFile(p); !strings.Contains(string(got), "Mine.") {
		t.Fatalf("saved %q", got)
	}
}

func TestNewFileAsksAboutEdits(t *testing.T) {
	f := newFixture(t, true)
	f.click(t, "review")
	f.click(t, "Files")
	f.a.edit.Buffer.Set("unsaved")
	f.tt.Frame()
	f.click(t, "New file")
	f.want(t, "Save changes to SKILL.md?")
	if f.a.newFileOpen {
		t.Fatal("the dialog opened over unsaved edits")
	}
	f.click(t, "Cancel")
	if !f.a.dirty() {
		t.Fatal("edits lost")
	}
}

func TestCloseAsksAboutEdits(t *testing.T) {
	f := newFixture(t, true)
	quits := 0
	quit := func() { quits++ }
	if !f.a.confirmClose(quit) {
		t.Fatal("asked with nothing to save")
	}
	f.click(t, "review")
	f.click(t, "Files")
	f.a.edit.Buffer.Set("unsaved")
	if f.a.confirmClose(quit) {
		t.Fatal("closed with unsaved edits")
	}
	f.tt.Frame()
	f.want(t, "Save changes to SKILL.md?")
	f.click(t, "Cancel")
	if quits != 0 || !f.a.dirty() {
		t.Fatal("quit after cancel")
	}
	f.a.confirmClose(quit)
	f.tt.Frame()
	f.click(t, "Discard")
	if quits != 1 {
		t.Fatalf("quits %d", quits)
	}
}

func TestRenameUpdatesRelativeLinks(t *testing.T) {
	f := newFixture(t, true)
	rel := filepath.Join(f.home, ".pi", "agent", "skills", "review")
	link(t, "../../../.agents/skills/review", rel)
	f.a.reload()
	f.click(t, "review")
	f.click(t, "Rename…")
	f.want(t, "Point the 1 link to this folder at its new name")
	f.tt.Key(ui.Cmd, ui.KeyA)
	f.tt.Type("review-code")
	f.click(t, "Rename")
	if tgt, _ := os.Readlink(rel); tgt != "../../../.agents/skills/review-code" {
		t.Fatalf("relative link -> %q", tgt)
	}
	if _, err := os.Stat(filepath.Join(rel, "SKILL.md")); err != nil {
		t.Fatal(err)
	}
}
