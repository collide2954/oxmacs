(module
  (import (ox string) (ox path) (ox file) (ox process) (ox regex)
          (r session :as session)
          (r language :as language))
  (export start heard-app))

(def help-line "k stops it   g runs it again   q quits")
(def job-keys {"k" 'r-job-stop "g" 'r-job-again "q" 'quit-window "n" 'next-line "p" 'previous-line})

(defonce jobs (atom {}))
(defonce runs (atom 0))

(def (current? buffer run) (= (get-in @jobs [buffer :run]) run))

(def (pump channel buffer run on-line face)
  (when-let [line (receive channel)]
    (when (current? buffer run)
      (append-lines buffer [(if face [[line face]] [line])])
      (when on-line (on-line line)))
    (pump channel buffer run on-line face)))

(def (finished buffer run status)
  (when (current? buffer run)
    (swap! jobs update buffer assoc :process nil)
    (append-lines buffer [[] (if (= status 0)
                               [["Finished" :success]]
                               [["Finished with status {status}" :error]])])))

(def (start buffer program args directory {on-line nil title nil})
  "Runs program with args in directory as a background job whose output shows in buffer, calling on-line with each line."
  (when-let [old (get-in @jobs [buffer :process])]
    (process/kill old))
  (let [process (try (process/start program args :cwd directory)
                     (catch error e (message e.message) nil))]
    (when process
      (let [run (swap! runs inc)]
        (swap! jobs assoc buffer {process: process run: run program: program args: args
                                  directory: directory on-line: on-line title: title})
        (open-buffer buffer [[[(or title (string/join " " (concat [(path/name program)] args))) :heading]]
                             [[help-line :comment]]
                             []]
                     :keys job-keys :beside true)
        (spawn (pump process.errors buffer run on-line :warning))
        (spawn (do (pump process.lines buffer run on-line nil)
                 (finished buffer run (await process.exit))))
        process))))

(defcommand r-job-stop "Stop the job of this buffer"
  (let [buffer (buffer-name)]
    (if-let [process (get-in @jobs [buffer :process])]
      (do (process/kill process)
        (swap! jobs update buffer merge {process: nil run: nil})
        (append-lines buffer [[] [["Stopped" :warning]]]))
      (message "This job is not running"))))

(defcommand r-job-again "Run the job of this buffer again"
  (if-let [job (get @jobs (buffer-name))]
    (start (buffer-name) job.program job.args job.directory :on-line job.on-line :title job.title)
    (message "No job to run again")))

(def (rscript)
  (when-let [program (session/find-r)]
    (let [script (language/rscript program)]
      (if (await (file/exists? script))
        script
        (do (message "Rscript was not found beside {program}") nil)))))

(defcommand r-run-job "Run the R script in a background job, apart from the session"
  (let [file (buffer-file)]
    (if (and file (= (buffer-language) "R"))
      (when-let [script (rscript)]
        (run-command 'save-buffer)
        (start "*R job: {(path/name file)}*" script [file] (or (session/project-here) (path/parent file))
               :title "Rscript {(path/name file)}"))
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
    (message "Opened {(first hit.groups)}")))

(defcommand r-run-app "Run the Shiny app of this folder in a background job and open it"
  (if-let [folder (app-folder)]
    (when-let [script (rscript)]
      (start "*R app: {(path/name folder)}*" script
             ["-e" "options(shiny.autoreload = TRUE); shiny::runApp({(session/r-string folder)}, launch.browser = FALSE)"]
             folder :on-line heard-app :title "Shiny app in {folder}"))
    (message "No Shiny app here: it needs app.R, or ui.R and server.R")))
