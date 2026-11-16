package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/egoist/mygo/ui"

	"skillhub/internal/skills"
)

func (a *app) dialogs(c *ui.Context) {
	t := c.Theme()

	ui.Modal(c, &a.settingsOpen, func() {
		dialogBody(c, func() {
			ui.Text(c, "Folders to Scan").FontSize(17).Bold()
			ui.Text(c, "SkillHub looks for projects with skills in these folders.").TextColor(t.TextMuted)
			a.rootsEditor(c)
			ui.Row(c).Gap(8).Justify(ui.End).Children(func() {
				if ui.Button(c, "Cancel").Clicked() {
					a.settingsOpen = false
				}
				if ui.PrimaryButton(c, "Save & Rescan").Clicked() {
					a.settingsOpen = false
					a.saveConfig()
					a.scan()
				}
			})
		})
	})

	ui.Modal(c, &a.newSkillOpen, func() {
		dialogBody(c, func() {
			l := a.newSkillLoc
			ui.Text(c, "New Skill").FontSize(17).Bold()
			field(c, "Name", func() {
				ui.TextInput(c, &a.newName).Placeholder("my-skill").Label("Name").AutoFocus()
			})
			field(c, "Description", func() {
				ui.TextArea(c, &a.newDesc).Placeholder("What the skill does and when to use it.").Label("Description").Height(72)
			})
			name := strings.TrimSpace(a.newName)
			err := skills.CheckName(name)
			if l != nil && name != "" {
				where := filepath.Join(l.Dir, name)
				if err == nil {
					if _, e := os.Lstat(where); e == nil {
						err = fmt.Errorf("%s already exists", a.tilde(where))
					}
				}
				ui.Text(c, "Creates "+a.tilde(where)+"/SKILL.md").FontSize(12).Font("monospace").TextColor(t.TextMuted)
			}
			if err != nil && name != "" {
				ui.Text(c, err.Error()).FontSize(12).TextColor(t.Danger)
			}
			buttons(c, "Create", err != nil, func() { a.newSkillOpen = false }, a.createSkill)
		})
	})

	ui.Modal(c, &a.renameOpen, func() {
		s := a.skill()
		if s == nil {
			a.renameOpen = false
			return
		}
		dialogBody(c, func() {
			ui.Text(c, "Rename "+s.Name).FontSize(17).Bold()
			ui.TextInput(c, &a.renameName).Label("New name").AutoFocus()
			if s.IsLink {
				ui.Text(c, "Renames the link only; the folder it points to keeps its name.").FontSize(12).TextColor(t.TextMuted)
			} else {
				if s.HasSkillMD && s.Meta.Name == s.Name {
					ui.Checkbox(c, &a.renameMeta, "Also change name: in SKILL.md")
				}
				if n := len(a.snap.LinksTo(s.RealPath, s.Path)); n > 0 {
					ui.Checkbox(c, &a.renameRelink, "Point the "+plural(n, "link", "links")+" to this folder at its new name")
				}
			}
			name := strings.TrimSpace(a.renameName)
			err := skills.CheckName(name)
			if err == nil && name == s.Name {
				err = fmt.Errorf("enter a new name")
			}
			if err != nil && name != "" && name != s.Name {
				ui.Text(c, err.Error()).FontSize(12).TextColor(t.Danger)
			}
			buttons(c, "Rename", err != nil, func() { a.renameOpen = false }, a.renameSkill)
		})
	})

	ui.Modal(c, &a.linkOpen, func() {
		s := a.skill()
		if s == nil {
			a.linkOpen = false
			return
		}
		dialogBody(c, func() {
			ui.Text(c, "Link "+s.Name+" Into…").FontSize(17).Bold()
			ui.Text(c, "Creates links to "+a.tilde(s.RealPath)).FontSize(12).Font("monospace").TextColor(t.TextMuted)
			labels, roots := a.scopeChoices()
			chosen := labels[0]
			for i, r := range roots {
				if r == a.linkScope {
					chosen = labels[i]
				}
			}
			field(c, "Scope", func() {
				if ui.Select(c, &chosen, labels).Label("Scope").Changed() {
					for i, l := range labels {
						if l == chosen {
							a.linkScope = roots[i]
						}
					}
					a.linkAgents = [3]bool{}
				}
			})
			field(c, "Name", func() {
				ui.TextInput(c, &a.linkName).Label("Link name")
			})
			name := strings.TrimSpace(a.linkName)
			any := false
			ui.Column(c).Gap(6).Children(func() {
				for i, l := range a.linkLocations() {
					p := filepath.Join(l.Dir, name)
					_, statErr := os.Lstat(p)
					taken := statErr == nil
					selfDir := l.RealDir != "" && filepath.Dir(s.RealPath) == l.RealDir && name == filepath.Base(s.RealPath)
					label := l.Label()
					switch {
					case selfDir:
						label += " — the skill is here"
					case taken:
						label += " — " + name + " exists"
					case !l.Exists:
						label += " — will be created"
					}
					if taken || selfDir {
						a.linkAgents[i] = false
					}
					ui.Checkbox(c.Key(l.Dir), &a.linkAgents[i], label).Disabled(taken || selfDir)
					any = any || a.linkAgents[i]
				}
			})
			err := skills.CheckName(name)
			if err != nil && name != "" {
				ui.Text(c, err.Error()).FontSize(12).TextColor(t.Danger)
			}
			buttons(c, "Create Links", err != nil || !any, func() { a.linkOpen = false }, a.createLinks)
		})
	})

	ui.Modal(c, &a.retargetOpen, func() {
		s := a.skill()
		if s == nil {
			a.retargetOpen = false
			return
		}
		dialogBody(c, func() {
			ui.Text(c, "Change Link Target").FontSize(17).Bold()
			ui.Text(c, a.tilde(s.Path)+" → "+s.Target()).FontSize(12).Font("monospace").TextColor(t.TextMuted)
			ui.Row(c).Gap(8).Children(func() {
				ui.TextInput(c, &a.retargetPath).Label("Target folder").Grow(1).AutoFocus()
				if ui.Button(c, "Choose…").Clicked() {
					a.env.pickFolders("Choose the skill folder to link to", false, func(p []string) {
						if len(p) > 0 {
							a.retargetPath = p[0]
						}
					})
				}
			})
			target := strings.TrimSpace(a.retargetPath)
			if strings.HasPrefix(target, "~/") {
				target = filepath.Join(a.env.home, target[2:])
			}
			warn, err := skills.CheckTarget(target)
			if err != nil {
				ui.Text(c, err.Error()).FontSize(12).TextColor(t.Danger)
			} else if warn != "" {
				ui.Text(c, warn).FontSize(12).TextColor(t.Warning)
			}
			buttons(c, "Change", err != nil, func() { a.retargetOpen = false }, a.retarget)
		})
	})

	ui.Modal(c, &a.convertOpen, func() {
		s := a.skill()
		if s == nil {
			a.convertOpen = false
			return
		}
		dialogBody(c, func() {
			ui.Text(c, "Move & Link "+s.Name).FontSize(17).Bold()
			ui.Text(c, "Moves the folder to a shared place and leaves a link to it here, so other projects and agents can link to the same skill.").TextColor(t.TextMuted)
			dests := a.convertDestinations()
			labels := make([]string, 0, len(dests)+1)
			for _, d := range dests {
				labels = append(labels, a.tilde(d))
			}
			other := a.convertDest != "" && !contains(dests, a.convertDest)
			if other {
				labels = append(labels, a.tilde(a.convertDest))
			}
			chosen := a.tilde(a.convertDest)
			field(c, "Move to", func() {
				ui.Row(c).Gap(8).Children(func() {
					if ui.Select(c, &chosen, labels).Label("Move to").Grow(1).Changed() {
						for i, l := range labels {
							if l == chosen && i < len(dests) {
								a.convertDest = dests[i]
							}
						}
					}
					if ui.Button(c, "Other…").Clicked() {
						a.env.pickFolders("Choose where to move the skill", false, func(p []string) {
							if len(p) > 0 {
								a.convertDest = p[0]
							}
						})
					}
				})
			})
			dest := filepath.Join(a.convertDest, s.Name)
			var err error
			if _, e := os.Lstat(dest); e == nil {
				err = fmt.Errorf("%s already exists", a.tilde(dest))
			} else if filepath.Clean(a.convertDest) == filepath.Dir(s.Path) {
				err = fmt.Errorf("the skill is already there")
			}
			ui.Text(c, a.tilde(dest)+"  ←  link at "+a.tilde(s.Path)).FontSize(12).Font("monospace").TextColor(t.TextMuted)
			if err != nil {
				ui.Text(c, err.Error()).FontSize(12).TextColor(t.Danger)
			}
			buttons(c, "Move & Link", err != nil, func() { a.convertOpen = false }, a.convert)
		})
	})

	if s := a.skill(); s != nil {
		title, msg, button := "Move “"+s.Name+"” to the Trash?", a.tilde(s.Path), "Move to Trash"
		if s.IsLink {
			title, button = "Remove the link “"+s.Name+"”?", "Remove Link"
			msg = "Only the link at " + a.tilde(s.Path) + " is removed. " + s.Target() + " stays."
		} else if deps := a.snap.LinksInto(s.RealPath, s.Path); len(deps) > 0 {
			msg += "\n\n" + plural(len(deps), "link points", "links point") + " into it and will break:"
			for _, d := range deps {
				msg += "\n" + a.tilde(d.Path)
			}
		}
		if ui.AlertDialog(c, &a.deleteOpen, title, msg, "Cancel", button) == 1 {
			a.deleteSkill()
		}
		if ui.AlertDialog(c, &a.materializeOpen, "Replace the link with a copy?",
			"Copies "+a.tilde(s.RealPath)+" to "+a.tilde(s.Path)+". Later changes to either are no longer shared.",
			"Cancel", "Make Copy") == 1 {
			a.materialize()
		}
	}

	ui.Modal(c, &a.newFileOpen, func() {
		dialogBody(c, func() {
			ui.Text(c, "New File").FontSize(17).Bold()
			ui.TextInput(c, &a.newFileRel).Placeholder("references/guide.md").Label("File path").AutoFocus()
			ui.Text(c, "A path inside the skill's folder; folders on the way are created.").FontSize(12).TextColor(t.TextMuted)
			buttons(c, "Create", strings.TrimSpace(a.newFileRel) == "", func() { a.newFileOpen = false }, a.createFile)
		})
	})

	{
		f := a.deleteFile
		title, msg, button := "Move “"+f.Rel+"” to the Trash?", "", "Move to Trash"
		if f.IsLink {
			title, msg, button = "Remove the link “"+f.Rel+"”?", "Only the link is removed; "+f.Target+" stays.", "Remove Link"
		}
		if ui.AlertDialog(c, &a.deleteFileOpen, title, msg, "Cancel", button) == 1 {
			a.removeFile()
		}
	}

	switch ui.AlertDialog(c, &a.discardOpen, "Save changes to "+filepath.Base(a.filePath)+"?",
		"Your edits will be lost if you don't save them.", "Cancel", "Discard", "Save") {
	case 1:
		a.edit = nil
		a.runPending()
	case 2:
		a.save()
		if !a.dirty() {
			a.runPending()
		}
	case ui.AlertDismissed:
		a.pending = nil
	}

	ui.AlertDialog(c, &a.errOpen, a.errTitle, a.errText, "OK")
}

func (a *app) runPending() {
	fn := a.pending
	a.pending = nil
	if fn != nil {
		fn()
	}
}

func dialogBody(c *ui.Context, fn func()) {
	ui.Column(c).Width(520).Gap(12).Children(fn)
}

func field(c *ui.Context, label string, fn func()) {
	ui.Column(c).Gap(4).Children(func() {
		ui.Text(c, label).FontSize(12).Bold().TextColor(c.Theme().TextMuted)
		fn()
	})
}

func buttons(c *ui.Context, ok string, disabled bool, cancel, confirm func()) {
	ui.Row(c).Gap(8).Justify(ui.End).Padding(4, 0, 0, 0).Children(func() {
		if ui.Button(c, "Cancel").Clicked() {
			cancel()
		}
		if ui.PrimaryButton(c, ok).Disabled(disabled).Clicked() {
			confirm()
		}
	})
}

func contains(xs []string, x string) bool {
	for _, v := range xs {
		if v == x {
			return true
		}
	}
	return false
}

// scopeChoices lists the scopes for the link dialog: global, then every
// project.
func (a *app) scopeChoices() (labels, roots []string) {
	labels, roots = []string{"Global (~)"}, []string{""}
	if a.snap != nil {
		for _, p := range a.snap.Projects {
			labels = append(labels, p.Name+" — "+a.tilde(p.Root))
			roots = append(roots, p.Root)
		}
	}
	return labels, roots
}

func (a *app) openRename(s *skills.Skill) {
	a.renameName = s.Name
	a.renameMeta = true
	a.renameRelink = true
	a.renameOpen = true
}

func (a *app) openLink(s *skills.Skill) {
	a.linkScope = a.scope
	a.linkName = s.Name
	a.linkAgents = [3]bool{}
	a.linkOpen = true
}

func plural(n int, one, many string) string {
	if n == 1 {
		return "1 " + one
	}
	return fmt.Sprintf("%d %s", n, many)
}
