package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"github.com/egoist/mygo/ui"
	"github.com/egoist/mygo/ui/editor"
	"github.com/egoist/mygo/ui/markdown"

	"skillhub/internal/config"
	"skillhub/internal/skills"
)

// env is what the app needs of the system, replaced in tests.
type env struct {
	home       string
	configPath string
	// pickFolders asks the user for folders and calls done on the UI
	// thread with them, none when canceled.
	pickFolders func(title string, multiple bool, done func([]string))
	trash       skills.Trasher
	reveal      func(path string)
	openURL     func(url string)
	// background runs work off the UI thread, then what it returns on it.
	background func(work func() func())
	// watch watches dirs (recursively) and roots (only their entries),
	// calling changed on the UI thread; it stops the previous watch.
	watch func(dirs, roots []string, changed func())
}

// maxEditSize is the largest file opened in the editor.
const maxEditSize = 4 << 20

// app is SkillHub's state.
type app struct {
	env env
	cfg config.Config

	snap     *skills.Snapshot
	scanning bool
	loaded   bool
	// welcome shows the first-run page until the user scans.
	welcome bool

	// scope is the root of the project shown, "" for the global skills.
	scope      string
	skillPath  string
	projQuery  string
	skillQuery string
	projects   ui.ListState
	projRow    int

	tab int // tabPreview, tabFiles, tabLinks

	files    []skills.File
	filePath string
	edit     *editor.State
	editDisk string // what the open file held on disk when read or saved
	editNote string // why the file is not in the editor
	changed  bool   // the file changed on disk while it had edits

	md markdown.Options

	sidebarWidth, listWidth float32

	notice   string // a toast for the next frame
	realHome string // the home directory with its links resolved

	// Dialogs.
	settingsOpen bool
	newSkillOpen bool
	newSkillLoc  *skills.Location
	newName      string
	newDesc      string

	renameOpen   bool
	renameName   string
	renameMeta   bool
	renameRelink bool

	linkOpen   bool
	linkScope  string
	linkAgents [3]bool
	linkName   string

	retargetOpen bool
	retargetPath string

	materializeOpen bool

	convertOpen bool
	convertDest string

	deleteOpen bool

	newFileOpen bool
	newFileRel  string

	deleteFileOpen bool
	deleteFile     skills.File

	discardOpen bool
	pending     func()

	errOpen  bool
	errTitle string
	errText  string
}

const (
	tabPreview = iota
	tabFiles
	tabLinks
)

func newApp(e env) *app {
	a := &app{env: e, projRow: -1, sidebarWidth: 270, listWidth: 330}
	cfg, err := config.Load(e.configPath)
	if err != nil {
		a.fail("Couldn't read the settings", err)
	}
	a.cfg = cfg
	if cfg.SidebarWidth > 0 {
		a.sidebarWidth = cfg.SidebarWidth
	}
	if cfg.ListWidth > 0 {
		a.listWidth = cfg.ListWidth
	}
	a.md.OnLink = a.followLink
	if len(cfg.Roots) > 0 {
		a.scan()
	} else {
		a.welcome = true
	}
	return a
}

// landing reports whether the app asks for folders to scan.
func (a *app) landing() bool { return a.welcome }

func (a *app) saveConfig() {
	a.cfg.SidebarWidth, a.cfg.ListWidth = a.sidebarWidth, a.listWidth
	if err := a.cfg.Save(a.env.configPath); err != nil {
		a.fail("Couldn't save the settings", err)
	}
}

// scan finds the projects of the scan folders and reads every skill.
func (a *app) scan() {
	if a.scanning {
		return
	}
	a.scanning = true
	roots, depth, home := slices.Clone(a.cfg.Roots), a.cfg.Depth, a.env.home
	a.env.background(func() func() {
		found := skills.FindProjects(roots, depth, home)
		snap := skills.Load(home, found)
		return func() {
			a.scanning = false
			a.setSnapshot(snap)
		}
	})
}

// reload reads the skills of the projects known again, as after a change.
func (a *app) reload() {
	var infos []skills.ProjectInfo
	if a.snap != nil {
		for _, p := range a.snap.Projects {
			if skills.HasAnyLocation(p.Root) {
				infos = append(infos, p.ProjectInfo)
			}
		}
	}
	a.setSnapshot(skills.Load(a.env.home, infos))
}

func (a *app) setSnapshot(s *skills.Snapshot) {
	a.snap = s
	a.loaded = true
	if a.scope != "" && s.Project(a.scope) == nil {
		a.scope = ""
	}
	a.syncProjectRow()
	if a.skillPath != "" && s.Find(a.skillPath) == nil {
		a.closeSkill()
	}
	a.refreshFiles()
	a.watch()
}

func (a *app) watch() {
	if a.env.watch == nil || a.snap == nil {
		return
	}
	var dirs, roots []string
	add := func(base string, ls []*skills.Location) {
		roots = append(roots, base)
		for _, l := range ls {
			if l.Exists {
				dirs = append(dirs, l.Dir)
				if l.RealDir != "" && l.RealDir != l.Dir {
					dirs = append(dirs, l.RealDir)
				}
			} else if fi, err := os.Stat(filepath.Dir(l.Dir)); err == nil && fi.IsDir() {
				roots = append(roots, filepath.Dir(l.Dir))
			}
		}
	}
	add(a.env.home, a.snap.Global)
	if a.scope != "" {
		add(a.scope, a.snap.Locations(a.scope))
	}
	if s := a.skill(); s != nil && s.RealPath != "" {
		dirs = append(dirs, s.RealPath)
	}
	slices.Sort(dirs)
	slices.Sort(roots)
	a.env.watch(slices.Compact(dirs), slices.Compact(roots), a.reload)
}

// skill returns the skill shown, or nil.
func (a *app) skill() *skills.Skill {
	if a.snap == nil || a.skillPath == "" {
		return nil
	}
	return a.snap.Find(a.skillPath)
}

// dirty reports edits not saved.
func (a *app) dirty() bool { return a.edit != nil && a.edit.Buffer.Modified() }

// guard runs fn, after asking whether to discard edits not saved.
func (a *app) guard(fn func()) {
	if a.dirty() {
		a.pending = fn
		a.discardOpen = true
		return
	}
	fn()
}

// confirmClose reports whether the window may close now; with edits not
// saved it asks first, and calls quit once they are saved or discarded.
func (a *app) confirmClose(quit func()) bool {
	if !a.dirty() {
		return true
	}
	a.pending = quit
	a.discardOpen = true
	return false
}

func (a *app) selectScope(root string) {
	if root == a.scope && a.skillPath != "" {
		return
	}
	a.guard(func() {
		a.scope = root
		a.skillQuery = ""
		a.closeSkill()
		a.syncProjectRow()
		a.watch()
	})
}

func (a *app) syncProjectRow() {
	a.projRow = -1
	for i, p := range a.visibleProjects() {
		if p.Root == a.scope {
			a.projRow = i
		}
	}
}

func (a *app) visibleProjects() []*skills.Project {
	if a.snap == nil {
		return nil
	}
	q := strings.ToLower(strings.TrimSpace(a.projQuery))
	if q == "" {
		return a.snap.Projects
	}
	var out []*skills.Project
	for _, p := range a.snap.Projects {
		if strings.Contains(strings.ToLower(p.Name), q) || strings.Contains(strings.ToLower(p.Root), q) {
			out = append(out, p)
		}
	}
	return out
}

func (a *app) selectSkill(path string) {
	if path == a.skillPath {
		return
	}
	a.guard(func() {
		a.closeSkill()
		a.skillPath = path
		a.refreshFiles()
		a.openFile(a.defaultFile())
		a.watch()
	})
}

func (a *app) closeSkill() {
	a.skillPath = ""
	a.files = nil
	a.closeFile()
}

func (a *app) closeFile() {
	a.filePath, a.edit, a.editDisk, a.editNote, a.changed = "", nil, "", "", false
}

func (a *app) defaultFile() string {
	if s := a.skill(); s != nil && s.HasSkillMD {
		return s.SkillMD()
	}
	for _, f := range a.files {
		if !f.IsDir {
			return f.Path
		}
	}
	return ""
}

// refreshFiles lists the files of the skill shown again, and takes in what
// changed on disk in the file open.
func (a *app) refreshFiles() {
	s := a.skill()
	if s == nil {
		a.files = nil
		return
	}
	if s.RealPath == "" {
		a.files = nil
		a.closeFile()
		return
	}
	files, _ := skills.ListFiles(s.Path)
	a.files = files
	a.md.LoadImage = markdown.FileImages(s.RealPath)
	if a.filePath == "" {
		return
	}
	data, err := os.ReadFile(a.filePath)
	if err != nil {
		if a.dirty() {
			a.changed = true // keep the edits; saving writes the file again
		} else {
			a.closeFile()
		}
		return
	}
	if a.edit == nil || string(data) == a.editDisk {
		return
	}
	if a.dirty() {
		a.changed = true
		return
	}
	a.edit.Buffer.Reset(string(data))
	a.edit.Buffer.MarkSaved()
	a.editDisk = string(data)
}

// openFile shows a file of the skill in the editor.
func (a *app) openFile(path string) {
	a.closeFile()
	if path == "" {
		return
	}
	a.filePath = path
	fi, err := os.Stat(path)
	if err != nil {
		a.editNote = err.Error()
		return
	}
	if fi.IsDir() {
		a.editNote = "A folder."
		return
	}
	if fi.Size() > maxEditSize {
		a.editNote = fmt.Sprintf("Too large to edit here (%s).", size(fi.Size()))
		return
	}
	data, err := os.ReadFile(path)
	if err != nil {
		a.editNote = err.Error()
		return
	}
	if !skills.IsText(data) {
		a.editNote = fmt.Sprintf("A binary file (%s).", size(fi.Size()))
		return
	}
	buf := editor.NewBuffer(string(data))
	buf.SetLanguage(editor.LanguageFor(filepath.Base(path)))
	a.edit = &editor.State{Buffer: buf, SoftWrap: true}
	a.editDisk = string(data)
}

// save writes the editor's text to its file.
func (a *app) save() {
	if a.edit == nil || a.filePath == "" {
		return
	}
	text := a.edit.Buffer.String()
	if err := skills.WriteFile(a.filePath, []byte(text)); err != nil {
		a.fail("Couldn't save "+filepath.Base(a.filePath), err)
		return
	}
	a.edit.Buffer.MarkSaved()
	a.editDisk = text
	a.changed = false
	a.notice = "Saved " + filepath.Base(a.filePath)
	a.reload()
}

func (a *app) revert() {
	if a.filePath == "" {
		return
	}
	a.openFile(a.filePath)
}

// followLink handles a link of the Markdown preview: web links open in the
// browser, links to the skill's files open them in the editor.
func (a *app) followLink(url string) {
	switch {
	case strings.HasPrefix(url, "http://"), strings.HasPrefix(url, "https://"), strings.HasPrefix(url, "mailto:"):
		if a.env.openURL != nil {
			a.env.openURL(url)
		}
		return
	case strings.HasPrefix(url, "#"):
		a.md.Anchor = url
		a.md.AnchorVersion++
		return
	}
	s := a.skill()
	if s == nil || s.RealPath == "" {
		return
	}
	rel, _, _ := strings.Cut(url, "#")
	rel = strings.TrimPrefix(rel, "./")
	p := filepath.Join(s.Path, filepath.FromSlash(rel))
	if !strings.HasPrefix(p, s.Path+string(filepath.Separator)) {
		return
	}
	if _, err := os.Stat(p); err != nil {
		a.notice = "No file " + rel + " in the skill"
		return
	}
	a.guard(func() {
		a.openFile(p)
		a.tab = tabFiles
	})
}

func (a *app) fail(title string, err error) {
	a.errTitle, a.errText, a.errOpen = title, err.Error(), true
}

// tilde writes a path under the home directory with ~, the home
// directory as written or with its links resolved.
func (a *app) tilde(p string) string {
	if a.realHome == "" {
		a.realHome = a.env.home
		if r, err := filepath.EvalSymlinks(a.env.home); err == nil {
			a.realHome = r
		}
	}
	for _, h := range []string{a.env.home, a.realHome} {
		if h != "" && (p == h || strings.HasPrefix(p, h+string(filepath.Separator))) {
			return "~" + p[len(h):]
		}
	}
	return p
}

func size(n int64) string {
	switch {
	case n < 1024:
		return fmt.Sprintf("%d B", n)
	case n < 1<<20:
		return fmt.Sprintf("%.1f KB", float64(n)/1024)
	}
	return fmt.Sprintf("%.1f MB", float64(n)/(1<<20))
}

// Actions on skills. Each changes the disk, then reads it again.

func (a *app) createSkill() {
	l := a.newSkillLoc
	if l == nil {
		return
	}
	p, err := skills.CreateSkill(l.Dir, strings.TrimSpace(a.newName), strings.TrimSpace(a.newDesc))
	if err != nil {
		a.fail("Couldn't create the skill", err)
		return
	}
	a.newSkillOpen = false
	a.reload()
	a.skillPath = ""
	a.selectSkill(p)
	a.tab = tabFiles
	a.notice = "Created " + filepath.Base(p)
}

func (a *app) renameSkill() {
	s := a.skill()
	if s == nil {
		return
	}
	name := strings.TrimSpace(a.renameName)
	oldReal := s.RealPath
	var dependents []*skills.Skill
	if !s.IsLink && a.renameRelink {
		for _, l := range a.snap.LinksTo(oldReal, s.Path) {
			if t := l.Target(); t != "" {
				if !filepath.IsAbs(t) {
					t = filepath.Join(filepath.Dir(l.Path), t)
				}
				if t = filepath.Clean(t); t == s.Path || t == oldReal {
					dependents = append(dependents, l)
				}
			}
		}
	}
	np, err := skills.Rename(s.Path, name)
	if err != nil {
		a.fail("Couldn't rename the skill", err)
		return
	}
	var errs []error
	if !s.IsLink && a.renameMeta && s.HasSkillMD {
		md := filepath.Join(np, "SKILL.md")
		if data, err := os.ReadFile(md); err == nil {
			if doc, ok := skills.SetName(string(data), name); ok {
				errs = append(errs, skills.WriteFile(md, []byte(doc)))
			}
		}
	}
	for _, l := range dependents {
		target := np
		if !filepath.IsAbs(l.Target()) { // keep relative links relative
			if rel, err := filepath.Rel(filepath.Dir(l.Path), np); err == nil {
				target = rel
			}
		}
		errs = append(errs, skills.Retarget(l.Path, target))
	}
	a.renameOpen = false
	a.closeFile()
	a.skillPath = np
	a.reload()
	a.openFile(a.defaultFile())
	if err := errors.Join(errs...); err != nil {
		a.fail("Renamed, but not everything was updated", err)
		return
	}
	a.notice = "Renamed to " + name
}

// linkTargets returns the locations a skill can be linked into from the
// link dialog's scope.
func (a *app) linkLocations() []*skills.Location {
	if a.snap == nil {
		return nil
	}
	return a.snap.Locations(a.linkScope)
}

func (a *app) createLinks() {
	s := a.skill()
	if s == nil || s.RealPath == "" {
		return
	}
	name := strings.TrimSpace(a.linkName)
	var made []string
	var errs []error
	for i, l := range a.linkLocations() {
		if !a.linkAgents[i] {
			continue
		}
		p, err := skills.CreateLink(l.Dir, name, s.RealPath)
		if err != nil {
			errs = append(errs, fmt.Errorf("%s: %w", l.Label(), err))
			continue
		}
		made = append(made, a.tilde(p))
	}
	if err := errors.Join(errs...); err != nil {
		a.fail("Couldn't create every link", err)
	} else if len(made) == 0 {
		a.fail("No link created", errors.New("choose where to link the skill"))
		return
	}
	a.linkOpen = false
	a.reload()
	if len(made) > 0 {
		a.notice = "Linked into " + strings.Join(made, ", ")
	}
}

func (a *app) retarget() {
	s := a.skill()
	if s == nil {
		return
	}
	target := strings.TrimSpace(a.retargetPath)
	if strings.HasPrefix(target, "~/") {
		target = filepath.Join(a.env.home, target[2:])
	}
	if _, err := skills.CheckTarget(target); err != nil {
		a.fail("Couldn't change the link", err)
		return
	}
	if err := skills.Retarget(s.Path, filepath.Clean(target)); err != nil {
		a.fail("Couldn't change the link", err)
		return
	}
	a.retargetOpen = false
	a.closeFile()
	a.reload()
	a.openFile(a.defaultFile())
	a.notice = "The link now points to " + a.tilde(target)
}

func (a *app) materialize() {
	s := a.skill()
	if s == nil {
		return
	}
	if err := skills.Materialize(s.Path); err != nil {
		a.fail("Couldn't make a copy", err)
		return
	}
	a.closeFile()
	a.reload()
	a.openFile(a.defaultFile())
	a.notice = s.Name + " is now a folder of its own"
}

// convertDestinations are the folders a skill can be moved to, to share.
func (a *app) convertDestinations() []string {
	var out []string
	for _, ag := range skills.Agents {
		out = append(out, filepath.Join(a.env.home, skills.RelDir(ag, skills.GlobalScope)))
	}
	return out
}

func (a *app) convert() {
	s := a.skill()
	if s == nil {
		return
	}
	dest, err := skills.ConvertToLink(s.Path, a.convertDest)
	if err != nil {
		a.fail("Couldn't move the skill", err)
		return
	}
	a.convertOpen = false
	a.closeFile()
	a.reload()
	a.openFile(a.defaultFile())
	a.notice = "Moved to " + a.tilde(dest) + " and linked here"
}

func (a *app) deleteSkill() {
	s := a.skill()
	if s == nil {
		return
	}
	if err := skills.Remove(s.Path, a.env.trash); err != nil {
		a.fail("Couldn't remove "+s.Name, err)
		return
	}
	a.closeSkill()
	a.reload()
	if s.IsLink {
		a.notice = "Removed the link " + s.Name
	} else {
		a.notice = "Moved " + s.Name + " to the Trash"
	}
}

func (a *app) createFile() {
	s := a.skill()
	if s == nil {
		return
	}
	p, err := skills.NewFile(s.Path, a.newFileRel)
	if err != nil {
		a.fail("Couldn't create the file", err)
		return
	}
	a.newFileOpen = false
	a.refreshFiles()
	a.openFile(p)
}

func (a *app) removeFile() {
	f := a.deleteFile
	var err error
	if f.IsLink {
		err = os.Remove(f.Path)
	} else if a.env.trash != nil {
		err = a.env.trash(f.Path)
	} else {
		err = errors.New("no trash")
	}
	if err != nil {
		a.fail("Couldn't remove "+f.Rel, err)
		return
	}
	if a.filePath == f.Path || strings.HasPrefix(a.filePath, f.Path+string(filepath.Separator)) {
		a.closeFile()
	}
	a.reload()
	if a.filePath == "" {
		a.openFile(a.defaultFile())
	}
	a.notice = "Removed " + f.Rel
}
