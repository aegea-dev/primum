# History and sync: a bare git repository behind each project

**Goal.** Every change to a project is versioned without anyone committing,
and several people (or one person on several machines) can change the same
board and have their changes merged without dealing with merges. The
project folder does not need to be a git repository; the item files stay
plain markdown, as decision 1 of the primum spec has them.

A person sees a merge only when it cannot be decided: the same field of the
same item changed to different values on two sides.

## Decisions

1. **One bare repository per project, inside it.** `<project>/.primum/history.git`,
   with the project folder as its work tree, the way dotfiles are versioned.
   Inside the project so it travels with it: copying or syncing the folder
   carries the history. `.primum/.gitignore` holds `*`, so a project folder
   that is itself a git repository never takes the history in.

   It tracks only primum's files, through an allowlist in
   `history.git/info/exclude`:

   ```
   /*
   !/primum.toml
   !/BOARD.md
   !/items/
   /items/*
   !/items/*.md
   !/conflicts/
   ```

   The lock, temporary files and anything else in the folder are never
   tracked.

2. **primum shells out to `git`.** No git library: `git` is on nearly every
   machine that would use primum, its behaviour is the reference, and `gix`
   or `git2` would be the largest dependency in the crate. Every call passes
   `--git-dir` and `--work-tree`, and `-c core.hooksPath=/dev/null -c
   commit.gpgsign=false`, so a person's global hooks or signing setup never
   run on, or block, primum's commits. The author is the person's git
   identity (`user.name`, `user.email`) when they have one, else
   `primum <primum@localhost>`. `PRIMUM_GIT` names another `git` binary
   (tests use it to simulate a machine without git).

   **Without git, history is off, never broken.** Every command works as
   today, and a change prints `primum: history is off: git not found` once
   to stderr.

3. **Every change is one commit.** A change already takes the project lock,
   writes its files and settles; it now ends with a commit of exactly what
   changed, with a message naming the change:

   | change | message |
   |---|---|
   | `insert` | `TODO:88: add "Özgür'ün listesi"` |
   | `update` | `TODO:88: status doing` or `TODO:88: impact, effort` |
   | `append_note` | `TODO:88: note` |
   | `set_kind` | `TODO:88 -> BUG:4` |
   | `delete` | `TODO:88: delete` |
   | `set_doc` | `TODO:88: spec document` |
   | `import` | `import 237 items from <dir>` |
   | `init` | `primum project <name>` |

   **Edits made outside primum are kept, and kept apart.** Before a change,
   anything changed in the work tree since the last commit (a text editor,
   a script) is committed first as `edited outside primum`. An edit is
   never lost, and it is never credited to the command that came after it.

   The history starts by itself: a project without `.primum/` gets it on its
   first change, with a first commit of everything already there
   (`start history`). `primum init` creates it at once.

4. **Reading history.** `primum log [ID] [-n N] [--json]` lists commits:
   the whole project's, or one item's, following it across kind changes
   (`git log --follow`). The API gains `/api/history?id=ID` and
   `/api/history/diff?commit=C&path=P`; the item page gains a **History**
   tab beside the documents: each change's date, author and message, which
   opens to its diff.

5. **A remote is per machine.** `primum remote set URL [--branch B]
   [--manual]`, `primum remote`, `primum remote remove`. The URL lives in
   the history repository's own git config (`remote.origin.url`,
   `primum.branch`, `primum.sync`), not in `primum.toml`: two people reach
   the same remote by different URLs (ssh and https), and `primum.toml` is
   shared. Branch default `main`. Several projects may share one remote,
   each on its own branch; anyone with access to that remote sees every
   project on it.

   `primum clone URL [FOLDER] [--branch B]` makes a project from a remote:
   a bare clone into `FOLDER/.primum/history.git`, the files written from
   it, and the folder added to this machine's list.

6. **Sync is automatic.** With a remote and `primum.sync` not `manual`:
   - every command that changes a project syncs after its change;
   - `primum serve` syncs every 60 seconds and after each change, and the
     board reloads its items every 60 seconds while visible;
   - `primum sync` syncs on demand, and is the only sync when manual.

   Network calls have a 10 second limit, never prompt (`GIT_TERMINAL_PROMPT=0`,
   ssh `BatchMode=yes`), and failing them is not an error: the change stays
   committed locally and `primum: offline, changes stay local until the next
   sync` is printed once. A sync is: commit outside edits, fetch, renumber
   colliding new items, merge, push; when the push is refused because the
   remote moved meanwhile, again, up to three times.

7. **primum merges, git only carries.** Git's line merge cannot be used on
   item files: two people changing `status` and `impact`, adjacent lines,
   is a git conflict (measured with git 2.55). primum merges what git
   leaves unmerged, every time, by these rules:

   - **New ids that collide.** An item added on both sides with the same id
     (`TODO:88` here and on the remote) is two different items. Before the
     merge, the local one, which nobody else has seen, moves to the next
     number free on both sides, with its documents, its heading, every
     `needs` and `docs` naming it, and a commit `TODO:88 -> TODO:89: number
     taken on the remote`. The remote's numbers never change, since others
     may already use them.
   - **An item changed on both sides** is merged field by field against the
     common ancestor. A field changed on one side takes that change; changed
     identically on both, that value. `needs` and `docs` are sets: additions
     and removals from both sides apply. `finished` follows `status`. The
     body is a three-way text merge (`git merge-file`), clean when the edits
     touch different lines.
   - **Deleted on one side, changed on the other:** the changed item stays,
     and the sync reports `TODO:88 was deleted on one side and changed on
     the other; kept`.
   - **Documents** merge as text. Two edits of the same lines keep this
     side's text in place and the other side's beside it, as
     `items/TODO-88.spec.<short commit>.md`, a second document of that type,
     recorded as a document conflict.
   - **BOARD.md** is never merged; it is rebuilt after the merge, which also
     settles the merged items (closed ids leave every `needs`).

8. **A true conflict waits for a person, and stops nothing.** When a field
   or a body changed differently on both sides, the item file keeps one
   value (so it stays valid and the board keeps working) and
   `conflicts/<STEM>.json` records both, with each side's author and date:

   ```json
   {
     "id": "TODO:88",
     "sides": [{"author": "Özgür", "date": "2026-10-14T09:12:00+01:00"},
               {"author": "Işıl", "date": "2026-10-14T09:15:00+01:00"}],
     "fields": [{"field": "status", "values": ["done", "wontfix"]}],
     "body": {"values": ["...", "..."]},
     "documents": [{"path": "items/TODO-88.spec.md", "other": "items/TODO-88.spec.1a2b3c4.md"}]
   }
   ```

   The first value is the one in the file. `conflicts/` is tracked, so the
   conflict syncs to everyone and anyone can resolve it. The board marks the
   row; the item page shows each conflicting field with both values and
   "keep" buttons (`POST /api/resolve {id, field, keep}`); `primum list`
   marks the item and `primum resolve ID` asks per field on a terminal, or
   takes `--field F --keep 1|2`. Resolving writes the chosen value, removes
   the entry (the file when it is empty) and commits `TODO:88: resolve
   status`. A document conflict is resolved by keeping one copy (the other
   is deleted) or both. BOARD.md lists conflicted items at the top. A kind
   change renames the item's conflict file; a delete removes it.

## Shape

| module | holds |
|---|---|
| `history` | the bare repository: open or create, commit, outside edits, log, diff, remote config, fetch, push, clone; every `git` call |
| `merge` | the three-way merge of one item: pure, no git, no files |
| `conflicts` | `conflicts/<STEM>.json`: read, write, resolve |
| `sync` | fetch, renumber, merge with `merge`, settle, push, retry |
| `store` | each change ends in a commit; `move_item` (renumber or change kind) shared by `set_kind` and `sync` |
| `cli`, `server` | `log`, `remote`, `clone`, `sync`, `resolve`; history and resolve routes; the History tab and conflict panel |

No new dependency.

## Testing

Real `git`, in temporary folders; a remote is a local bare repository, two
people are two clones of it. History: one commit per change with its
message; an outside edit committed apart before the next change; the lock
and other files never tracked; `start history` on a project that had none;
`PRIMUM_GIT=/nonexistent` leaves every command working. Merge (pure): one
side's change, both sides' changes to different fields, the same change on
both, set merges of `needs`, a status conflict, a body conflict.
Sync: two clones changing different items, different fields of one item
(adjacent lines), the same id added on both sides (renumbered, `needs`
rewritten), a delete against an edit, a field conflict recorded on both
machines and resolved on one, an offline remote (the change is kept, the
command succeeds), a push race retried. Playwright: the History tab, and a
conflict resolved from the item page.

## Not in scope

Hosting remotes, authentication beyond what `git` already has, real-time
updates (the board polls), merging `primum.toml` changes beyond git's own
merge, and history for documents outside `items/`.
