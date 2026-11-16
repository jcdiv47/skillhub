// Package skills finds, reads and changes agent skills on disk: the skill
// directories of .agents/skills, .claude/skills and pi's skills, in the
// home directory (global scope) and in projects (project scope).
package skills

import (
	"os"
	"path/filepath"
	"sort"
)

// Agent is the tool whose skills directory a location is.
type Agent int

const (
	// AgentShared is .agents/skills, shared by agents that read it.
	AgentShared Agent = iota
	// AgentClaude is Claude Code's .claude/skills.
	AgentClaude
	// AgentPi is pi's ~/.pi/agent/skills (global) and .pi/skills (project).
	AgentPi
)

// Agents lists every agent, in the order the app shows them.
var Agents = []Agent{AgentShared, AgentClaude, AgentPi}

func (a Agent) String() string {
	switch a {
	case AgentShared:
		return "Agents"
	case AgentClaude:
		return "Claude"
	case AgentPi:
		return "Pi"
	}
	return "?"
}

// Scope is where a location's skills apply.
type Scope int

const (
	GlobalScope Scope = iota
	ProjectScope
)

func (s Scope) String() string {
	if s == GlobalScope {
		return "Global"
	}
	return "Project"
}

// RelDir returns the skills directory of an agent relative to the home
// directory (Global) or a project's root (Project).
func RelDir(a Agent, s Scope) string {
	switch a {
	case AgentShared:
		return filepath.Join(".agents", "skills")
	case AgentClaude:
		return filepath.Join(".claude", "skills")
	case AgentPi:
		if s == GlobalScope {
			return filepath.Join(".pi", "agent", "skills")
		}
		return filepath.Join(".pi", "skills")
	}
	return ""
}

// Hop is a symbolic link met on the way to a path: the link and what it
// holds, as readlink returns it.
type Hop struct {
	Link   string
	Target string
}

// Location is a skills directory: one agent's, in one scope.
type Location struct {
	Agent Agent
	Scope Scope
	// Base is the home directory (Global) or the project's root.
	Base string
	// Dir is Base joined with the agent's directory, as written.
	Dir string
	// Exists reports whether Dir is a directory (through links).
	Exists bool
	// Links are the links between Base and Dir, as .claude being a link,
	// or skills itself.
	Links []Hop
	// RealDir is Dir with every link resolved, when it exists.
	RealDir string
	// Err is why the directory could not be read, if it could not.
	Err    string
	Skills []*Skill
}

// Label names the location as the app lists it, as "Claude · .claude/skills".
func (l *Location) Label() string {
	return l.Agent.String() + " · " + RelDir(l.Agent, l.Scope)
}

// Severity grades a problem.
type Severity int

const (
	Warning Severity = iota
	Error
)

// Problem is something wrong with a skill.
type Problem struct {
	Severity Severity
	Message  string
}

// Skill is an entry of a skills directory: a directory, or a link to one.
type Skill struct {
	// Name is the entry's name, the directory's name.
	Name string
	// Path is the entry in its location's directory.
	Path     string
	Location *Location
	// IsLink reports whether the entry itself is a symbolic link.
	IsLink bool
	// Chain is every link from Path to the real directory, the first
	// being Path itself when IsLink: more than one hop is a link to a link.
	Chain []Hop
	// Broken reports a link whose target does not exist, or loops.
	Broken bool
	// RealPath is the directory with every link resolved, "" when broken.
	RealPath string
	// HasSkillMD reports whether SKILL.md exists in the directory.
	HasSkillMD bool
	Meta       Frontmatter
	Problems   []Problem
}

// Target returns what the entry links to, as written, or "".
func (s *Skill) Target() string {
	if !s.IsLink || len(s.Chain) == 0 {
		return ""
	}
	return s.Chain[0].Target
}

// SkillMD returns the path of the skill's SKILL.md, through the entry.
func (s *Skill) SkillMD() string { return filepath.Join(s.Path, "SKILL.md") }

// Worst returns the worst severity among its problems, and false for none.
func (s *Skill) Worst() (Severity, bool) {
	if len(s.Problems) == 0 {
		return 0, false
	}
	w := Warning
	for _, p := range s.Problems {
		if p.Severity > w {
			w = p.Severity
		}
	}
	return w, true
}

// ReadLocation reads the skills directory of an agent under base.
func ReadLocation(a Agent, scope Scope, base string) *Location {
	l := &Location{Agent: a, Scope: scope, Base: base, Dir: filepath.Join(base, RelDir(a, scope))}
	l.Links = linksBetween(base, RelDir(a, scope))
	fi, err := os.Stat(l.Dir)
	if err != nil {
		if !os.IsNotExist(err) {
			l.Err = err.Error()
		}
		return l
	}
	if !fi.IsDir() {
		l.Err = "not a directory"
		return l
	}
	l.Exists = true
	if real, err := filepath.EvalSymlinks(l.Dir); err == nil {
		l.RealDir = real
	}
	entries, err := os.ReadDir(l.Dir)
	if err != nil {
		l.Err = err.Error()
		return l
	}
	for _, e := range entries {
		name := e.Name()
		if len(name) > 0 && name[0] == '.' {
			continue // .DS_Store, .git and other hidden entries
		}
		s := readSkill(l, name)
		if s != nil {
			l.Skills = append(l.Skills, s)
		}
	}
	sort.Slice(l.Skills, func(i, j int) bool { return l.Skills[i].Name < l.Skills[j].Name })
	return l
}

// ReadScope reads the three locations of a scope under base.
func ReadScope(scope Scope, base string) []*Location {
	ls := make([]*Location, 0, len(Agents))
	for _, a := range Agents {
		ls = append(ls, ReadLocation(a, scope, base))
	}
	return ls
}

// HasAnyLocation reports whether dir holds a project skills directory of
// any agent.
func HasAnyLocation(dir string) bool {
	for _, a := range Agents {
		if fi, err := os.Stat(filepath.Join(dir, RelDir(a, ProjectScope))); err == nil && fi.IsDir() {
			return true
		}
	}
	return false
}

// readSkill reads one entry, nil when it is not a skill (a plain file).
func readSkill(l *Location, name string) *Skill {
	p := filepath.Join(l.Dir, name)
	fi, err := os.Lstat(p)
	if err != nil {
		return nil
	}
	s := &Skill{Name: name, Path: p, Location: l}
	if fi.Mode()&os.ModeSymlink != 0 {
		s.IsLink = true
		chain, final, ok := FollowChain(p)
		s.Chain = chain
		if !ok {
			s.Broken = true
			if len(chain) >= maxHops {
				s.problem(Error, "The link loops, or goes through too many links.")
			} else {
				s.problem(Error, "Broken link: "+final+" does not exist.")
			}
			return s
		}
		if len(chain) > 1 {
			s.problem(Warning, "Links to another link; it goes through "+itoa(len(chain))+" links.")
		}
	}
	real, err := filepath.EvalSymlinks(p)
	if err != nil {
		s.Broken = s.IsLink
		s.problem(Error, err.Error())
		return s
	}
	rfi, err := os.Stat(real)
	if err != nil {
		s.problem(Error, err.Error())
		return s
	}
	if !rfi.IsDir() {
		return nil // a file in the skills directory, not a skill
	}
	s.RealPath = real
	data, err := os.ReadFile(filepath.Join(real, "SKILL.md"))
	if err != nil {
		if os.IsNotExist(err) {
			s.problem(Error, "No SKILL.md in the folder.")
		} else {
			s.problem(Error, "Cannot read SKILL.md: "+err.Error())
		}
		return s
	}
	s.HasSkillMD = true
	s.Meta = ParseFrontmatter(string(data))
	for _, p := range Validate(name, s.Meta) {
		s.Problems = append(s.Problems, p)
	}
	return s
}

func (s *Skill) problem(sev Severity, msg string) {
	s.Problems = append(s.Problems, Problem{Severity: sev, Message: msg})
}

const maxHops = 40

// FollowChain follows the links from p, one hop at a time, returning each
// link met and the last path reached; ok is false when that path does not
// exist or the links loop. A link in a parent directory of a hop is
// followed by the system, not listed.
func FollowChain(p string) (chain []Hop, final string, ok bool) {
	cur := p
	for i := 0; i < maxHops; i++ {
		fi, err := os.Lstat(cur)
		if err != nil {
			return chain, cur, false
		}
		if fi.Mode()&os.ModeSymlink == 0 {
			return chain, cur, true
		}
		t, err := os.Readlink(cur)
		if err != nil {
			return chain, cur, false
		}
		chain = append(chain, Hop{Link: cur, Target: t})
		if !filepath.IsAbs(t) {
			t = filepath.Join(filepath.Dir(cur), t)
		}
		cur = filepath.Clean(t)
	}
	return chain, cur, false
}

// linksBetween lists the links among the directories from base down to
// base/rel, not counting base itself.
func linksBetween(base, rel string) []Hop {
	var hops []Hop
	cur := base
	for _, part := range splitPath(rel) {
		cur = filepath.Join(cur, part)
		fi, err := os.Lstat(cur)
		if err != nil {
			break
		}
		if fi.Mode()&os.ModeSymlink != 0 {
			t, _ := os.Readlink(cur)
			hops = append(hops, Hop{Link: cur, Target: t})
		}
	}
	return hops
}

func splitPath(rel string) []string {
	var parts []string
	for rel != "" && rel != "." {
		dir, file := filepath.Split(rel)
		parts = append([]string{file}, parts...)
		rel = filepath.Clean(dir)
		if rel == string(filepath.Separator) {
			break
		}
	}
	return parts
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var b []byte
	for n > 0 {
		b = append([]byte{byte('0' + n%10)}, b...)
		n /= 10
	}
	return string(b)
}
