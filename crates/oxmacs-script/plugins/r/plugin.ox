(plugin r
  :version "0.20.14"
  :summary "R consoles, plots, data and help in the style of ESS and RStudio"
  :commands [run-r r-new-session r-send-statement r-send-function r-switch-to-console r-help
             r-environment r-peek r-layout r-layout-restore r-load-all r-document r-check
             r-install r-build r-test r-packages r-complete r-indent-or-complete r-find-definition
             r-insert-assignment r-insert-pipe r-markdown-run-chunk r-markdown-run-all
             r-markdown-run-above r-markdown-next-chunk r-markdown-previous-chunk
             r-markdown-send-statement r-markdown-insert-chunk r-markdown-outline
             r-markdown-comment r-markdown-render r-markdown-preview r-run-job r-run-app
             r-debug-next r-debug-step r-debug-finish r-debug-continue r-debug-stop
             r-debug-function r-undebug-function r-renv-status r-renv-snapshot r-renv-restore
             r-renv-init]
  :modes [{title: "R" grammar: "r" extensions: ["R" "r"] file-names: [".Rprofile"] comment: "#"
           roots: ["DESCRIPTION" "renv.lock" "*.Rproj"]}
          {title: "R Markdown" extensions: ["Rmd" "rmd" "qmd"] roots: ["DESCRIPTION" "renv.lock" "*.Rproj"]}]
  :mode-keys {"R" {"M--" r-insert-assignment
                   "C-S-m" r-insert-pipe
                   "C-RET" r-send-statement
                   "C-c C-c" r-send-function
                   "C-c C-z" r-switch-to-console
                   "C-c C-v" r-help
                   "C-c C-e" r-environment
                   "C-c C-o" r-peek
                   "C-c C-f" format-buffer
                   "M-." r-find-definition
                   "TAB" r-indent-or-complete
                   "C-M-i" r-complete
                   "C-c C-w l" r-load-all "C-S-l" r-load-all
                   "C-c C-w d" r-document "C-S-d" r-document
                   "C-c C-w c" r-check "C-S-e" r-check
                   "C-c C-w i" r-install "C-S-b" r-install
                   "C-c C-w b" r-build
                   "C-c C-w t" r-test "C-S-t" r-test
                   "C-c C-t d" r-debug-function "C-c C-t u" r-undebug-function
                   "<f10>" r-debug-next "S-<f4>" r-debug-step "S-<f6>" r-debug-finish
                   "S-<f5>" r-debug-continue "S-<f8>" r-debug-stop}
              "R Markdown" {"C-c C-c" r-markdown-run-chunk "C-S-RET" r-markdown-run-chunk
                            "C-RET" r-markdown-send-statement
                            "C-c C-a" r-markdown-run-all
                            "C-c C-r" r-markdown-run-above
                            "M-n" r-markdown-next-chunk "M-p" r-markdown-previous-chunk
                            "C-c C-i" r-markdown-insert-chunk
                            "M-g i" r-markdown-outline
                            "M-;" r-markdown-comment
                            "C-c C-k" r-markdown-render "C-S-k" r-markdown-render
                            "C-c C-z" r-switch-to-console
                            "C-c C-v" r-help
                            "C-c C-e" r-environment
                            "C-c C-o" r-peek
                            "M--" r-insert-assignment
                            "C-S-m" r-insert-pipe}}
  :settings {program: {type: :text doc: "The R to run, such as Rterm or a full path; unset finds R on PATH, in R_HOME or where R installs itself"}
             air-program: {type: :text doc: "The air that formats R; unset uses the one M-x list-tools installs, or air on PATH"}
             render-on-save: {type: :toggle default: false doc: "Render R Markdown and Quarto documents each time they are saved"}
             language-server: {type: :toggle default: true doc: "Run R's languageserver package for diagnostics, completion and definitions when it is installed"}
             completion-delay: {type: :number default: 0.25 doc: "Seconds of rest after typing before R completion pops up on its own"}
             signature-delay: {type: :number default: 0.4 doc: "Seconds of rest before the arguments of the call at point show in the echo area"}}
  :tools [{name: "air" repository: "posit-dev/air" summary: "R formatter" group: "R" tag-prefix: "v"
           checksum: :sidecar setting: :air-program
           archives: {windows: {x86_64: "air-x86_64-pc-windows-msvc.zip" aarch64: "air-aarch64-pc-windows-msvc.zip"}
                      macos: {x86_64: "air-x86_64-apple-darwin.tar.gz" aarch64: "air-aarch64-apple-darwin.tar.gz"}
                      linux: {x86_64: "air-x86_64-unknown-linux-gnu.tar.gz" aarch64: "air-aarch64-unknown-linux-gnu.tar.gz"}}}]
  :requires {markdown: "^0.2.9"}
  :wakes-for ["R"]
  :opt-in true
  :entry "r.ox")
