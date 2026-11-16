package main

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/egoist/mygo/ui"
	"github.com/egoist/mygo/ui/editor"
	"github.com/egoist/mygo/ui/markdown"

	"skillhub/internal/skills"
)

func (a *app) view(c *ui.Context) {
	if a.notice != "" {
		c.Toast(a.notice)
		a.notice = ""
	}
	if c.Shortcut(ui.Cmd, ui.KeyS) {
		a.save()
	}
	if a.landing() {
		a.landingView(c)
	} else {
		a.mainView(c)
	}
	a.dialogs(c)
}

// landingView asks for the folders to scan, the first time.
func (a *app) landingView(c *ui.Context) {
	t := c.Theme()
	ui.Column(c).Fill().Center().Padding(32).Children(func() {
		ui.Column(c).Width(560).Gap(16).Padding(32).Radius(14).Background(t.Surface).Border(1, t.Border).Children(func() {
			ui.Text(c, "Welcome to SkillHub").FontSize(24).Bold()
			ui.Text(c, "Choose the folders that hold your projects. SkillHub looks in them for "+
				".agents/skills, .claude/skills and .pi/skills. Global skills in your home folder "+
				"(~/.agents/skills, ~/.claude/skills and ~/.pi/agent/skills) always show.").TextColor(t.TextMuted)
			a.rootsEditor(c)
			ui.Row(c).Gap(8).Justify(ui.End).Children(func() {
				if ui.PrimaryButton(c, "Scan").Disabled(len(a.cfg.Roots) == 0).Clicked() {
					a.welcome = false
					a.saveConfig()
					a.scan()
				}
			})
		})
	})
}

// rootsEditor lists the folders to scan, with buttons to add and remove.
func (a *app) rootsEditor(c *ui.Context) {
	t := c.Theme()
	ui.Column(c).Gap(6).Children(func() {
		if len(a.cfg.Roots) == 0 {
			ui.Text(c, "No folders chosen yet.").TextColor(t.TextMuted).Italic()
		}
		for _, r := range a.cfg.Roots {
			ui.Row(c.Key(r)).Gap(8).Padding(6, 10).Radius(8).Background(t.Background).Border(1, t.Border).Children(func() {
				ui.Icon(c, iconFolder).TextColor(t.TextMuted)
				ui.Text(c, a.tilde(r)).Grow(1).SingleLine()
				if ui.Button(c, "Remove").Label("Remove " + a.tilde(r)).Clicked() {
					a.cfg.RemoveRoot(r)
				}
			})
		}
		ui.Row(c).Gap(10).AlignItems(ui.Center).Children(func() {
			if ui.Button(c, "Choose Folders…").Clicked() {
				a.env.pickFolders("Choose folders to scan", true, func(paths []string) {
					a.cfg.AddRoots(paths...)
				})
			}
			ui.Spacer(c)
			ui.Text(c, "Depth").TextColor(t.TextMuted)
			depth := fmt.Sprint(a.cfg.Depth)
			if ui.Select(c, &depth, []string{"1", "2", "3", "4", "5", "6", "8"}).Label("Depth").Changed() {
				fmt.Sscan(depth, &a.cfg.Depth)
			}
		})
	})
}

func (a *app) mainView(c *ui.Context) {
	t := c.Theme()
	ui.Split(c, &a.sidebarWidth, func() {
		a.sidebar(c)
	}, func() {
		ui.Split(c, &a.listWidth, func() {
			a.skillList(c)
		}, func() {
			ui.Column(c).Fill().Background(t.Background).Children(func() { a.detail(c) })
		}).Fill()
	}).Fill()
}

func (a *app) sidebar(c *ui.Context) {
	t := c.Theme()
	ui.Column(c).Fill().Background(t.Surface).Children(func() {
		ui.Row(c).Padding(12, 12, 8, 14).Gap(6).AlignItems(ui.Center).Children(func() {
			ui.Text(c, "SkillHub").FontSize(16).Bold().Grow(1)
			if a.scanning {
				ui.Spinner(c).Size(16, 16)
			}
			if iconButton(c, iconRefresh, "Rescan").Disabled(a.scanning).Clicked() {
				a.scan()
			}
			if ui.Button(c, "Folders…").Tooltip("Choose the folders to scan").Clicked() {
				a.settingsOpen = true
			}
		})
		// Global.
		global := a.scope == ""
		count := 0
		if a.snap != nil {
			count = a.snap.GlobalCount()
		}
		a.scopeRow(c, "global", iconGlobe, "Global", "~", count, global, func() { a.selectScope("") })
		ui.Row(c).Padding(14, 14, 6, 14).Gap(6).AlignItems(ui.Center).Children(func() {
			n := 0
			if a.snap != nil {
				n = len(a.snap.Projects)
			}
			ui.Textf(c, "PROJECTS · %d", n).FontSize(11).Bold().TextColor(t.TextMuted)
		})
		ui.Box(c).Padding(0, 10, 8, 10).Children(func() {
			ui.SearchField(c, &a.projQuery).Placeholder("Filter projects").Label("Filter projects")
		})
		projects := a.visibleProjects()
		a.projects.Key = func(i int) any { return projects[i].Root }
		a.projects.Label = func(i int) string { return projects[i].Name }
		a.projects.Selected = &a.projRow
		list := ui.List(c.Key("projects"), &a.projects, len(projects)).Grow(1).Padding(0, 6, 8, 6)
		list.Rows(func(row ui.ListRow) {
			a.projectRow(row.Context, projects[row.Index], row.Selected() && row.ListFocused())
		})
		list.Children(func() {
			if len(projects) == 0 && a.snap != nil {
				msg := "No projects with skills found."
				if a.projQuery != "" {
					msg = "No projects match."
				}
				ui.Text(c, msg).TextColor(t.TextMuted).Padding(12)
			}
		})
		if list.Changed() && a.projRow >= 0 && a.projRow < len(projects) {
			a.selectScope(projects[a.projRow].Root)
			if a.scope != projects[a.projRow].Root { // waiting on unsaved edits
				a.syncProjectRow()
			}
		}
	})
}

func (a *app) scopeRow(c *ui.Context, key string, ic *ui.SVG, name, path string, count int, chosen bool, choose func()) {
	t := c.Theme()
	row := ui.Row(c.Key(key)).Gap(10).Padding(8, 10).Margin(0, 6).Radius(8).AlignItems(ui.Center).Label(name).Focusable()
	if chosen {
		row.Background(t.Accent.Alpha(0.16))
	} else if row.Hovered() {
		row.Background(t.SurfaceHover)
	}
	if row.Clicked() {
		choose()
	}
	row.Children(func() {
		ui.Icon(c, ic).TextColor(t.TextMuted)
		ui.Column(c).Grow(1).Shrink(1).Children(func() {
			ui.Text(c, name).Bold().SingleLine()
			ui.Text(c, path).FontSize(11).TextColor(t.TextMuted).SingleLine()
		})
		ui.Badge(c, fmt.Sprint(count))
	})
}

func (a *app) projectRow(c *ui.Context, p *skills.Project, onAccent bool) {
	t := c.Theme()
	muted := t.TextMuted
	if onAccent {
		muted = t.AccentText.Alpha(0.8)
	}
	ui.Row(c).Gap(10).Padding(7, 10).Radius(8).AlignItems(ui.Center).Children(func() {
		ui.Icon(c, iconFolder).TextColor(muted)
		ui.Column(c).Grow(1).Shrink(1).Children(func() {
			ui.Text(c, p.Name).FontWeight(600).SingleLine()
			ui.Text(c, a.tilde(p.Root)).FontSize(11).TextColor(muted).SingleLine()
		})
		ui.Badge(c, fmt.Sprint(p.Count()))
	})
}

func iconButton(c *ui.Context, ic *ui.SVG, label string) ui.Element {
	return ui.Button(c, "").Label(label).Tooltip(label).Padding(4, 6).Children(func() {
		ui.Icon(c, ic).FontSize(15)
	})
}

// skillList lists the skills of the scope shown, by location.
func (a *app) skillList(c *ui.Context) {
	t := c.Theme()
	ui.Column(c).Fill().Background(t.Background).BorderWidth(0, 1, 0, 1).BorderColor(t.Border).Children(func() {
		ui.Column(c).Padding(12, 14, 8, 14).Gap(8).Children(func() {
			title, sub := "Global skills", "~"
			if a.snap != nil {
				if p := a.snap.Project(a.scope); p != nil {
					title, sub = p.Name, a.tilde(p.Root)
				}
			}
			ui.Column(c).Children(func() {
				ui.Text(c, title).FontSize(16).Bold().SingleLine()
				ui.Text(c, sub).FontSize(11).TextColor(t.TextMuted).SingleLine().Selectable()
			})
			ui.SearchField(c, &a.skillQuery).Placeholder("Filter skills").Label("Filter skills")
		})
		ui.Scroll(c.Key("skills")).Grow(1).Padding(0, 8, 12, 8).Gap(4).Children(func() {
			if a.snap == nil {
				ui.Row(c).Gap(8).Padding(16).Children(func() {
					ui.Spinner(c).Size(16, 16)
					ui.Text(c, "Scanning…").TextColor(t.TextMuted)
				})
				return
			}
			for _, l := range a.snap.Locations(a.scope) {
				a.locationSection(c, l)
			}
		})
	})
}

func (a *app) matches(s *skills.Skill) bool {
	q := strings.ToLower(strings.TrimSpace(a.skillQuery))
	return q == "" || strings.Contains(strings.ToLower(s.Name), q) || strings.Contains(strings.ToLower(s.Meta.Description), q)
}

func (a *app) locationSection(c *ui.Context, l *skills.Location) {
	t := c.Theme()
	ui.Column(c.Key(l.Dir)).Gap(2).Children(func() {
		ui.Row(c).Gap(6).Padding(10, 6, 4, 6).AlignItems(ui.Center).Children(func() {
			ui.Text(c, strings.ToUpper(l.Agent.String())).FontSize(11).Bold().TextColor(t.TextMuted)
			ui.Text(c, filepath.ToSlash(skills.RelDir(l.Agent, l.Scope))).FontSize(11).Font("monospace").TextColor(t.TextMuted).SingleLine().Shrink(1)
			if len(l.Links) > 0 {
				hop := l.Links[len(l.Links)-1]
				ui.Icon(c, iconLink).FontSize(12).TextColor(t.Accent).Label("Linked folder").
					Tooltip(a.tilde(hop.Link) + " → " + hop.Target)
			}
			ui.Spacer(c)
			if iconButton(c, iconPlus, "New skill in "+l.Label()).Clicked() {
				a.newSkillLoc, a.newName, a.newDesc, a.newSkillOpen = l, "", "", true
			}
		})
		switch {
		case l.Err != "":
			ui.Text(c, l.Err).FontSize(12).TextColor(t.Danger).Padding(2, 6)
		case !l.Exists:
			ui.Text(c, "Not created").FontSize(12).TextColor(t.TextMuted).Italic().Padding(2, 6)
		case len(l.Skills) == 0:
			ui.Text(c, "Empty").FontSize(12).TextColor(t.TextMuted).Italic().Padding(2, 6)
		}
		for _, s := range l.Skills {
			if a.matches(s) {
				a.skillRow(c, s)
			}
		}
	})
}

func (a *app) skillRow(c *ui.Context, s *skills.Skill) {
	t := c.Theme()
	chosen := s.Path == a.skillPath
	row := ui.Column(c.Key(s.Path)).Gap(2).Padding(7, 10).Radius(8).Label(s.Name).Focusable()
	if chosen {
		row.Background(t.Accent.Alpha(0.16))
	} else if row.Hovered() {
		row.Background(t.SurfaceHover)
	}
	if row.Clicked() {
		a.selectSkill(s.Path)
	}
	row.Children(func() {
		ui.Row(c).Gap(6).AlignItems(ui.Center).Children(func() {
			kindIcon(c, s)
			ui.Text(c, s.Name).FontWeight(600).SingleLine().Shrink(1)
			ui.Spacer(c)
			problemIcon(c, s)
			if same := a.snap.Same(s, a.scope); len(same) > 0 {
				ui.Badge(c, fmt.Sprintf("×%d", len(same)+1)).Tooltip(sameTip(a, s, same))
			}
		})
		desc := s.Meta.Description
		if s.Broken {
			desc = "→ " + a.tilde(s.Target()) + " (missing)"
		}
		if desc != "" {
			ui.Text(c, desc).FontSize(12).TextColor(t.TextMuted).MaxLines(2)
		}
	})
}

func sameTip(a *app, s *skills.Skill, same []*skills.Skill) string {
	lines := []string{"Also named " + s.Name + ":"}
	for _, o := range same {
		note := ""
		if o.RealPath != "" && o.RealPath == s.RealPath {
			note = " (same folder)"
		}
		lines = append(lines, a.tilde(o.Path)+note)
	}
	return strings.Join(lines, "\n")
}

func kindIcon(c *ui.Context, s *skills.Skill) {
	t := c.Theme()
	switch {
	case s.Broken:
		ui.Icon(c, iconBroken).FontSize(14).TextColor(t.Danger).Label("Broken link")
	case s.IsLink:
		ui.Icon(c, iconLink).FontSize(14).TextColor(t.Accent).Label("Link")
	default:
		ui.Icon(c, iconFolder).FontSize(14).TextColor(t.TextMuted).Label("Folder")
	}
}

func problemIcon(c *ui.Context, s *skills.Skill) {
	sev, ok := s.Worst()
	if !ok {
		return
	}
	t := c.Theme()
	color := t.Warning
	if sev == skills.Error {
		color = t.Danger
	}
	var msgs []string
	for _, p := range s.Problems {
		msgs = append(msgs, p.Message)
	}
	ui.Icon(c, iconAlert).FontSize(14).TextColor(color).Label(fmt.Sprintf("%d problems", len(s.Problems))).Tooltip(strings.Join(msgs, "\n"))
}

// detail shows the skill chosen.
func (a *app) detail(c *ui.Context) {
	t := c.Theme()
	s := a.skill()
	if s == nil {
		ui.Column(c).Fill().Center().Gap(6).Children(func() {
			ui.Text(c, "No skill selected").FontSize(16).TextColor(t.TextMuted)
			ui.Text(c, "Choose a skill to see its content, files and links.").TextColor(t.TextMuted)
		})
		return
	}
	ui.Column(c.Key(s.Path)).Fill().Children(func() {
		ui.Column(c).Padding(16, 20, 10, 20).Gap(10).BorderWidth(0, 0, 1, 0).BorderColor(t.Border).Children(func() {
			ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
				kindIcon(c, s)
				ui.Text(c, s.Name).FontSize(20).Bold().SingleLine().Shrink(1)
				ui.Badge(c, s.Location.Agent.String()+" · "+s.Location.Scope.String())
			})
			a.actions(c, s)
			a.paths(c, s)
			for _, p := range s.Problems {
				color := t.Warning
				if p.Severity == skills.Error {
					color = t.Danger
				}
				ui.Row(c).Gap(6).AlignItems(ui.Center).Children(func() {
					ui.Icon(c, iconAlert).FontSize(13).TextColor(color)
					ui.Text(c, p.Message).FontSize(12).TextColor(color).Selectable()
				})
			}
		})
		links := a.snap.LinksTo(s.RealPath, s.Path)
		ui.Box(c).Padding(8, 20, 0, 20).Children(func() {
			ui.Tabs(c, &a.tab, "Preview", "Files", fmt.Sprintf("Links (%d)", len(links)))
		})
		ui.Column(c).Grow(1).Children(func() {
			switch a.tab {
			case tabPreview:
				a.preview(c, s)
			case tabFiles:
				a.filesView(c, s)
			case tabLinks:
				a.linksView(c, s, links)
			}
		})
	})
}

func (a *app) actions(c *ui.Context, s *skills.Skill) {
	ui.Row(c).Gap(6).Wrap().Children(func() {
		if ui.Button(c, "Rename…").Clicked() {
			a.openRename(s)
		}
		if ui.Button(c, "Link Into…").Disabled(s.RealPath == "").Tooltip("Link this skill into other skills folders").Clicked() {
			a.openLink(s)
		}
		if s.IsLink {
			if ui.Button(c, "Change Target…").Clicked() {
				a.retargetPath = a.tildeTarget(s)
				a.retargetOpen = true
			}
			if ui.Button(c, "Make Real Copy").Disabled(s.Broken).Tooltip("Replace the link with a copy of the folder").Clicked() {
				a.materializeOpen = true
			}
		} else if ui.Button(c, "Move & Link…").Tooltip("Move the folder to a shared place and leave a link here").Clicked() {
			dests := a.convertDestinations()
			a.convertDest = dests[0]
			a.convertOpen = true
		}
		label := "Move to Trash"
		if s.IsLink {
			label = "Remove Link"
		}
		if ui.Button(c, label).TextColor(c.Theme().Danger).Clicked() {
			a.deleteOpen = true
		}
	})
}

func (a *app) tildeTarget(s *skills.Skill) string {
	if s.RealPath != "" {
		return s.RealPath
	}
	t := s.Target()
	if !filepath.IsAbs(t) {
		t = filepath.Join(filepath.Dir(s.Path), t)
	}
	return filepath.Clean(t)
}

// paths shows where the skill is: its entry, and for a link every hop and
// the real folder.
func (a *app) paths(c *ui.Context, s *skills.Skill) {
	t := c.Theme()
	ui.Column(c).Gap(4).Children(func() {
		a.pathRow(c, "Path", s.Path, true)
		for i, h := range s.Chain {
			label := "Links to"
			if i > 0 {
				label = "then to"
			}
			target := h.Target
			ui.Row(c.Key(fmt.Sprint("hop", i))).Gap(8).AlignItems(ui.Center).Children(func() {
				ui.Text(c, label).FontSize(12).TextColor(t.TextMuted).Width(64).Shrink(0)
				ui.Icon(c, iconArrowOr(s.Broken && i == len(s.Chain)-1)).FontSize(12).TextColor(t.Accent)
				ui.Text(c, target).FontSize(12).Font("monospace").Selectable().Grow(1).Shrink(1)
			})
		}
		switch {
		case s.Broken:
			ui.Text(c, "The link points to nothing.").FontSize(12).TextColor(t.Danger)
		case s.IsLink || s.RealPath != s.Path:
			a.pathRow(c, "Real path", s.RealPath, true)
		}
	})
}

func iconArrowOr(broken bool) *ui.SVG {
	if broken {
		return iconBroken
	}
	return iconLink
}

func (a *app) pathRow(c *ui.Context, label, path string, reveal bool) {
	t := c.Theme()
	ui.Row(c.Key(label)).Gap(8).AlignItems(ui.Center).Children(func() {
		ui.Text(c, label).FontSize(12).TextColor(t.TextMuted).Width(64).Shrink(0)
		ui.Text(c, a.tilde(path)).FontSize(12).Font("monospace").Selectable().Grow(1).Shrink(1)
		if iconButton(c, iconCopy, "Copy "+strings.ToLower(label)).Clicked() {
			c.WriteClipboard(path)
			a.notice = "Copied " + a.tilde(path)
		}
		if reveal && a.env.reveal != nil {
			if iconButton(c, iconReveal, "Reveal "+strings.ToLower(label)).Clicked() {
				a.env.reveal(path)
			}
		}
	})
}

// preview shows SKILL.md: its frontmatter as a table, its body as Markdown.
func (a *app) preview(c *ui.Context, s *skills.Skill) {
	t := c.Theme()
	ui.Scroll(c.Key("preview")).Grow(1).Padding(16, 20, 24, 20).Gap(16).Children(func() {
		if !s.HasSkillMD {
			ui.Text(c, "This skill has no SKILL.md to preview.").TextColor(t.TextMuted)
			return
		}
		ui.Row(c).Justify(ui.End).Children(func() {
			if ui.Button(c, "Edit SKILL.md").Clicked() {
				a.guard(func() {
					a.openFile(s.SkillMD())
					a.tab = tabFiles
				})
			}
		})
		m := s.Meta
		if m.Present {
			ui.Column(c).Radius(10).Border(1, t.Border).Background(t.Surface).Padding(4, 0).Children(func() {
				if m.Err != "" {
					ui.Text(c, "Frontmatter is not valid YAML: "+m.Err).TextColor(t.Danger).Padding(8, 14)
					ui.Text(c, m.Raw).Font("monospace").FontSize(12).Padding(8, 14).Selectable()
					return
				}
				for i, f := range m.Fields {
					ui.Row(c.Key(f.Key)).Gap(16).Padding(8, 14).AlignItems(ui.Start).Children(func() {
						ui.Text(c, f.Key).Font("monospace").FontSize(12).TextColor(t.TextMuted).Width(130).Shrink(0)
						v := ui.Text(c, f.Value).Selectable().Grow(1).Shrink(1)
						if f.Multiline {
							v.Font("monospace").FontSize(12)
						}
					}).BorderWidth(0, 0, borderIf(i < len(m.Fields)-1), 0).BorderColor(t.Border)
				}
			})
		}
		markdown.View(c, m.Body, &a.md)
	})
}

func borderIf(b bool) float32 {
	if b {
		return 1
	}
	return 0
}

// filesView lists the files of the skill, beside the editor of the one
// open.
func (a *app) filesView(c *ui.Context, s *skills.Skill) {
	t := c.Theme()
	if s.RealPath == "" {
		ui.Text(c, "The link is broken: there are no files to show.").TextColor(t.TextMuted).Padding(20)
		return
	}
	ui.Row(c).Grow(1).AlignItems(ui.Stretch).Children(func() {
		ui.Column(c).Width(220).Shrink(0).BorderWidth(0, 1, 0, 0).BorderColor(t.Border).Children(func() {
			ui.Row(c).Padding(8, 8, 4, 12).AlignItems(ui.Center).Children(func() {
				ui.Textf(c, "%d items", len(a.files)).FontSize(12).TextColor(t.TextMuted).Grow(1)
				if iconButton(c, iconPlus, "New file").Clicked() {
					a.guard(func() { a.newFileRel, a.newFileOpen = "", true })
				}
			})
			ui.Scroll(c.Key("files")).Grow(1).Padding(0, 6, 8, 6).Gap(1).Children(func() {
				for _, f := range a.files {
					a.fileRow(c, f)
				}
			})
		})
		ui.Column(c).Grow(1).Shrink(1).Children(func() { a.editorPane(c) })
	})
}

func (a *app) fileRow(c *ui.Context, f skills.File) {
	t := c.Theme()
	chosen := f.Path == a.filePath
	row := ui.Row(c.Key(f.Rel)).Gap(6).Padding(4, 8).PaddingStart(8 + 14*float32(f.Depth)).Radius(6).AlignItems(ui.Center).Label(f.Rel).Focusable()
	if chosen {
		row.Background(t.Accent.Alpha(0.16))
	} else if row.Hovered() {
		row.Background(t.SurfaceHover)
	}
	if row.Clicked() && !f.IsDir && !chosen {
		a.guard(func() { a.openFile(f.Path) })
	}
	row.ContextMenu(func(m *ui.Menu) {
		if m.Item("Copy Path").Chosen() {
			c.WriteClipboard(f.Path)
		}
		if a.env.reveal != nil && m.Item("Reveal").Chosen() {
			a.env.reveal(f.Path)
		}
		m.Separator()
		if m.Item("Delete…").Disabled(f.Rel == "SKILL.md").Chosen() {
			a.deleteFile, a.deleteFileOpen = f, true
		}
	})
	row.Children(func() {
		ic, color := iconFile, t.TextMuted
		switch {
		case f.Broken:
			ic, color = iconBroken, t.Danger
		case f.IsLink:
			ic, color = iconLink, t.Accent
		case f.IsDir:
			ic = iconFolder
		}
		ui.Icon(c, ic).FontSize(13).TextColor(color)
		name := filepath.Base(f.Rel)
		txt := ui.Text(c, name).FontSize(13).SingleLine().Shrink(1)
		if f.IsLink {
			txt.Tooltip(name + " → " + f.Target)
		}
	})
}

func (a *app) editorPane(c *ui.Context) {
	t := c.Theme()
	if a.filePath == "" {
		ui.Column(c).Fill().Center().Children(func() {
			ui.Text(c, "Choose a file to edit.").TextColor(t.TextMuted)
		})
		return
	}
	ui.Row(c).Padding(6, 10).Gap(8).AlignItems(ui.Center).BorderWidth(0, 0, 1, 0).BorderColor(t.Border).Children(func() {
		rel := a.filePath
		if s := a.skill(); s != nil {
			if r, err := filepath.Rel(s.Path, a.filePath); err == nil {
				rel = filepath.ToSlash(r)
			}
		}
		ui.Text(c, rel).Font("monospace").FontSize(12).SingleLine().Shrink(1)
		if a.dirty() {
			ui.Badge(c, "Edited")
		}
		ui.Spacer(c)
		if a.edit != nil {
			if ui.Button(c, "Revert").Disabled(!a.dirty()).Clicked() {
				a.revert()
			}
			if ui.PrimaryButton(c, "Save").Disabled(!a.dirty()).Tooltip("Save (⌘S)").Clicked() {
				a.save()
			}
		}
	})
	if a.changed {
		ui.Row(c).Padding(6, 10).Gap(8).AlignItems(ui.Center).Background(t.Warning.Alpha(0.15)).Children(func() {
			ui.Text(c, "The file changed on disk.").FontSize(12).Grow(1)
			if ui.Button(c, "Load From Disk").Clicked() {
				a.revert()
			}
		})
	}
	if a.edit == nil {
		ui.Column(c).Fill().Center().Children(func() {
			ui.Text(c, a.editNote).TextColor(t.TextMuted)
		})
		return
	}
	editor.View(c.Key(a.filePath), a.edit).Grow(1).FillWidth()
}

// linksView lists the links that lead to the skill's folder.
func (a *app) linksView(c *ui.Context, s *skills.Skill, links []*skills.Skill) {
	t := c.Theme()
	ui.Scroll(c.Key("links")).Grow(1).Padding(16, 20).Gap(8).Children(func() {
		if s.RealPath == "" {
			ui.Text(c, "The link is broken.").TextColor(t.TextMuted)
			return
		}
		intro := "Links in the scanned locations that lead to " + a.tilde(s.RealPath) + "."
		ui.Text(c, intro).TextColor(t.TextMuted)
		if s.IsLink {
			ui.Row(c).Gap(8).AlignItems(ui.Center).Children(func() {
				ui.Text(c, "The real folder:").FontSize(12).TextColor(t.TextMuted)
				orig := a.snap.Find(s.RealPath)
				if orig != nil {
					if ui.Link(c, a.tilde(orig.Path), "").Clicked() {
						a.goTo(orig)
					}
				} else {
					ui.Text(c, a.tilde(s.RealPath)).FontSize(12).Font("monospace")
				}
			})
		}
		if len(links) == 0 {
			ui.Text(c, "No other links lead here.").Italic().TextColor(t.TextMuted)
		}
		for _, l := range links {
			ui.Row(c.Key(l.Path)).Gap(10).Padding(8, 12).Radius(8).Border(1, t.Border).AlignItems(ui.Center).Children(func() {
				ui.Icon(c, iconLink).TextColor(t.Accent)
				ui.Column(c).Grow(1).Shrink(1).Children(func() {
					ui.Text(c, a.tilde(l.Path)).Font("monospace").FontSize(12).SingleLine()
					ui.Text(c, l.Location.Agent.String()+" · "+a.scopeName(l.Location)).FontSize(11).TextColor(t.TextMuted)
				})
				if ui.Button(c, "Show").Label("Show " + a.tilde(l.Path)).Clicked() {
					a.goTo(l)
				}
			})
		}
	})
}

func (a *app) scopeName(l *skills.Location) string {
	if l.Scope == skills.GlobalScope {
		return "Global"
	}
	return filepath.Base(l.Base)
}

// goTo shows another skill, in its scope.
func (a *app) goTo(s *skills.Skill) {
	a.guard(func() {
		root := ""
		if s.Location.Scope == skills.ProjectScope {
			root = s.Location.Base
		}
		a.scope = root
		a.skillQuery = ""
		a.syncProjectRow()
		a.closeSkill()
		a.skillPath = s.Path
		a.refreshFiles()
		a.openFile(a.defaultFile())
		a.tab = tabPreview
		a.watch()
	})
}
