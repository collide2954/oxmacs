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

## Screensaver

The fourth bundled plugin lives in `crates/oxmacs-ox/plugins/screensaver`. It is off until you set its `idle` time, `(use-plugin screensaver :config {idle: 300})`: after that many seconds without input, while oxmacs has focus, a screensaver takes over the window area, leaving one mode line and the minibuffer. The first key or click ends it and does nothing else. `kind` picks `:starfield`, `:matrix` or `:random` (the default), and `M-x screensaver-preview` shows either one at any time. The plugin is `plugin.ox`, the entry `screensaver.ox` (the preview command and the idle hook), `noise.ox` (repeatable random numbers from `hash`) and one file per screensaver, each a pure function from the frame to its shapes, so a frame never depends on the one before.

- **Canvas draw lists.** A drawing is a vector of shapes, `[:rect x y width height color]`, `[:circle x y radius color]`, `[:line x1 y1 x2 y2 color width]` and `[:text x y "text" color size]`, in pixels. A color is a face keyword such as `:success`, `:foreground` or `:background`, or `"#rrggbb"`, optionally with an alpha, `[:success 0.4]`. `(canvas shapes :height 120)` puts a drawing in a view.
- **Taking over.** `(take-over draw :background color :on-end f :fps 15)` calls `draw` with `{time width height font-size}` up to `:fps` times a second (30 at most, the default) and draws what it returns over every window; `(end-take-over)` or any input ends it. Drawing happens in the update step, never while the screen paints. The matrix asks for 15 frames a second: its glyphs move a whole row at a time, and building a frame's shapes costs about 9 ms in a release build.
- **Idle.** `(on-idle :key seconds f)` calls `f` once each time oxmacs has had focus and no input for that long. Like `on-disk-change`, a key registered again replaces its handler, so reloading a plugin never doubles it.
- **Choices.** `(await (prompt "Screensaver: " :choices {starfield: "Stars rushing toward you" matrix: "Glyphs raining down the screen"}))` completes from the names and shows each note; a vector of names works too.

Screensaver slices 1 to 4 are done:

1. **Shapes and the takeover in the bridge,** and the `canvas` widget.
2. **Draw the takeover in the GUI** over the windows, keeping a mode line and the minibuffer, with any input ending it.
3. **Idle hooks** that fire once per idle stretch while oxmacs has focus.
4. **The screensaver plugin:** starfield, matrix rain, `M-x screensaver-preview` and the `idle` and `kind` settings.
## Vault

The password manager is the bundled `vault` plugin in `crates/oxmacs-ox/plugins/vault`: `plugin.ox`, the entry `vault.ox` (the `*Vault*` list, the `*Vault entry*` view and the commands), `store.ox` (the sealed file, unlocking, locking and generators), `import.ox` (KeePass, Bitwarden, 1Password and pass) and `words.ox` (the passphrase words).

- **Its own file.** The vault is `~/.oxmacs/vault.json`: a clear header with the format version, the Argon2id salt and cost (64 MiB, 3 passes), and the entries as one XChaCha20-Poly1305 sealed blob. A wrong password fails to open the seal, so no password check is stored. Writes go to `vault.json.new`, copy the old file to `vault.json.bak` and rename, so a crash never leaves half a vault.
- **Unlocked in memory only.** The key and entries live in a `defonce` atom while the vault is unlocked. A watcher locks it after `lock-after` minutes unused (10 by default), and `M-x vault-lock` or `L` locks it at once, closing its buffers.
- **The clipboard.** Passwords and codes go to the system clipboard without entering the kill ring, and are cleared after `clear-clipboard` seconds (30 by default) unless something newer was copied.
- **Entries.** An entry has a title, user name, password, URL, a one-time code secret or `otpauth://` link, and notes. Adding one with an empty password generates 20 letters, digits and symbols; `M-x vault-generate` also makes 7-word passphrases and 6-digit PINs, all from `crypto/random-below`, so no choice is biased.
- **Imports.** `M-x vault-import` reads KeePass `.kdbx` files (through the keepass crate, without the recycle bin), Bitwarden and 1Password CSV exports, and the pass store by running `gpg --decrypt` on each file. Entries already in the vault, by title and user name, are skipped.
- **For other plugins.** `(import (vault store :as vault))` then `(vault/secret "Libera")` returns that entry's password, asking for the master password first when the vault is locked, so IRC and calendar logins never sit in `init.ox`.

## IRC

The IRCv3 client is the bundled `irc` plugin in `crates/oxmacs-ox/plugins/irc`, in the style of ERC: a buffer per network, channel and person with the input at the bottom. `protocol.ox` reads and writes IRC lines (IRCv3 tags and their escapes, CTCP, mIRC formatting, SASL PLAIN, case mapping), `format.ox` turns messages into lines, `client.ox` holds the connections and buffers in a `defonce` atom and handles the server's messages, `logs.ox` keeps and searches logs, and `irc.ox` has the commands. There is no bouncer in it; a bouncer such as soju works as a server.

- **Chat buffers.** `open-buffer` takes `:input "prompt> "` for an input line under the lines, `append-lines` adds lines above it without disturbing what is typed, `take-input` empties it for sending, and `set-input`, `input-text` and `input-point` let a plugin complete in it. `M-p` and `M-n` recall input, `C-a` stops after the prompt, and added lines can't be undone. `:show false` makes a buffer without showing it, for channels joined at connect.
- **Layout.** A faint time, shown only when the minute changes, then the nick right-aligned in a column of `nick-width` (12), blank when the same person goes on within five minutes, then the message filled to the window's width with wrapped lines under the message. The lines are filled when a message arrives and again when the window's width changes, as `erc-fill` does, so search, the region and copying see plain text. Nicks always get the same of eight colors, mIRC bold and colors become faces, links are links, mentions of your nick or a `highlights` word get a background, and a dated line starts each day.
- **Capabilities.** It asks for server-time, message-tags, echo-message, batch, chathistory, sasl, away-notify, account-notify, multi-prefix, extended-join, draft/multiline, draft/read-marker, invite-notify, chghost and setname, and uses what the server grants: history for each channel it joins, the server's time on messages, replies and reactions through message tags, and pastes as one multiline message.
- **Logins.** A network's `:login` names a vault entry, whose user name and password log in with SASL PLAIN; the password never sits in `init.ox`. `M-x irc-setup` asks for the server, nick and account, saves the password in the vault and writes `(use-plugin irc :config {networks: […]})` to `init.ox`.
- **Quiet joins.** Joins, parts, quits and nick changes show only for people who spoke in the last 30 minutes; `C-c C-h` shows them all.
- **Unread and mentions.** A buffer you aren't looking at gets a `new` line where its unread messages start, kept until you leave it again, and the mode line lists buffers with new messages, `!` marking mentions. `C-c C-SPC` goes to the next one, mentions first, and back once there are none. A mention or private message while oxmacs is in the background, or the buffer isn't shown, makes a desktop notification when `notify` is on.
- **Keys.** `RET` sends, or opens the link at point in the lines above; `TAB` completes nicks, recent speakers first, cycling on repeats; `C-c C-n` shows the channel's people beside it, grouped by operators, voiced and everyone, with away people dimmed; `C-c C-r` replies to the message at point or the last one; `C-c C-e` reacts with an emoji; `C-c C-v` fetches the link at point and says its title, or its type and size for an image, only when asked; `C-c C-j` joins, `C-c C-p` parts and `C-c C-b` switches buffers. Commands such as `/join`, `/me`, `/msg`, `/query`, `/nick`, `/topic`, `/whois`, `/away` and `/quote` work in the input.
- **Logs.** With `logs` on, messages go to `irc/logs/<network>/<channel>/<day>.log` in the oxmacs home folder, one line each, and `M-x irc-search` lists the lines holding some text; `RET` visits one.
- **Later.** Sending typing notifications (other people's typing shows in the mode line), images shown inline, and `CAP NEW` while connected.

## Calendar

The calendar is the bundled `calendar` plugin in `crates/oxmacs-ox/plugins/calendar`: `ics.ox` reads and writes iCalendar, `rrule.ox` expands repeating events, `store.ox` keeps the files, `caldav.ox` syncs, `reminders.ox` reminds, and the entry `calendar.ox` has the views and commands.

- **Files.** Each calendar is a folder under `calendar` in the oxmacs home folder with one `.ics` file per event, as vdir and khal keep them, so other tools can read them and CalDAV maps one resource to one file. `calendars` lists them, `{name: "Work" url: "https://dav.example.com/cal/work/" login: "Work calendar"}`, with a Personal one when it is empty.
- **Times.** Times keep their zone: events from elsewhere keep their `TZID`, and events made in oxmacs carry the local zone, so a weekly 9:00 meeting stays at 9:00 across daylight saving changes. All-day events are dates. `(ox time)` gained zones for this, through jiff.
- **Repeats.** `RRULE` with `FREQ` daily, weekly, monthly or yearly, `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` with ordinals such as `-1FR`, `BYMONTHDAY` and `BYMONTH`, less `EXDATE`s, with changed occurrences (`RECURRENCE-ID`) replacing theirs. Dates are counted in whole days, so months without a 31st skip a monthly event on the 31st, as RFC 5545 says. Deleting one occurrence adds an `EXDATE`.
- **Views.** `M-x calendar` is an agenda of the next `days` (14) days: `RET` shows an event, `a` adds, `e` edits, `d` deletes, `f` and `b` move a week, `t` comes back to today. `m` is a month grid of text, with `f`, `b`, `n` and `p` moving the selected day and `<` and `>` the month. `w` is a week drawn on a canvas as a grid of hours, with a line at the current time; `ui/canvas` gained `:relative` for it, so the grid fills the window's width.
- **Reminders.** An event's own `VALARM`s, or `reminder` (10) minutes before, show in the echo area and as a desktop notification, checked every 30 seconds. With `reminders` on, the plugin starts with oxmacs when there are events.
- **Invitations.** `M-x calendar-import` adds the events of an `.ics` file. For an invitation, `a`, `x` and `?` in the event accept, decline or answer maybe for the guest whose address is `email`, and write a `METHOD:REPLY` file beside the event to send to the organizer; oxmacs sends no mail.
- **CalDAV.** `M-x calendar-sync` (or `s`) first sends local changes, `PUT` with `If-None-Match: *` for new events and `If-Match` with the known ETag otherwise, and `DELETE` for removed ones, then asks the server for every event with a `calendar-query` `REPORT` and brings in what changed, moving events the server dropped to the trash. A change made on both sides keeps the server's copy and says so. The login comes from the vault entry `:login` names. `.sync.json` in the calendar's folder remembers ETags and pending changes.

## Profiler

Profiling tools for anyone writing Ox for oxmacs: where the time goes in plugins, init.ox, views, modes and frames. The VM collects; the report in oxmacs is the bundled `profiler` plugin, written in Ox.

- **Samples and call counts.** While the profiler runs, the VM records the Ox call stack at its safe points, every 128 instructions instead of 1024, weighted by the time since the last sample, and counts every call to each Ox function. Calls to natives are timed one by one and show as leaves under their caller. Collection only adds to the node of the current stack, found through a cache keyed by the stack's code; totals and the flat table are worked out when a profile is taken. Profiling slows Ox down about two times, most in code that calls many natives; when it is off it costs one branch per call.
- **Grouped by plugin.** A function belongs to its module, and a module to its plugin (`git/status` to git), so the report opens on how much each plugin, `user` (init.ox and the REPL) and Ox's own library took. A tail call replaces its caller's frame, so a function reached by tail calls shows under the caller that started the chain.
- **Editor timing.** In oxmacs the profile also records how long each Ox tick, takeover frame, view refresh and highlighting pass took, and the report starts with the slowest ones.
- **Entry points.**
  - `M-x ox-profiler-start`, `M-x ox-profiler-stop` and `M-x ox-profiler-report` profile everything Ox does in the editor.
  - `ox profile FILE` runs a file and prints the report.
  - `(profile (fn [] …))` returns the profile as data, `{wall busy samples tree functions ticks report}` with times in milliseconds, and `(profile-report found)` prints it. `profiler-start`, `profiler-stop` and `profiler-snapshot` are the controls underneath.
- **The report.** `*ox profile*` shows a call tree with total and self time, their share and the call count on each line; `TAB` folds, `f` switches to the functions by self time, `RET` visits the function's source and `g` takes a fresh report while profiling continues.
- **Native profiling.** `ox perf` (see [ox-internals.md](ox-internals.md)) samples the Rust side for work on the VM itself.
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
