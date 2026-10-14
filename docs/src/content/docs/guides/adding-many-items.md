---
title: Adding many items
description: Write each item's text to a file, then add them with one command each.
---

Write each item's text as a markdown file in a scratch folder, then run one
[`primum add`](/commands/add/) per item. `--body-file` reads the text from a
file, so nothing has to be quoted on the command line.

```bash
dir=$(mktemp -d)

cat > "$dir/csv.md" <<'END'
## Why

Işıl wants the open items as CSV for the weekly review.
END

cat > "$dir/crash.md" <<'END'
## Steps

1. Open the board with an empty `items/` folder.
2. Click "+ new".
END

primum add TODO "Export open items as CSV" --impact 6 --effort 3 --body-file "$dir/csv.md"
primum add BUG "Empty board crashes on + new" --urgency 8 --body-file "$dir/crash.md"
rm -r "$dir"
```

Each `add` takes the next number in its lane, writes `items/<ID>.md`,
rewrites `BOARD.md`, and is one commit in the history. Specs and plans go
beside an item the same way:

```bash
primum doc TODO:7 spec --file "$dir/spec.md"
```

## From standard input

`-` reads the text from standard input, which suits a script or an agent
producing text:

```bash
generate-report | primum add TODO "Weekly review findings" --body-file -
```
