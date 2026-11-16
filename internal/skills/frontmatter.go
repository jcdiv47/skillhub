package skills

import (
	"fmt"
	"regexp"
	"strings"
	"unicode/utf8"

	"gopkg.in/yaml.v3"
)

// Field is a key of the frontmatter and its value, written as text.
type Field struct {
	Key   string
	Value string
	// Multiline reports a value that is a list, a map or text of several
	// lines, shown as a block.
	Multiline bool
}

// Frontmatter is the YAML between the --- lines that start SKILL.md.
type Frontmatter struct {
	// Present reports whether the file starts with a frontmatter block.
	Present bool
	// Raw is the YAML between the delimiters.
	Raw string
	// Fields are its top-level keys, in the order written.
	Fields      []Field
	Name        string
	Description string
	// Err is why the YAML could not be read, if it could not.
	Err string
	// Body is the Markdown after the frontmatter.
	Body string
	// BodyLine is the line of the file Body starts on, from 1.
	BodyLine int
}

// Get returns the value of a top-level key.
func (f Frontmatter) Get(key string) (string, bool) {
	for _, fl := range f.Fields {
		if fl.Key == key {
			return fl.Value, true
		}
	}
	return "", false
}

// SplitFrontmatter splits a document into its frontmatter's YAML and its
// body. ok is false when it has no frontmatter, or it is never closed.
func SplitFrontmatter(doc string) (yamlText, body string, bodyLine int, ok bool) {
	doc = strings.TrimPrefix(doc, "\ufeff")
	first, rest, found := strings.Cut(doc, "\n")
	if !found || strings.TrimRight(first, " \t\r") != "---" {
		return "", doc, 1, false
	}
	line := 2
	var y strings.Builder
	for rest != "" {
		l, r, _ := strings.Cut(rest, "\n")
		t := strings.TrimRight(l, " \t\r")
		if t == "---" || t == "..." {
			return y.String(), r, line + 1, true
		}
		y.WriteString(l)
		y.WriteByte('\n')
		rest = r
		line++
	}
	return "", doc, 1, false
}

// ParseFrontmatter reads the frontmatter of a SKILL.md.
func ParseFrontmatter(doc string) Frontmatter {
	y, body, line, ok := SplitFrontmatter(doc)
	f := Frontmatter{Present: ok, Raw: y, Body: body, BodyLine: line}
	if !ok {
		return f
	}
	var root yaml.Node
	if err := yaml.Unmarshal([]byte(y), &root); err != nil {
		f.Err = cleanYAMLError(err)
		return f
	}
	if len(root.Content) == 0 {
		return f
	}
	m := root.Content[0]
	if m.Kind != yaml.MappingNode {
		f.Err = "the frontmatter is not a map of keys and values"
		return f
	}
	for i := 0; i+1 < len(m.Content); i += 2 {
		k, v := m.Content[i], m.Content[i+1]
		fl := Field{Key: k.Value}
		if v.Kind == yaml.ScalarNode {
			fl.Value = v.Value
			fl.Multiline = strings.Contains(strings.TrimRight(v.Value, "\n"), "\n")
		} else {
			out, err := yaml.Marshal(v)
			if err == nil {
				fl.Value = strings.TrimRight(string(out), "\n")
			}
			fl.Multiline = true
		}
		f.Fields = append(f.Fields, fl)
		switch k.Value {
		case "name":
			f.Name = strings.TrimSpace(fl.Value)
		case "description":
			f.Description = strings.TrimSpace(fl.Value)
		}
	}
	return f
}

func cleanYAMLError(err error) string {
	return strings.TrimPrefix(err.Error(), "yaml: ")
}

var nameRule = regexp.MustCompile(`^[a-z0-9]+(-[a-z0-9]+)*$`)

// Validate checks a skill's frontmatter against the Agent Skills
// specification: a name of lowercase letters, digits and single hyphens,
// at most 64 characters, the same as its directory's, and a description
// of at most 1024 characters.
func Validate(dirName string, f Frontmatter) []Problem {
	var ps []Problem
	add := func(sev Severity, format string, a ...any) {
		ps = append(ps, Problem{Severity: sev, Message: fmt.Sprintf(format, a...)})
	}
	if !f.Present {
		add(Error, "SKILL.md has no frontmatter (--- name/description --- at the top).")
		return ps
	}
	if f.Err != "" {
		add(Error, "The frontmatter is not valid YAML: %s", f.Err)
		return ps
	}
	switch {
	case f.Name == "":
		add(Error, "The frontmatter has no name.")
	case f.Name != dirName:
		add(Warning, "The name %q differs from the folder's name %q.", f.Name, dirName)
	}
	if f.Name != "" {
		if utf8.RuneCountInString(f.Name) > 64 {
			add(Warning, "The name is longer than 64 characters.")
		}
		if !nameRule.MatchString(f.Name) {
			add(Warning, "The name should be lowercase letters, digits and single hyphens.")
		}
	}
	switch {
	case f.Description == "":
		add(Error, "The frontmatter has no description.")
	case utf8.RuneCountInString(f.Description) > 1024:
		add(Warning, "The description is longer than 1024 characters.")
	}
	return ps
}

// SetName returns doc with the frontmatter's top-level name set to name,
// and false when it has no name line to change.
func SetName(doc, name string) (string, bool) {
	y, _, _, ok := SplitFrontmatter(doc)
	if !ok {
		return doc, false
	}
	start := strings.Index(doc, y)
	if start < 0 {
		return doc, false
	}
	lines := strings.SplitAfter(y, "\n")
	off := start
	for _, l := range lines {
		trimmed := strings.TrimRight(l, "\r\n")
		if strings.HasPrefix(trimmed, "name:") {
			end := off + len(trimmed)
			return doc[:off] + "name: " + name + doc[end:], true
		}
		off += len(l)
	}
	return doc, false
}

// Template returns a new SKILL.md for a skill.
func Template(name, description string) string {
	if description == "" {
		description = "Describe what this skill does and when to use it."
	}
	desc, _ := yaml.Marshal(description)
	return "---\nname: " + name + "\ndescription: " + strings.TrimRight(string(desc), "\n") + "\n---\n\n# " + title(name) + "\n\n## Instructions\n\n"
}

func title(name string) string {
	words := strings.FieldsFunc(name, func(r rune) bool { return r == '-' || r == '_' })
	for i, w := range words {
		words[i] = strings.ToUpper(w[:1]) + w[1:]
	}
	if len(words) == 0 {
		return name
	}
	return strings.Join(words, " ")
}
