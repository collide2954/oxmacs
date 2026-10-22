# Applets as Ox plugins

The applets removed on 2026-10-21 come back one at a time as bundled Ox plugins, after weather (see [ox-embedding.md](ox-embedding.md)). Each port adds the editor and library APIs it needs, so later plugins find them ready.

## Decisions

- **Order.** Dired first: it needs the APIs most plugins want next (files, prompts, list buffers, hooks). Markdown waits until core can lend a plugin its grammar and views can show rendered Markdown.
- **List buffers.** Listings such as dired, and later git status and IRC, are styled text buffers: lines of text with faces, so movement, `C-s`, the region and copying work as in any buffer. Views stay for dashboards such as weather.
- **Prompts.** `(await (prompt "Rename to: " :initial name))` and `(await (yes-or-no "Delete 3 files?"))` wait in a task while the minibuffer is open; `C-g` makes the await raise "Quit".
- **Commands run as tasks.** A command's function runs in its own task, so it can await a prompt or a fetch without holding up typing. Its error goes to `*ox log*` when it finishes.
- **Files.** `ox file` is asynchronous like the rest of the I/O; `file/trash` uses the `trash` crate.
- **Hooks.** A manifest can bind global keys (`:keys {"C-x d" dired}`), add start-screen lines (`:start-screen {dired: "Browse a directory"}`) and claim opening directories (`:opens-directories true`); each wakes the plugin lazily. `(on-open-directory f)` receives the path, `(on-disk-change :key f)` runs after the editor notices changes on disk and `(on-saved :key f)` after a file is saved, once per key.
- **Plugin buffers keep their names.** Core names file and directory buffers after their paths, but a plugin finds its buffers by name, so plugin buffers never get renamed.

- **Git reads and writes with the CLI.** Git runs through `ox process`, so the plugin is pure Ox over porcelain output; `git status` stays fast with git's own caches, and gix leaves the build. `process/run` resolves `{status stdout stderr}` whatever the exit status, and only a missing program raises.
- **Git's diff is text.** The diff beside the status is a styled text buffer, unified or side by side, with a faint line-number gutter, Added and Removed backgrounds and word highlights, so search, the region and copying work and the region picks lines to stage.
- **Transients come from Ox.** A menu is data with switches and functions; the chosen function runs as a task with the switches that are on.
- **Git is on.** Like dired it costs nothing until `C-x g`; the repository mode line is a `mode-line` setting, off by default, that wakes it at startup.
- **Plugins define modes.** `(defmode git-commit "Git commit" :file-names ["COMMIT_EDITMSG"] :comment "#" :keys {…} :highlight f)` adds a file mode with its own keys. After each edit Ox highlights the buffer's lines into a cache, and drawing only reads the cache, so drawing never runs Ox. Markdown and other language plugins build on it. A manifest names its modes, `:modes [{name: "Git commit" file-names: ["COMMIT_EDITMSG"] comment: "#"}]`, so opening such a file wakes the plugin.
- **Plugin buffers follow point and width.** `open-buffer` takes `:beside` (shown in another window, keeping focus), `:on-move` (called with the line when point moves to another one) and `:on-resize` (called with the new width), so the diff follows the status and side-by-side diffs refit.
- **Tasks share state safely.** Tasks are switched in the middle of Ox code, so `swap!` retries with `compare-and-set!` until no other task changed the atom meanwhile; a plugin updates only the fields it owns, never writing back a whole state it read before an `await`.

## Dired

The second bundled plugin lives in `crates/oxmacs-ox/plugins/dired`: `plugin.ox`, the entry `dired.ox` (listings in a `defonce` atom keyed by directory, the listing keys, commands and file operations) and `listing.ox` (sizes, dates, sorting and rendering, pure functions of a listing). Listings are text buffers that watch their directory, so they follow changes on disk. Its tests drive a real temporary folder with keys, ticking until Ox is idle or waiting on a prompt.

## Git

The third bundled plugin lives in `crates/oxmacs-ox/plugins/git`: `plugin.ox`, the entry `git.ox` (status state in a `defonce` atom keyed by buffer, keys, folding, visiting, staging and refresh), `repo.ox` (running git and reading `git status --porcelain=v2`, numstat, stashes and the log), `diff.ox` (hunks from unified diffs) and `status.ox` (rendering, a pure function of the snapshot and folds). Reads run their git commands at the same time with `--no-optional-locks`, so a refresh never holds the index lock. `pane.ox` draws the diff beside the status, unified or side by side, with changed words picked out by a token LCS, and `commit.ox` holds the message template, recall and highlighting. Hunks and the region's lines are staged by piping a patch built from the diff to `git apply --cached`, and commits pass the message to `git commit -F -`. Its tests drive real repositories made in temporary folders.

## Slices

Slices 1 to 11 are done.

1. **List, read, write, rename, copy, remove and trash files, and work with paths** (`ox file`, `ox path`).
2. **Show plugin text buffers with styled lines** and move between their lines.
3. **Prompt for text and yes-or-no answers,** and run commands as tasks.
4. **Hook into the editor:** opening a directory, opening files, opening externally, disk changes and the start screen.
5. **Port dired:** listings with sorting and hidden details, marks and flags, delete, rename, copy, new directories, and refresh.
6. **Run programs** (`ox process`): `(await (process/run "git" ["status"] :cwd root :env {…} :input text))`, and `process/env`.
7. **Git status** on `C-x g`: Head and Upstream, untracked, unstaged and staged files with status letters and +/− counts, stashes and recent commits. `n` and `p` move, `TAB` folds sections and opens a file's hunks in place, `RET` visits the file at the line shown, `g` refreshes, and it follows `.git` and saved files.
8. **Stage, unstage and discard** files and sections with `s`, `u` and `k`, asking before discarding and trashing untracked files.
9. **The diff beside the status:** unified with line numbers, `t` for side by side with word highlights, and `s`, `u` and `k` on hunks and the region's lines.
10. **Transients from Ox,** with switches and functions.
11. **Commit:** the `c` transient, the message buffer (`C-c C-c`, `C-c C-k`, `M-p`, `M-n`, the subject length) and the staged diff beside it.
12. **The repository mode line** (`main ↑2 ↓0 ●`) and the `$` process buffer.
13. **The log:** commits with a lane graph, labels, authors and dates, the commit below and its diff beside.
14. **Branches:** a filterable tree and the `b` transient.

Later: blame, margin markers, file history and compare; push, pull and fetch; stash, cherry-pick, revert, tags, worktrees and bisect; the merge tool and interactive rebase.
