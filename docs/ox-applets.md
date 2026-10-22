# Applets as Ox plugins

The applets removed on 2026-10-21 come back one at a time as bundled Ox plugins, after weather (see [ox-embedding.md](ox-embedding.md)). Each port adds the editor and library APIs it needs, so later plugins find them ready.

## Decisions

- **Order.** Dired first: it needs the APIs most plugins want next (files, prompts, list buffers, hooks). Markdown waits until core can lend a plugin its grammar and views can show rendered Markdown.
- **List buffers.** Listings such as dired, and later git status and IRC, are styled text buffers: lines of text with faces, so movement, `C-s`, the region and copying work as in any buffer. Views stay for dashboards such as weather.
- **Prompts.** `(await (prompt "Rename to: " :initial name))` and `(await (yes-or-no "Delete 3 files?"))` wait in a task while the minibuffer is open; `C-g` makes the await raise "Quit".
- **Commands run as tasks.** A command's function runs in its own task, so it can await a prompt or a fetch without holding up typing. Its error goes to `*ox log*` when it finishes.
- **Files.** `ox file` is asynchronous like the rest of the I/O; `file/trash` uses the `trash` crate.

## Slices

1. **List, read, write, rename, copy, remove and trash files, and work with paths** (`ox file`, `ox path`).
2. **Show plugin text buffers with styled lines** and move between their lines.
3. **Prompt for text and yes-or-no answers,** and run commands as tasks.
4. **Hook into the editor:** opening a directory, opening files, opening externally, disk changes and the start screen.
5. **Port dired:** listings with sorting and hidden details, marks and flags, delete, rename, copy, new directories, and refresh.
