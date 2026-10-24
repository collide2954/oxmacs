(plugin r
  :version "0.17.0"
  :summary "R consoles, plots, data and help in the style of ESS and RStudio"
  :commands {run-r: "Start an R console beside the script"
             r-new-session: "Start another R session beside the current one"
             r-send-statement: "Send the statement or selection to R and step"
             r-send-function: "Send the function or paragraph to R and step"
             r-switch-to-console: "Switch between the R script and its R session"
             r-help: "Show R's help on a topic in *R help*"
             r-environment: "List the objects in the R session"
             r-peek: "Show the structure of the R object at point in the echo area"
             r-layout: "Arrange the windows like RStudio: script and console, environment and plots"
             r-layout-restore: "Put the windows back as they were before r-layout"
             r-load-all: "Load the R package of this project from its source"
             r-document: "Write the documentation of the R package from its roxygen comments"
             r-check: "Check the R package of this project"
             r-install: "Install the R package of this project"
             r-build: "Build the R package of this project into a source archive"
             r-test: "Run the tests of the R package and list what passed and failed"
             r-packages: "List the R packages installed, with those attached marked"
             r-complete: "Complete the R code before point from the session"
             r-indent-or-complete: "Indent the line, or complete the R code once it is indented"
             r-find-definition: "Jump to the definition of the R name at point"
             r-insert-assignment: "Insert the assignment arrow <-"
             r-insert-pipe: "Insert the native pipe |>"
             r-markdown-run-chunk: "Run the R chunk at point"
             r-markdown-run-all: "Run every R chunk of the document"
             r-markdown-run-above: "Run the R chunks above point"
             r-markdown-next-chunk: "Move to the code of the next chunk"
             r-markdown-previous-chunk: "Move to the code of the previous chunk"
             r-markdown-send-statement: "Send the R statement or selection to R and step"
             r-markdown-insert-chunk: "Insert an R chunk and put point in it"
             r-markdown-outline: "Jump to a heading or chunk of the document"
             r-markdown-render: "Save the document, render it with R Markdown or Quarto and open the result"
             r-run-job: "Run the R script in a background job, apart from the session"
             r-run-app: "Run the Shiny app of this folder in a background job and open it"
             r-job-stop: "Stop the job of this buffer"
             r-job-again: "Run the job of this buffer again"
             r-debug-next: "Run the next line of the code R is debugging"
             r-debug-step: "Step into the function called on the line R is debugging"
             r-debug-finish: "Finish the function or loop R is debugging"
             r-debug-continue: "Continue running the code R is debugging"
             r-debug-stop: "Stop debugging and go back to R's prompt"
             r-debug-function: "Debug the function at point the next time it runs"
             r-undebug-function: "Stop debugging the function at point"
             r-renv-status: "Show how the project's library differs from renv.lock"
             r-renv-snapshot: "Record the project's library in renv.lock"
             r-renv-restore: "Install the packages renv.lock records into the project's library"
             r-renv-init: "Give the project a library of its own with renv"}
  :modes [{name: "R Markdown" extensions: ["Rmd" "rmd" "qmd"]}]
  :mode-keys {"R" {"M--" r-insert-assignment
                   "C-S-m" r-insert-pipe
                   "C-RET" r-send-statement
                   "C-c C-c" r-send-function
                   "C-c C-z" r-switch-to-console
                   "C-c C-v" r-help
                   "C-c C-e" r-environment
                   "C-c C-o" r-peek
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
                            "C-c C-k" r-markdown-render "C-S-k" r-markdown-render
                            "C-c C-z" r-switch-to-console
                            "C-c C-v" r-help
                            "C-c C-e" r-environment
                            "C-c C-o" r-peek
                            "M--" r-insert-assignment
                            "C-S-m" r-insert-pipe}}
  :settings {program: {type: :string doc: "The R to run, such as Rterm or a full path; unset finds R on PATH, in R_HOME or where R installs itself"}
             render-on-save: {type: :bool default: false doc: "Render R Markdown and Quarto documents each time they are saved"}
             language-server: {type: :bool default: true doc: "Run R's languageserver package for diagnostics, completion and definitions when it is installed"}
             completion-delay: {type: :number default: 0.25 doc: "Seconds of rest after typing before R completion pops up on its own"}
             signature-delay: {type: :number default: 0.4 doc: "Seconds of rest before the arguments of the call at point show in the echo area"}}
  :wakes-for ["R"]
  :opt-in true
  :entry "r.ox")
