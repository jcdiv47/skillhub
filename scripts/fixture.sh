#!/bin/sh
# Makes a home folder with global skills, projects and links between them, to look at the app
# without touching your own skills:
#
#   scripts/fixture.sh /tmp/skillhub-fx
#   SKILLHUB_HOME=/tmp/skillhub-fx/home SKILLHUB_CONFIG=/tmp/skillhub-fx/config cargo run
set -eu

root=${1:?usage: fixture.sh DIR}
rm -rf "$root"
home=$root/home
code=$home/Code
mkdir -p "$root/config"

skill() { # dir name description [body]
    mkdir -p "$1/$2"
    printf -- '---\nname: %s\ndescription: %s\n%s---\n\n%s\n' "$2" "$3" "${5:-}" "${4:-# $2}" >"$1/$2/SKILL.md"
}

shared=$home/.agents/skills
skill "$shared" pdf "Fill and read PDF forms. Use when a task mentions PDFs, forms or extracting text from scans." \
    '# PDF

Use **pdftk** for forms and `pdftotext` for text. See [the guide](references/guide.md).

## Fill a form

1. Dump the fields.
2. Write an FDF with the values.
3. Fill and flatten.

```bash
pdftk in.pdf dump_data_fields
pdftk in.pdf fill_form values.fdf output out.pdf flatten
```

| Tool | For |
| --- | --- |
| pdftk | forms, merging |
| pdftotext | text |

> Scans need OCR first.' \
    'allowed-tools:
  - Read
  - Bash
license: MIT
'
mkdir -p "$shared/pdf/references" "$shared/pdf/scripts"
printf '# Guide\n\nField names are case sensitive.\n' >"$shared/pdf/references/guide.md"
printf '#!/usr/bin/env python3\nimport sys\n\n\ndef main(path: str) -> int:\n    """Print the fields of a PDF form."""\n    print(f"fields of {path}")\n    return 0\n\n\nif __name__ == "__main__":\n    sys.exit(main(sys.argv[1]))\n' >"$shared/pdf/scripts/fields.py"
skill "$shared" review "Review code changes for bugs before they merge." '# Review

Read the diff, then the code around it.'
skill "$shared" commit-messages "Write commit messages that say why." '# Commit messages'

mkdir -p "$home/.claude/skills"
ln -s "$shared/pdf" "$home/.claude/skills/pdf"
mkdir -p "$home/.claude/skills/notes"
printf 'no frontmatter here\n' >"$home/.claude/skills/notes/SKILL.md"
skill "$home/.pi/agent/skills" pi-only "Only for pi." '# Pi'

web=$code/web
skill "$web/.claude/skills" deploy "Deploy the site to production and check it came up." '# Deploy'
ln -s "$shared/pdf" "$web/.claude/skills/pdf"
ln -s "$home/gone" "$web/.claude/skills/old"
skill "$web/.pi/skills" lint "Lint the code." '# Lint'

skill "$code/api/.agents/skills" review "Project review rules." '# Review here'
skill "$code/api/.agents/skills" Bad_Name "A name the spec does not allow." '# Bad'
skill "$code/tools/cli/.claude/skills" release "Cut a release." '# Release'

printf '{\n  "roots": ["%s"],\n  "depth": 3\n}\n' "$code" >"$root/config/config.json"
echo "$root"
