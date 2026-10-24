(module
  (import (ox string) (ox path) (ox file)
          (r session :as session))
  (export available tested installed))

(def tests-buffer "*R tests*")
(def packages-buffer "*R packages*")
(def packages-help "RET attaches or detaches   h shows its help   g refreshes   q quits")

(defonce known (atom {}))
(defonce results (atom {root: nil rows: []}))
(defonce listing (atom {session: nil rows: []}))

(def (available session payload)
  (swap! known assoc session (set (filter (fn [name] (not (string/blank? name))) (string/split payload "\t")))))

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

(def test-keys {"RET" 'r-test-visit "n" 'next-line "p" 'previous-line "g" 'r-test "q" 'quit-window})

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
        files (distinct (map (fn [row] row.file) rows))
        laid (reduce (fn [built file]
                       (let [members (filter (fn [row] (= row.file file)) rows)
                             heading (conj built.lines [[file :heading]])]
                         (reduce (fn [done row]
                                   (let [[mark face] (or (get marks row.status) ["?" :warning])]
                                     {lines: (conj done.lines (if (string/blank? row.message)
                                                                ["  " [mark face] " " row.test]
                                                                ["  " [mark face] " " row.test "  " [row.message :comment]]))
                                      places: (conj done.places [(count done.lines) row])}))
                                 {lines: heading places: built.places}
                                 members)))
                     {lines: [[["{(tally "passed")} passed, {(tally "failed")} failed, {(tally "skipped")} skipped" :heading]] []]
                      places: []}
                     files)]
    (reset! results {root: root places: laid.places})
    (open-buffer tests-buffer laid.lines :keys test-keys :beside true :select true :line 0)))

(defcommand r-test-visit "Visit the test on this line"
  (let [found (find (fn [[line _]] (= line (current-line))) (get @results :places))]
    (if found
      (let [row (second found)
            file (path/join (get @results :root) "tests" "testthat" row.file)]
        (when (open-file file)
          (goto-line (max 0 (- (or row.line 1) 1)))))
      (message "No test on this line"))))

(def package-keys {"RET" 'r-package-toggle "h" 'r-package-help "g" 'r-packages "q" 'quit-window
                   "n" 'next-line "p" 'previous-line})

(defcommand r-packages "List the R packages installed, with those attached marked"
  (when-let [running (session/ensure)]
    (swap! listing assoc :session running)
    (session/request running "getOption(\"oxmacs\")$packages()")))

(def (installed session payload)
  (let [rows (keep (fn [line]
                     (let [fields (string/split line "\t")]
                       (when (>= (count fields) 3)
                         {name: (nth fields 0) version: (nth fields 1) attached: (= (nth fields 2) "TRUE")})))
                   (string/lines payload))
        width (apply max 0 (map (fn [row] (count row.name)) rows))]
    (reset! listing {session: session rows: rows})
    (open-buffer packages-buffer
                 (concat [[[packages-help :comment]] []]
                         (map (fn [row] [(if row.attached "* " "  ")
                                         [(string/pad-end row.name width) (if row.attached :keyword :link)]
                                         "  " [row.version :comment]])
                              rows))
                 :keys package-keys :beside true :select true)))

(def (package-here)
  (let [at (- (current-line) 2)
        rows (get @listing :rows)]
    (when (and (>= at 0) (< at (count rows)))
      (nth rows at))))

(defcommand r-package-toggle "Attach or detach the package on this line"
  (if-let [row (package-here)]
    (let [running (get @listing :session)]
      (session/show-console running false)
      (session/evaluate running (if row.attached
                                  "detach({(session/r-string (str "package:" row.name))})"
                                  "library({row.name})"))
      (swap! listing update :rows (fn [rows] (map (fn [other] (if (= other.name row.name) (assoc other :attached (not row.attached)) other)) rows))))
    (message "No package on this line")))

(defcommand r-package-help "Show the help index of the package on this line"
  (if-let [row (package-here)]
    (session/request (get @listing :session) "getOption(\"oxmacs\")$help_index({(session/r-string row.name)})")
    (message "No package on this line")))
