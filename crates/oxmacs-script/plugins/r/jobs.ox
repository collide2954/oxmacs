(module
  (import (ox path) (ox file) (ox regex)
          (r session :as session)
          (r language :as language))
  (export start rscript heard-app))

(def (start buffer program args directory {on-line nil on-exit nil title nil})
  "Runs program with args in directory as a background job whose output shows in buffer, calling on-line with each line and on-exit with its status."
  (run-job buffer program args :cwd directory :title title :on-line on-line :on-exit on-exit))

(def (rscript)
  (when-let [program (session/find-r)]
    (let [script (language/rscript program)]
      (if (await (file/exists? script))
        script
        (do (message $"Rscript was not found beside {program}") nil)))))

(defcommand r-run-job "Run the R script in a background job, apart from the session"
  (let [file (buffer-file)]
    (if (and file (= (buffer-language) "R"))
      (when-let [script (rscript)]
        (run-command 'save-buffer)
        (start $"*R job: {(path/name file)}*" script [file] (or (session/project-here) (path/parent file))
               :title $"Rscript {(path/name file)}"))
      (message "Run a saved R script as a job"))))

(def (app? folder)
  (or (await (file/exists? (path/join folder "app.R")))
      (and (await (file/exists? (path/join folder "ui.R")))
           (await (file/exists? (path/join folder "server.R"))))))

(def (app-folder)
  (let [here (when-let [file (buffer-file)] (path/parent file))
        project (session/project-here)]
    (cond ((and here (app? here)) here)
      ((and project (app? project)) project)
      (else nil))))

(def (heard-app line)
  (when-let [hit (regex/find #"Listening on (https?://\S+)" line)]
    (open-externally (first hit.groups))
    (message $"Opened {(first hit.groups)}")))

(defcommand r-run-app "Run the Shiny app of this folder in a background job and open it"
  (if-let [folder (app-folder)]
    (when-let [script (rscript)]
      (start $"*R app: {(path/name folder)}*" script
             ["-e" $"options(shiny.autoreload = TRUE); shiny::runApp({(session/r-string folder)}, launch.browser = FALSE)"]
             folder :on-line heard-app :title $"Shiny app in {folder}"))
    (message "No Shiny app here: it needs app.R, or ui.R and server.R")))
