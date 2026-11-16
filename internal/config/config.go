// Package config keeps SkillHub's settings: the folders it scans for
// projects, and how deep.
package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"slices"
)

// DefaultDepth is how many levels below a scan folder projects are looked
// for, unless the user chooses otherwise.
const DefaultDepth = 4

// Config is what SkillHub remembers between runs.
type Config struct {
	// Roots are the folders scanned for projects.
	Roots []string `json:"roots"`
	// Depth is how many levels below each root projects are looked for.
	Depth int `json:"depth"`
	// SidebarWidth and ListWidth are the panes' widths.
	SidebarWidth float32 `json:"sidebarWidth,omitempty"`
	ListWidth    float32 `json:"listWidth,omitempty"`
}

// Load reads the configuration at path; a missing file is an empty one.
func Load(path string) (Config, error) {
	var c Config
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		c.Depth = DefaultDepth
		return c, nil
	}
	if err != nil {
		return c, err
	}
	if err := json.Unmarshal(data, &c); err != nil {
		return Config{Depth: DefaultDepth}, err
	}
	if c.Depth <= 0 {
		c.Depth = DefaultDepth
	}
	return c, nil
}

// Save writes the configuration to path, creating its folder.
func (c Config) Save(path string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, append(data, '\n'), 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// AddRoots adds folders to scan, leaving out those already there.
func (c *Config) AddRoots(paths ...string) {
	for _, p := range paths {
		p = filepath.Clean(p)
		if p != "" && !slices.Contains(c.Roots, p) {
			c.Roots = append(c.Roots, p)
		}
	}
}

// RemoveRoot stops scanning a folder.
func (c *Config) RemoveRoot(p string) {
	c.Roots = slices.DeleteFunc(c.Roots, func(r string) bool { return r == p })
}
