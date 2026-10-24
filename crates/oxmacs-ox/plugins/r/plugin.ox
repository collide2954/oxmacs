(plugin r
  :version "0.6.0"
  :summary "R consoles, plots, data and help in the style of ESS and RStudio"
  :commands {run-r: "Start an R console beside the script"
             r-new-session: "Start another R session beside the current one"
             r-send-statement: "Send the statement or selection to R and step"
             r-send-function: "Send the function or paragraph to R and step"
             r-switch-to-console: "Switch between the R script and its R session"
             r-help: "Show R's help on a topic in *R help*"
             r-environment: "List the objects in the R session"
             r-peek: "Show the structure of the R object at point in the echo area"
             r-complete: "Complete the R code before point from the session"
             r-indent-or-complete: "Indent the line, or complete the R code once it is indented"
             r-find-definition: "Jump to the definition of the R name at point"
             r-insert-assignment: "Insert the assignment arrow <-"
             r-insert-pipe: "Insert the native pipe |>"}
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
                   "C-M-i" r-complete}}
  :settings {program: {type: :string doc: "The R to run, such as Rterm or a full path; unset finds R on PATH, in R_HOME or where R installs itself"}}
  :opt-in true
  :entry "r.ox")
