package skills

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// ProjectInfo is a directory holding skills of at least one agent.
type ProjectInfo struct {
	Root string
	Name string
	// ScanRoot is the folder whose scan found it.
	ScanRoot string
}

// skipDirs are directories never searched for projects: dependencies,
// builds and caches, which are large and hold no projects of the user's.
var skipDirs = map[string]bool{
	"node_modules": true, "vendor": true, "target": true, "dist": true,
	"build": true, "out": true, "__pycache__": true, "venv": true,
	"Library": true, "Applications": true, "Pictures": true, "Music": true,
	"Movies": true, "go": true, "pkg": true,
}

// FindProjects walks the scan roots, at most depth levels below each, for
// directories with a project skills directory. It does not follow links
// to directories, nor go into hidden ones, nor into home itself as a
// project (its skills are the global ones). Projects found under several
// roots are listed once.
func FindProjects(roots []string, depth int, home string) []ProjectInfo {
	seen := map[string]bool{}
	var out []ProjectInfo
	for _, root := range roots {
		root = filepath.Clean(root)
		var walk func(dir string, level int)
		walk = func(dir string, level int) {
			if dir != home && HasAnyLocation(dir) {
				key := dir
				if real, err := filepath.EvalSymlinks(dir); err == nil {
					key = real
				}
				if !seen[key] {
					seen[key] = true
					out = append(out, ProjectInfo{Root: dir, Name: filepath.Base(dir), ScanRoot: root})
				}
			}
			if level >= depth {
				return
			}
			entries, err := os.ReadDir(dir)
			if err != nil {
				return
			}
			for _, e := range entries {
				name := e.Name()
				if !e.IsDir() || strings.HasPrefix(name, ".") || skipDirs[name] {
					continue
				}
				walk(filepath.Join(dir, name), level+1)
			}
		}
		if fi, err := os.Stat(root); err == nil && fi.IsDir() {
			walk(root, 0)
		}
	}
	sort.SliceStable(out, func(i, j int) bool {
		a, b := strings.ToLower(out[i].Name), strings.ToLower(out[j].Name)
		if a != b {
			return a < b
		}
		return out[i].Root < out[j].Root
	})
	return out
}

// Project is a project with its skills read.
type Project struct {
	ProjectInfo
	Locations []*Location
}

// Count returns the number of skills in the project's locations.
func (p *Project) Count() int { return countSkills(p.Locations) }

func countSkills(ls []*Location) int {
	n := 0
	for _, l := range ls {
		n += len(l.Skills)
	}
	return n
}

// Snapshot is everything the app knows: the global locations and the
// projects found, with their skills.
type Snapshot struct {
	Home     string
	Global   []*Location
	Projects []*Project
}

// Load reads the global locations and every project's.
func Load(home string, projects []ProjectInfo) *Snapshot {
	s := &Snapshot{Home: home, Global: ReadScope(GlobalScope, home)}
	for _, p := range projects {
		s.Projects = append(s.Projects, &Project{ProjectInfo: p, Locations: ReadScope(ProjectScope, p.Root)})
	}
	return s
}

// GlobalCount returns the number of global skills.
func (s *Snapshot) GlobalCount() int { return countSkills(s.Global) }

// Project returns the project of a root, or nil.
func (s *Snapshot) Project(root string) *Project {
	for _, p := range s.Projects {
		if p.Root == root {
			return p
		}
	}
	return nil
}

// Locations returns the locations of a scope: global for "", else the
// project's.
func (s *Snapshot) Locations(root string) []*Location {
	if root == "" {
		return s.Global
	}
	if p := s.Project(root); p != nil {
		return p.Locations
	}
	return nil
}

// All calls fn for every skill known.
func (s *Snapshot) All(fn func(*Skill)) {
	each := func(ls []*Location) {
		for _, l := range ls {
			for _, sk := range l.Skills {
				fn(sk)
			}
		}
	}
	each(s.Global)
	for _, p := range s.Projects {
		each(p.Locations)
	}
}

// Find returns the skill whose entry is at path, or nil.
func (s *Snapshot) Find(path string) *Skill {
	var found *Skill
	s.All(func(sk *Skill) {
		if found == nil && sk.Path == path {
			found = sk
		}
	})
	return found
}

// LinksTo returns the links among the skills known whose real directory
// is real, other than the entry at except.
func (s *Snapshot) LinksTo(real, except string) []*Skill {
	var out []*Skill
	if real == "" {
		return nil
	}
	s.All(func(sk *Skill) {
		if sk.IsLink && sk.Path != except && sk.RealPath == real {
			out = append(out, sk)
		}
	})
	return out
}

// LinksInto returns the links known whose real directory is real or inside
// it, as those that break when it goes, other than the entry at except.
func (s *Snapshot) LinksInto(real, except string) []*Skill {
	var out []*Skill
	if real == "" {
		return nil
	}
	s.All(func(sk *Skill) {
		if sk.Path == except {
			return
		}
		if sk.IsLink && (sk.RealPath == real || within(real, sk.RealPath)) {
			out = append(out, sk)
		}
	})
	return out
}

// Same returns the other skills of the name in the scope of root ("" for
// global) and the global scope, which an agent working there sees beside
// this one.
func (s *Snapshot) Same(sk *Skill, root string) []*Skill {
	var out []*Skill
	look := func(ls []*Location) {
		for _, l := range ls {
			for _, o := range l.Skills {
				if o != sk && o.Name == sk.Name {
					out = append(out, o)
				}
			}
		}
	}
	look(s.Locations(root))
	if root != "" {
		look(s.Global)
	}
	return out
}

func within(dir, p string) bool {
	if p == "" {
		return false
	}
	rel, err := filepath.Rel(dir, p)
	return err == nil && rel != "." && !strings.HasPrefix(rel, "..")
}
