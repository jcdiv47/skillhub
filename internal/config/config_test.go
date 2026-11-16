package config

import (
	"os"
	"path/filepath"
	"slices"
	"testing"
)

func TestLoadSave(t *testing.T) {
	path := filepath.Join(t.TempDir(), "nested", "config.json")
	c, err := Load(path)
	if err != nil || len(c.Roots) != 0 || c.Depth != DefaultDepth {
		t.Fatalf("missing file: %+v %v", c, err)
	}
	c.AddRoots("/a", "/b/", "/a")
	if !slices.Equal(c.Roots, []string{"/a", "/b"}) {
		t.Fatalf("roots %v", c.Roots)
	}
	c.Depth = 2
	c.SidebarWidth = 300
	if err := c.Save(path); err != nil {
		t.Fatal(err)
	}
	got, err := Load(path)
	if err != nil || !slices.Equal(got.Roots, c.Roots) || got.Depth != 2 || got.SidebarWidth != 300 {
		t.Fatalf("loaded %+v %v", got, err)
	}
	got.RemoveRoot("/a")
	if !slices.Equal(got.Roots, []string{"/b"}) {
		t.Fatalf("after remove %v", got.Roots)
	}

	os.WriteFile(path, []byte("{nope"), 0o644)
	if c, err := Load(path); err == nil || c.Depth != DefaultDepth {
		t.Fatalf("corrupt file: %+v %v", c, err)
	}
	os.WriteFile(path, []byte(`{"roots":["/x"],"depth":0}`), 0o644)
	if c, _ := Load(path); c.Depth != DefaultDepth {
		t.Fatalf("zero depth: %+v", c)
	}
}
