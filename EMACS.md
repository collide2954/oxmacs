# oxmacs and GNU Emacs

oxmacs is for people who love how Emacs works and want it on a modern foundation. It keeps the Emacs way of editing and rebuilds everything underneath: a native Rust core, a GPU-drawn display, and Ox, an extension language designed this decade. It doesn't run Emacs Lisp or try to copy every package. This page gives the general picture of where the two differ; the Ox manual inside oxmacs (`C-h i`) has the details.

## At a glance

| | GNU Emacs | oxmacs |
| --- | --- | --- |
| Core | C, with much of the editor in Emacs Lisp | Rust |
| Display | its own redisplay, in a terminal or a GUI toolkit | drawn on the GPU with Iced, with a software fallback |
| Extension language | Emacs Lisp | Ox, a modern Scheme |
| Waiting on files, programs and the network | one Lisp thread: blocking calls, or callbacks and timers | tasks: a call that waits pauses only its task, never the editor |
| Configuration | `init.el`, Customize | `init.ox`, `M-x customize`, typed settings |
| Packages | package.el, ELPA and MELPA | bundled Ox plugins with versioned manifests, and your own |
| Platforms | nearly everywhere, terminals included | Windows, Linux and macOS as equals, as a graphical app |
| Age | since 1985 | new |

## What stays the same

If your fingers know Emacs, they know oxmacs. `C-x C-f`, `C-x b`, `C-x 2`, `M-x`, `C-g`, the minibuffer with completion, buffers and windows, the kill ring, the mark and region, incremental search, query-replace, keyboard macros on `F3` and `F4`, dired, and special buffers with their own keys all work the way you expect. `C-h t` opens a hands-on tutorial and `C-h i` the manual.

## Where oxmacs differs

**A native core.** Buffers, the display, language servers, processes and file watching are Rust. Only the features built on top are Ox, so the hot paths never run in an interpreter, and oxmacs is one binary that behaves the same on all three systems. Windows is a first-class home rather than a port: programs start without console windows, stopping one stops everything it started, and WSL distributions are places to work like any folder.

**A modern display.** oxmacs draws on the GPU: smooth scrolling and cursor movement, images inline, PDFs and EPUB books as pages, and plugin views with real buttons, sliders and canvases. Suggestions can appear in grey right at point across several lines, with the text below moving aside. Emacs draws a grid of text and places images into it; oxmacs draws its text and its widgets the same way.

**More comes in the box.** Language servers, a git status buffer in the style of Magit, remote files over ssh, sftp and ftp, IRC, calendars with CalDAV, an encrypted password vault, PDF and EPUB readers, R consoles and plots, WSL and pixi environments, and an assistant built on Claude all ship with oxmacs as plugins, most of them opt-in with `(use-plugin name)`. They share one set of conventions instead of each bringing its own.

**Settings you can read.** Every setting is declared with a type and a description, so `M-x customize` shows them as a form and a wrong value is caught with a message instead of failing later. Your configuration is one `settings` form in `init.ox`, and saving `init.ox` applies it at once.

**It never freezes.** Ox runs in short slices between keystrokes. Reading a file, running a program, calling a web service or waiting for an answer in the minibuffer pauses only the code that asked, so the editor keeps responding while plugins work.

## Emacs Lisp and Ox

Both are Lisps, and both let you change the editor while it runs. Emacs Lisp carries forty years of history; Ox started from what Lisps have learned since.

**Modules instead of one namespace.** Emacs Lisp has a single global namespace, kept apart by prefixes like `magit-` and `org-`. Ox has modules with imports and exports, so `(import (ox string))` gives you `string/trim`, and two plugins can't step on each other's names. Ox scope is lexical everywhere; in Emacs Lisp, older code and files without a `lexical-binding` cookie still bind dynamically.

**Data that doesn't change under you.** Emacs Lisp works with mutable lists, vectors and hash tables, association and property lists, and a `nil` that is both false and the empty list. Ox has vectors `[1 2]`, maps `{name: "Ada"}`, sets, keywords and records, all immutable and cheap to update because new values share structure with old ones. `true`, `false` and `nil` are distinct, atoms hold the state that changes, and destructuring and `match` take values apart.

**Strings and regexes that read like what they mean.** An Emacs Lisp regex is a string with doubled backslashes, such as `"\\(foo\\|bar\\)"`. Ox writes `#"(foo|bar)"` and checks the pattern before the code runs. `$"Hello, {name}"` fills in values, and `"""` blocks keep text exactly as written.

**Waiting written straight down.** In Emacs Lisp, work that waits on the network or another program means callbacks, process filters and sentinels, or blocking the editor. In Ox, calls that wait simply return their result and pause only their task, and `spawn`, promises, channels and generators cover the rest. `worker` runs heavy computation on a thread of its own.

Here is a command that asks for a city and shows its weather. In Emacs Lisp, the answer arrives in a callback:

```elisp
(defun my-weather (city)
  (interactive "sCity: ")
  (url-retrieve
   (format "https://wttr.in/%s?format=3" (url-hexify-string city))
   (lambda (_status)
     (goto-char url-http-end-of-headers)
     (message "%s" (string-trim (buffer-substring (point) (point-max)))))))
```

In Ox, the same command reads top to bottom, and the editor stays responsive while it waits:

```clojure
(module (import (ox http) (ox string)))

(defcommand my-weather "Show the weather for a city"
  (let [city (prompt "City: ")
        reply (http/get $"https://wttr.in/{(string/url-encode city)}?format=3")]
    (message (string/trim reply.body))))
```

**Errors with types.** Ox errors are typed: `deferror` declares your own, and `try` handles them with `catch` and `finally`. `C-g` raises a quit of its own kind, so a `catch` meant for errors never swallows it.

**Tools in one command.** The `ox` command runs Ox outside the editor too, so Ox works as a scripting language as well as an extension language. `ox check` finds unknown names, wrong argument counts and unused variables, and oxmacs runs the same checks as you type. `ox fmt` formats in one house style, `ox test` runs `test` blocks kept next to the code they test, and `ox profile` and `ox perf` find where time goes. `ox migrate` rewrites your code when Ox changes, and `ox lsp` brings Ox to VS Code, Helix and Neovim.

**Plugins with manifests.** A plugin declares its version, the plugins it requires and their versions, its commands, keys, modes and typed settings in one `plugin.ox`. oxmacs loads it the first time you use it, and a plugin can lend its modules to others as a library.

## Where Emacs is still ahead

- **Age and ecosystem.** Decades of packages, documentation and community answers, Org mode among them. oxmacs is new and its plugin list is short by comparison.
- **Terminals.** Emacs runs in a terminal and over a plain ssh session; oxmacs is a graphical app.
- **Your existing setup.** oxmacs runs no Emacs Lisp, so an `init.el` doesn't carry over. Most settings are a line or two of Ox, and the keys come with you.

## Trying it

The [README](README.md) shows how to build oxmacs or install it on Fedora. Once it's open, `C-h t` walks you through the basics, `M-x find-init-file` opens your configuration, and `C-h i` has the Ox manual.
