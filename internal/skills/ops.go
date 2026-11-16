package skills

import (
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Trasher moves a file or directory to the system's trash.
type Trasher func(path string) error

// CheckName reports whether name can be a skill's folder or a file's name:
// not empty, no separators, not hidden.
func CheckName(name string) error {
	switch {
	case strings.TrimSpace(name) == "":
		return errors.New("enter a name")
	case name != strings.TrimSpace(name):
		return errors.New("the name starts or ends with spaces")
	case strings.ContainsAny(name, `/\`):
		return errors.New("the name cannot contain / or \\")
	case name == "." || name == "..":
		return errors.New("that name is reserved")
	case strings.HasPrefix(name, "."):
		return errors.New("hidden names are not listed as skills")
	}
	return nil
}

func exists(p string) bool {
	_, err := os.Lstat(p)
	return err == nil
}

// CreateSkill makes a skill folder named name in dir, creating dir when it
// does not exist, with a SKILL.md for it. It returns the folder's path.
func CreateSkill(dir, name, description string) (string, error) {
	if err := CheckName(name); err != nil {
		return "", err
	}
	p := filepath.Join(dir, name)
	if exists(p) {
		return "", fmt.Errorf("%s already exists", p)
	}
	if err := os.MkdirAll(p, 0o755); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(p, "SKILL.md"), []byte(Template(name, description)), 0o644); err != nil {
		return "", err
	}
	return p, nil
}

// WriteFile writes data to path as one change: to a new file beside it,
// renamed over it. A path that is a link, or under one, is written at its
// real place, so the link stays a link. The file keeps its mode.
func WriteFile(path string, data []byte) error {
	real := path
	if r, err := filepath.EvalSymlinks(path); err == nil {
		real = r
	} else if !os.IsNotExist(err) {
		return err
	} else if d, err := filepath.EvalSymlinks(filepath.Dir(path)); err == nil {
		real = filepath.Join(d, filepath.Base(path))
	}
	mode := fs.FileMode(0o644)
	if fi, err := os.Stat(real); err == nil {
		mode = fi.Mode().Perm()
	}
	tmp, err := os.CreateTemp(filepath.Dir(real), "."+filepath.Base(real)+".skillhub-*")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Chmod(mode); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), real)
}

// CreateLink makes a link named name in dir to target, an absolute path,
// creating dir when it does not exist. It returns the link's path.
func CreateLink(dir, name, target string) (string, error) {
	if err := CheckName(name); err != nil {
		return "", err
	}
	if !filepath.IsAbs(target) {
		return "", errors.New("the target must be an absolute path")
	}
	p := filepath.Join(dir, name)
	if exists(p) {
		return "", fmt.Errorf("%s already exists", p)
	}
	if same(dir, target) || within(target, dir) {
		return "", errors.New("a skill cannot be linked into itself")
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return p, os.Symlink(target, p)
}

func same(a, b string) bool {
	ra, err1 := filepath.EvalSymlinks(a)
	rb, err2 := filepath.EvalSymlinks(b)
	return err1 == nil && err2 == nil && ra == rb
}

// CheckTarget reports what is wrong with a directory as a skill's target:
// nil for a directory with SKILL.md, a warning error for a directory
// without, an error otherwise.
func CheckTarget(target string) (warning string, err error) {
	if !filepath.IsAbs(target) {
		return "", errors.New("enter an absolute path")
	}
	fi, err := os.Stat(target)
	if err != nil {
		if os.IsNotExist(err) {
			return "", errors.New("the folder does not exist")
		}
		return "", err
	}
	if !fi.IsDir() {
		return "", errors.New("not a folder")
	}
	if !exists(filepath.Join(target, "SKILL.md")) {
		return "The folder has no SKILL.md.", nil
	}
	return "", nil
}

// Retarget points the link at path to target, replacing it as one change.
// A relative target is relative to the link's folder, as in a link.
func Retarget(path, target string) error {
	fi, err := os.Lstat(path)
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink == 0 {
		return errors.New("not a link")
	}
	abs := target // a relative target is written as given, from the link's folder
	if !filepath.IsAbs(abs) {
		abs = filepath.Join(filepath.Dir(path), abs)
	}
	if within(abs, filepath.Dir(path)) || same(filepath.Dir(path), abs) {
		return errors.New("a link cannot point to the folder it is in")
	}
	tmp := filepath.Join(filepath.Dir(path), "."+filepath.Base(path)+".skillhub-link")
	os.Remove(tmp)
	if err := os.Symlink(target, tmp); err != nil {
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		os.Remove(tmp)
		return err
	}
	return nil
}

// Materialize replaces the link at path with a copy of the directory it
// points to. Links inside the directory are copied as links.
func Materialize(path string) error {
	fi, err := os.Lstat(path)
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink == 0 {
		return errors.New("not a link")
	}
	target, err := os.Readlink(path)
	if err != nil {
		return err
	}
	real, err := filepath.EvalSymlinks(path)
	if err != nil {
		return fmt.Errorf("the link is broken: %w", err)
	}
	tmp := filepath.Join(filepath.Dir(path), "."+filepath.Base(path)+".skillhub-copy")
	os.RemoveAll(tmp)
	if err := CopyDir(real, tmp); err != nil {
		os.RemoveAll(tmp)
		return err
	}
	if err := os.Remove(path); err != nil {
		os.RemoveAll(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		os.Symlink(target, path) // put the link back
		os.RemoveAll(tmp)
		return err
	}
	return nil
}

// ConvertToLink moves the directory at path to dir, under the same name,
// and leaves a link to it at path. It returns the directory's new path.
func ConvertToLink(path, dir string) (string, error) {
	fi, err := os.Lstat(path)
	if err != nil {
		return "", err
	}
	if fi.Mode()&os.ModeSymlink != 0 || !fi.IsDir() {
		return "", errors.New("only a real folder can be moved and linked")
	}
	if !filepath.IsAbs(dir) {
		return "", errors.New("the destination must be an absolute path")
	}
	if same(filepath.Dir(path), dir) {
		return "", errors.New("the folder is already there")
	}
	if within(path, dir) || same(path, dir) {
		return "", errors.New("cannot move a folder into itself")
	}
	dest := filepath.Join(dir, filepath.Base(path))
	if exists(dest) {
		return "", fmt.Errorf("%s already exists", dest)
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	if err := move(path, dest); err != nil {
		return "", err
	}
	if err := os.Symlink(dest, path); err != nil {
		move(dest, path) // put the folder back
		return "", err
	}
	return dest, nil
}

// move renames, or copies and removes across file systems.
func move(from, to string) error {
	err := os.Rename(from, to)
	if err == nil {
		return nil
	}
	var le *os.LinkError
	if !errors.As(err, &le) {
		return err
	}
	if cerr := CopyDir(from, to); cerr != nil {
		os.RemoveAll(to)
		return err
	}
	return os.RemoveAll(from)
}

// Rename renames the entry at path, a folder or a link, to name in the
// same directory. It returns the new path.
func Rename(path, name string) (string, error) {
	if err := CheckName(name); err != nil {
		return "", err
	}
	np := filepath.Join(filepath.Dir(path), name)
	if np == path {
		return path, nil
	}
	if fi, err := os.Lstat(np); err == nil {
		// Only a change of case on a case-insensitive disk finds the
		// entry itself.
		self, err := os.Lstat(path)
		if err != nil || !os.SameFile(fi, self) {
			return "", fmt.Errorf("%s already exists", np)
		}
	}
	return np, os.Rename(path, np)
}

// Remove removes a skill's entry: only the link when it is one, otherwise
// the folder, to the trash.
func Remove(path string, trash Trasher) error {
	fi, err := os.Lstat(path)
	if err != nil {
		return err
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return os.Remove(path)
	}
	if trash == nil {
		return errors.New("no trash to move the folder to")
	}
	return trash(path)
}

// CopyDir copies the directory from to to, which must not exist. Links are
// copied as links, files with their modes.
func CopyDir(from, to string) error {
	return filepath.WalkDir(from, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(from, p)
		if err != nil {
			return err
		}
		dst := filepath.Join(to, rel)
		info, err := os.Lstat(p)
		if err != nil {
			return err
		}
		switch {
		case info.Mode()&os.ModeSymlink != 0:
			t, err := os.Readlink(p)
			if err != nil {
				return err
			}
			return os.Symlink(t, dst)
		case info.IsDir():
			return os.Mkdir(dst, info.Mode().Perm()|0o700)
		case info.Mode().IsRegular():
			return copyFile(p, dst, info.Mode().Perm())
		}
		return nil // sockets, devices: left out
	})
}

func copyFile(from, to string, mode fs.FileMode) error {
	in, err := os.Open(from)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(to, os.O_WRONLY|os.O_CREATE|os.O_EXCL, mode)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}

// File is a file inside a skill's folder.
type File struct {
	// Rel is its path in the folder, with / between parts.
	Rel  string
	Path string
	// IsLink reports a link; Target is what it holds, Broken whether it
	// points nowhere.
	IsLink bool
	Target string
	Broken bool
	IsDir  bool
	Size   int64
	Depth  int
}

// MaxFiles bounds the files listed of one skill.
const MaxFiles = 2000

// ListFiles lists the files and folders inside a skill's folder, through
// its link, SKILL.md first and then by path. Links inside are listed, not
// followed. Hidden .git folders are left out.
func ListFiles(dir string) ([]File, error) {
	root, err := filepath.EvalSymlinks(dir)
	if err != nil {
		return nil, err
	}
	var files []File
	err = filepath.WalkDir(root, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if p == root {
			return nil
		}
		if d.IsDir() && (d.Name() == ".git" || d.Name() == "node_modules") {
			return filepath.SkipDir
		}
		if d.Name() == ".DS_Store" {
			return nil
		}
		if len(files) >= MaxFiles {
			return filepath.SkipAll
		}
		rel, _ := filepath.Rel(root, p)
		f := File{Rel: filepath.ToSlash(rel), Path: filepath.Join(dir, rel), IsDir: d.IsDir(), Depth: strings.Count(filepath.ToSlash(rel), "/")}
		if d.Type()&fs.ModeSymlink != 0 {
			f.IsLink = true
			f.Target, _ = os.Readlink(p)
			if fi, err := os.Stat(p); err != nil {
				f.Broken = true
			} else {
				f.Size = fi.Size()
				f.IsDir = fi.IsDir()
			}
		} else if info, err := d.Info(); err == nil && !d.IsDir() {
			f.Size = info.Size()
		}
		files = append(files, f)
		return nil
	})
	sort.SliceStable(files, func(i, j int) bool {
		a, b := files[i].Rel == "SKILL.md", files[j].Rel == "SKILL.md"
		if a != b {
			return a
		}
		return files[i].Rel < files[j].Rel
	})
	return files, err
}

// NewFile makes an empty file at rel inside the skill's folder dir,
// creating the folders on its way. It returns its path.
func NewFile(dir, rel string) (string, error) {
	rel = strings.TrimSpace(filepath.ToSlash(rel))
	if rel == "" {
		return "", errors.New("enter a file name")
	}
	clean := filepath.Clean(filepath.FromSlash(rel))
	if filepath.IsAbs(clean) || clean == "." || strings.HasPrefix(clean, "..") {
		return "", errors.New("the file must be inside the skill's folder")
	}
	p := filepath.Join(dir, clean)
	if exists(p) {
		return "", fmt.Errorf("%s already exists", rel)
	}
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return "", err
	}
	f, err := os.OpenFile(p, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
	if err != nil {
		return "", err
	}
	return p, f.Close()
}

// IsText reports whether data reads as text: UTF-8 without NUL bytes in
// its first 8 KB.
func IsText(data []byte) bool {
	if len(data) > 8192 {
		data = data[:8192]
	}
	for _, b := range data {
		if b == 0 {
			return false
		}
	}
	return true
}
