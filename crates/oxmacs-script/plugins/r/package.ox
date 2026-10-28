(module
  (import (ox string) (ox path) (ox file)
          (r session :as session)
          (r language :as language))
  (export available tested installed))

(def tests-buffer "*R tests*")
(def packages-buffer "*R packages*")
(def packages-help "RET attaches or detaches   h shows its help   g refreshes   q quits")

(defonce known (atom {}))
(defonce listing (atom {session: nil rows: []}))

(def (available session payload)
  (let [found (set (filter (fn [name] (not (string/blank? name))) (string/split payload "\t")))]
    (swap! known assoc session found)
    (when (and (contains? found "languageserver") (language/wanted?))
      (when-let [program (session/find-r)]
        (language/define! program)))))

(def (has? session package)
  (let [found (get @known session)]
    (or (nil? found) (contains? found package))))

(def (package-root)
  (when-let [root (session/project-here)]
    (when (await (file/exists? (path/join root "DESCRIPTION")))
      root)))

(def tools
  {load: [["devtools" "devtools::load_all"] ["pkgload" "pkgload::load_all"]]
   document: [["devtools" "devtools::document"] ["roxygen2" "roxygen2::roxygenise"]]
   check: [["devtools" "devtools::check"]]
   install: [["devtools" "devtools::install"]]
   build: [["devtools" "devtools::build"] ["pkgbuild" "pkgbuild::build"]]})

(def actions {load: "Loading" document: "Documenting" check: "Checking" install: "Installing" build: "Building"})

(def (develop action)
  (if-let [root (package-root)]
    (when-let [running (session/ensure)]
      (if-let [[_ call] (find (fn [[package _]] (has? running package)) (get tools action))]
        (do (session/show-console running false)
          (session/evaluate running "{call}({(session/r-string root)})"))
        (message "{(get actions action)} a package needs devtools")))
    (message "This project has no DESCRIPTION, so it is not an R package")))

(defcommand r-load-all "Load the R package of this project from its source" (develop :load))
(defcommand r-document "Write the documentation of the R package from its roxygen comments" (develop :document))
(defcommand r-check "Check the R package of this project" (develop :check))
(defcommand r-install "Install the R package of this project" (develop :install))
(defcommand r-build "Build the R package of this project into a source archive" (develop :build))

(def (renv call)
  (if-let [root (session/project-here)]
    (when-let [running (session/ensure)]
      (if (has? running "renv")
        (do (session/show-console running false)
          (session/evaluate running "renv::{call}(project = {(session/r-string root)})"))
        (message "This needs renv; install it in R with install.packages(\"renv\")")))
    (message "This file is not in an R project")))

(defcommand r-renv-status "Show how the project's library differs from renv.lock" (renv "status"))
(defcommand r-renv-snapshot "Record the project's library in renv.lock" (renv "snapshot"))
(defcommand r-renv-restore "Install the packages renv.lock records into the project's library" (renv "restore"))
(defcommand r-renv-init "Give the project a library of its own with renv" (renv "init"))

(defcommand r-test "Run the tests of the R package and list what passed and failed"
  (if-let [root (package-root)]
    (when-let [running (session/ensure)]
      (if (has? running "testthat")
        (session/request running "getOption(\"oxmacs\")$test({(session/r-string root)})")
        (message "Running tests needs testthat")))
    (message "This project has no DESCRIPTION, so it is not an R package")))

(def marks {"passed" ["✓" :success] "failed" ["✗" :error] "skipped" ["–" :comment]})

(def (tested _ payload)
  (let [lines (string/lines payload)
        root (first lines)
        rows (keep (fn [line]
                     (let [fields (string/split line "\t" 5)]
                       (when (>= (count fields) 3)
                         {file: (nth fields 0) test: (nth fields 1) status: (nth fields 2)
                          line: (string/to-number (or (get fields 3) ""))
                          message: (or (get fields 4) "")})))
                   (rest lines))
        tally (fn [status] (count (filter (fn [row] (= row.status status)) rows)))
        folder (path/join root "tests" "testthat")]
    (show-places tests-buffer
                 (vec (map (fn [row]
                             (let [[mark face] (or (get marks row.status) ["?" :warning])]
                               {file: (path/join folder row.file) line: (or row.line 1)
                                label: mark face: face
                                text: (if (string/blank? row.message) row.test (str row.test "  " row.message))}))
                           rows))
                 :title "{(tally "passed")} passed, {(tally "failed")} failed, {(tally "skipped")} skipped"
                 :root folder
                 :on-refresh (fn [] (run-command 'r-test)))))
(def package-keys {"RET" 'r-package-toggle "h" 'r-package-help "g" 'r-packages "q" 'quit-window
                   "n" 'next-line "p" 'previous-line})

(defcommand r-packages "List the R packages installed, with those attached marked"
  (when-let [running (session/ensure)]
    (swap! listing assoc :session running)
    (session/request running "getOption(\"oxmacs\")$packages()")))

(def (package-lines rows)
  (let [width (apply max 0 (map (fn [row] (count row.name)) rows))]
    (vec (map (fn [row] {line: [(if row.attached "* " "  ")
                                [(string/pad-end row.name width) (if row.attached :keyword :link)]
                                "  " [row.version :comment]]
                         package: row})
              rows))))

(def (installed session payload)
  (let [rows (vec (keep (fn [line]
                          (let [fields (string/split line "\t")]
                            (when (>= (count fields) 3)
                              {name: (nth fields 0) version: (nth fields 1) attached: (= (nth fields 2) "TRUE")})))
                        (string/lines payload)))]
    (reset! listing {session: session rows: rows})
    (open-buffer packages-buffer (package-lines rows) :keys package-keys :beside true :select true)
    (set-header packages-buffer [[[packages-help :comment]]])))

(def (package-here)
  (get (line-item) :package))

(defcommand r-package-toggle "Attach or detach the package on this line"
  (if-let [row (package-here)]
    (let [running (get @listing :session)]
      (session/show-console running false)
      (session/evaluate running (if row.attached
                                  "detach({(session/r-string (str "package:" row.name))})"
                                  "library({row.name})"))
      (swap! listing update :rows (fn [rows] (vec (map (fn [other] (if (= other.name row.name) (assoc other :attached (not row.attached)) other)) rows))))
      (set-lines packages-buffer (package-lines (get @listing :rows))))
    (message "No package on this line")))

(defcommand r-package-help "Show the help index of the package on this line"
  (if-let [row (package-here)]
    (session/request (get @listing :session) "getOption(\"oxmacs\")$help_index({(session/r-string row.name)})")
    (message "No package on this line")))
