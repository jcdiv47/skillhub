# SkillHub

A desktop app for managing agent skills across the projects on your machine,
built with [Rune](https://github.com/anomalyco/rune), a native GPU UI framework.

- **Locations.** Global: `~/.agents/skills`, `~/.claude/skills`,
  `~/.pi/agent/skills`. Project: `<project>/.agents/skills`,
  `<project>/.claude/skills`, `<project>/.pi/skills`.
- **Projects.** On first run, choose the folders to scan (and how deep).
  Projects are the folders holding any project skills directory;
  `node_modules`, hidden folders and build outputs are skipped. Change them
  with **Folders…**.
- **One thing at a time.** The sidebar picks a scope, the list shows its
  skills under the folder each is in, and the pane shows the one chosen. A
  folder linked to another of the scope folds to its title. A row
  marks only what stands out: a link, a problem, a name used more than once.
  Folders with no skills are left out; **New Skill** names all three.
- **Preview.** The description, then SKILL.md's body as Markdown with
  highlighted code. The rest of the frontmatter folds under its keys. Links to
  files of the skill open them in the editor.
- **Edit.** Opens the skill's files over the window: every file under its
  folder, in an editor with syntax colours (SKILL.md alone shows without the
  list). Save and Revert show once there is something to save (⌘S). Saving
  writes through links (the link stays a link). New file; delete from the
  right-click menu (to the Trash; links are only unlinked). Closing asks
  about edits not saved.
- **Links.** Where a link leads, hop by hop, and the other links to the same
  folder; each row goes to that skill.
- **Actions (···).** Rename (updating `name:` and the links pointing at it),
  link into other locations, change a link's target, replace a link with a
  real copy, move a folder to a shared place and leave a link, copy paths,
  reveal, and remove a link or move a folder to the Trash (listing the links
  that would break).
- **Checks.** Missing SKILL.md or frontmatter, missing `name` or
  `description`, a name that differs from the folder or breaks the Agent
  Skills naming rules, broken links and loops.
- **Live.** Watches the folders shown and reads them again on change, and
  when the window comes back to the front.

Keys: ⌘N new skill, ⌘F filter, ⌘R rescan, ⌘S save, ⇧⌘B sidebar.

## Develop

Rune is pinned to one rev of its `dev` branch in `Cargo.toml` (the dependency
and each `[patch.crates-io]` entry share it).

```sh
cargo test                 # core, and the app driven headless
cargo run                  # the window
scripts/mac-app.sh --install   # macOS: build/SkillHub.app, copied to /Applications
```

`src/tests.rs` runs the real view in a `rune::Runtime`: it presses elements by
key, types, answers folder dialogs, and checks the disk. No window, no GPU.

To look at the app without a window, or without your own skills:

```sh
scripts/fixture.sh /tmp/skillhub-fx
export SKILLHUB_HOME=/tmp/skillhub-fx/home SKILLHUB_CONFIG=/tmp/skillhub-fx/config
cargo run -- --list-scenes
cargo run -- --snap files.png --scene files --theme dark
cargo run -- --tree tree.json --scene link          # the element tree
cargo run -- --script drive.json --scene link --snap out.png --at mark:end
```

A script is timed input by element key, for example
`[{"t":0.2,"type":"tap","key":"actions"},{"t":1,"type":"mark","name":"end"}]`.
Release builds take these flags with `--features tooling`.

`SKILLHUB_HOME` stands in for the home folder and `SKILLHUB_CONFIG` for the
settings folder (`config.json`; by default the system's, as
`~/Library/Application Support/SkillHub`).
