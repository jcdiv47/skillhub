# SkillHub

A desktop app for managing agent skills across the projects on your machine,
built with [MyGo](https://github.com/egoist/mygo) native UI.

- **Locations.** Global: `~/.agents/skills`, `~/.claude/skills`,
  `~/.pi/agent/skills`. Project: `<project>/.agents/skills`,
  `<project>/.claude/skills`, `<project>/.pi/skills`.
- **Projects.** On first run, choose the folders to scan (and how deep).
  Projects are the folders holding any project skills directory;
  `node_modules`, hidden folders and build outputs are skipped. Change them
  with **Folders…**.
- **Links.** Every skill shows its path, each link hop, and the real
  path, with Copy and Reveal. Links to links, broken links and loops are
  flagged; a skills directory that is itself a link (as `.claude` → elsewhere)
  shows a link icon on its section.
- **Preview.** SKILL.md's frontmatter as a table and its body as Markdown,
  with highlighted code. Links to files of the skill open them in the editor.
- **Edit.** Every file under the skill's folder, in a code editor with
  syntax highlighting. Saving writes through links (the link stays a link).
  New file, delete file (to the Trash; links inside are only unlinked).
- **Manage.** New skill, rename (updating `name:` and the links pointing
  at it), link into other locations, change a link's target, replace a link
  with a real copy, move a folder to a shared place and leave a link,
  remove a link or move a folder to the Trash (listing links that would
  break).
- **Checks.** Missing SKILL.md or frontmatter, missing `name` or
  `description`, a name that differs from the folder or breaks the Agent
  Skills naming rules, and skills of the same name across the scope and
  global (badge `×N`, with whether they are the same folder).
- **Live.** Watches the folders shown and reads them again on change, and
  when the window comes back to the front.

## Develop

MyGo is used from a checkout at `../../GitHub/mygo` (see `go.mod`).

```sh
go test ./...                    # core + views, headless
SKILLHUB_SHOTS=/tmp/shots go test .   # also save PNGs of the views
go run .                         # run the app
go tool mygo build -skip-dmg     # package: build/<os>-<arch>/
```

End-to-end, with the automation plugin:

```sh
go build -tags e2e -o /tmp/skillhub-e2e .
MYGO_AUTOMATION=/tmp/sh.sock HOME=/tmp/fake-home SKILLHUB_CONFIG=/tmp/cfg /tmp/skillhub-e2e
```

`SKILLHUB_CONFIG` overrides the settings folder.
