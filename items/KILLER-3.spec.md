# GitHub Issues: two-way sync, optional

**Goal.** A project can be connected to a GitHub repository, and from then
on its items and that repository's issues are the same work, kept in step
in both directions without anyone copying anything: an item added, edited,
started, finished or moved in primum changes its issue, and an issue opened,
edited, labelled, closed or reopened on GitHub changes its item. GitHub's
closing keywords work as they always do: a pull request saying `Fixes #123`
closes the issue when it merges, and the item becomes `done`. Not connecting
changes nothing.

**Builds on KILLER:2** (history and sync): its three-way item merge, its
conflict records and its automatic sync. GitHub is treated as one more side
of the same merge.

## Decisions

1. **Optional, per project.** `primum github connect owner/repo` writes

   ```toml
   [github]
   repo = "owner/repo"
   ```

   into `primum.toml`, shared with everyone using the board.
   `primum github disconnect` removes it; links already in item files stay,
   inert. Without `[github]` nothing here runs.

2. **primum talks to GitHub through `gh`.** Like `git` in KILLER:2: no HTTP
   client in the crate, and authentication is each person's own `gh auth
   login` (or `GH_TOKEN`), which primum never reads or stores. `PRIMUM_GH`
   names another `gh` (tests). Without `gh`, or logged out, the GitHub part
   of a sync is skipped with `primum: GitHub sync is off: <reason>`; the
   board works as before.

3. **An item and its issue.**

   | primum | GitHub |
   |---|---|
   | the link | `github: 123` in the item's front matter (written only when set) |
   | `title` | title |
   | `body` | the issue body, before primum's block |
   | `open` | open |
   | `doing` | open, label `doing` |
   | `done` | closed as completed |
   | `wontfix` | closed as not planned |
   | `finished` | the date it was closed |
   | kind | label `kind:urgent`, `kind:bug`, `kind:debt`, `kind:killer`, `kind:todo`, in the lane colors |
   | `impact`, `urgency`, `risk`, `effort`, `needs`, the id | primum's block at the end of the body |

   primum's block is an HTML comment, invisible on GitHub:

   ```
   <!-- primum
   id: TODO:87
   impact: 7
   urgency: 6
   risk: 2
   effort: 4
   needs: [TODO:64]
   -->
   ```

   Above it, when the item has needs, a visible line `Needs: #45, TODO:64`
   (an issue number where the needed item has one). Documents stay in primum;
   the issue is not their home.

   Scores are edited in primum. A hand edit of the block on GitHub is read
   like any other change; a malformed block is ignored and rewritten.

4. **What syncs, and when.** GitHub sync runs inside every KILLER:2 sync,
   after the history is pulled and before it is pushed, so a link another
   machine wrote is seen before anything is created:
   - **from GitHub**: issues and pull requests updated since the last sync
     (`since=`, one request, paginated). An issue with no item becomes a new
     item (its kind from its `kind:` label, else `TODO`; scores from its
     block, else the defaults); a linked issue that changed is merged into
     its item.
   - **to GitHub**: an item with no link becomes an issue (open items only;
     `primum github connect --all` also files closed ones); a linked item
     that changed updates its issue.

   The last sync time is per machine (`primum.github-since` in the history's
   git config); a machine without one reads everything once.

5. **Merging is KILLER:2's.** For each linked item primum keeps the version
   both sides last agreed on, tracked at `github/<STEM>.json` (so every
   machine shares it). Item, issue and that base go through the same
   `merge_item`: a field changed on one side takes that change; changed on
   both, it is a conflict in `conflicts/<STEM>.json` with the sides
   "this board" and "GitHub (login)", resolved with `primum resolve` or the
   item page, and the conflicting field is not written to GitHub until it is
   resolved. Kind is merged the same way: changed on GitHub only, the item
   moves lanes (`set_kind`); changed on both, a `kind` conflict.

6. **Closing keywords.** GitHub's own (`close`, `closes`, `closed`, `fix`,
   `fixes`, `fixed`, `resolve`, `resolves`, `resolved` before `#123`) need
   nothing from primum: the issue closes as completed and the item follows
   at the next sync. primum adds its ids to them: a merged pull request whose
   title or body says `Fixes TODO:87` closes TODO:87 (and its issue), with a
   note on the item `closed by #<pr>`. The item page and `primum show` show
   the issue as `#123` with a link, ready to paste into a pull request, and
   `#45` and `owner/repo#45` in item text link to GitHub on the board.

7. **Deleting.** Issues are not deleted. Deleting a linked item closes its
   issue as not planned, with a comment `Deleted in primum.`. An issue that
   disappears (deleted by an admin, or the repository changed) leaves its
   item, without its link, the next time primum writes to it (GitHub answers
   404).

8. **No duplicates.** An issue is created only after the history is pulled,
   and primum's block carries the item's id: an issue found with the id of
   an item that has no link (another machine created it in the same minute)
   is linked instead of creating a second one. When two issues carry the
   same id, the newer is closed as a duplicate of the older.

9. **Renumbering and kind changes** keep the link (it is the issue number);
   the block's id and the kind label follow at the next sync.

## Shape

| module | holds |
|---|---|
| `github` | `Issue`, `IssueWrite`, the `Api` trait, `Gh` (shelling out to `gh api`), `Memory` (an in-memory GitHub for tests); the mapping between an item and an issue (`issue_write`, `item_from_issue`, primum's block) |
| `github_sync` | one GitHub sync of one project: fetch, create, merge, push, closing keywords, deletes, duplicates; the `github/` bases |
| `item_file` | the `github` field |
| `projects` | the `[github]` table |
| `sync` | calls `github_sync` between pull and push |
| `cli`, `server`, static | `primum github connect/disconnect/status`; the issue link on the board and item page |

No new dependency. The history allowlist gains `!/github/`.

## Testing

The sync logic runs against `Memory`, an in-memory GitHub: an item becomes
an issue with its labels and block; an issue becomes an item; edits on each
side reach the other; both sides changing different fields merge; both
changing one field is a conflict and that field is not pushed; closing as
completed and as not planned; reopening; a kind label change moves the item;
a merged pull request with `Fixes TODO:2` closes TODO:2; deleting closes the
issue; an issue with the id of an unlinked item is linked, not duplicated;
two issues with one id close the newer. The `gh` wiring is checked once
against a fake `gh` script. Playwright: the issue link on the item page.

## Not in scope

GitHub Projects (boards), milestones, assignees, issue comments as notes,
documents as issues, webhooks (the sync polls), and other forges (GitLab,
Gitea), though `Api` leaves room for them.
