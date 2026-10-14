---
title: Workspaces
description: One folder holding your boards, found from wherever you run primum. Nothing machine-wide.
---

primum finds everything from the folder it runs in. Nothing lives in
`~/.config`: copy or back up a workspace folder and everything comes with it.

```text
~/boards/
  primum.toml          the workspace: which projects, in tab order
  my-app/
    project.json       the project's name and settings
    items/
  my-lib/
    project.json
    items/
```

## Making one

```bash
mkdir ~/boards && cd ~/boards
primum workspace init       # primum.toml listing the projects below it
primum init my-app          # a project in the subfolder my-app
primum init my-lib
```

`primum workspace init` in a folder that already holds projects lists them.

## primum.toml

Written by people, and optional in what it says:

```toml
projects = ["my-app", "my-lib"]
```

Entries are subfolder names, in tab order. Without `projects`, every
subfolder holding `project.json` is a project, by name. A listed folder that
is missing or broken shows as unavailable without stopping the others, and
two projects with one name are refused: tabs and `-p` tell projects apart by
name.

```bash
primum projects                  # name, folder, and whether it opens
primum projects add my-lib       # put a subfolder on the list
primum projects remove my-lib    # take it off; the project is not touched
```

## project.json

Written by primum, and tracked in the project's history, so it syncs
between machines and arrives with `primum clone`:

```json
{
  "name": "my-app"
}
```

`name` is what tabs and `-p` show. Keys primum does not know are kept when
it rewrites the file.

## Picking a project

- Inside a project folder (or any folder below it), commands use that
  project.
- `-p NAME` picks another project of the workspace.
- At the workspace root, a command uses the only project, else asks for
  `-p NAME` and lists the names.

```bash
cd ~/boards/my-app && primum next      # my-app
primum next -p my-lib                  # the other one
cd ~/boards && primum serve            # every project as a tab
```

A project with no workspace above it works on its own: `primum init` in any
folder makes one.

## Moving boards from earlier versions

Earlier versions kept a list of projects in `~/.config/primum/projects.toml`
and each project's settings in its own `primum.toml`. Those boards keep
working. To convert them, move the project folders into one folder, then:

```bash
cd ~/boards
primum workspace migrate
```

Each project's `primum.toml` becomes `project.json`, committed in its
history as "move settings to project.json", and the folder becomes a
workspace listing them in the old list's order. Running it again changes
nothing. Delete `~/.config/primum/projects.toml` when you are done; primum
no longer reads it.

## Keep boards apart from code

Keep a workspace in its own folder, for example `~/boards`, not in a code
repository: a board's history and sync are separate from the code's.
