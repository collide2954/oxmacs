(module
  (import (ox file) (ox path) (ox process) (ox string) (ox crypto) (ox json)
          (r helper :as helper))
  (export label sessions known? current-session running? link! linked own! owner project-of project-here
          windows? arguments locate find-r launch ensure show-console evaluate request send script?
          r-string helper-of on-marker on-ready use-keys!))

(def label "R")
(def missing "R not found; install it from https://cran.r-project.org or set the r plugin's program")

(defonce state (atom {links: {} helpers: {} widths: {} owners: {} launched: 0}))
(defonce handlers (atom {}))
(defonce ready-hooks (atom {}))
(defonce keys (atom {}))

(def (use-keys! bindings) (reset! keys bindings))

(def (on-marker kind f) (swap! handlers assoc kind f))

(def (on-ready key f) (swap! ready-hooks assoc key f))

(def (sessions) (consoles label))

(def (known? name) (some (fn [known] (= known name)) (sessions)))

(def (own! buffer session) (swap! state assoc-in [:owners buffer] session))

(def (owner buffer) (get-in @state [:owners buffer]))

(def (linked script) (get-in @state [:links script]))

(def project-markers ["DESCRIPTION" "renv.lock" ".git"])

(defonce projects (atom {}))

(def (marked? folder at)
  (cond ((>= at (count project-markers)) false)
        ((await (file/exists? (path/join folder (nth project-markers at)))) true)
        (else (marked? folder (+ at 1)))))

(def (r-project? folder)
  (some (fn [entry] (= (path/extension entry.name) "Rproj"))
        (try (await (file/list folder)) (catch error e []))))

(def (project-of folder)
  "The folder of the R project holding folder: the nearest with a DESCRIPTION, renv.lock, .git or .Rproj file below the home folder, or nil."
  (when (and folder (not= folder (path/home)))
    (if (or (marked? folder 0) (r-project? folder))
      folder
      (project-of (path/parent folder)))))

(def (project-here)
  (let [folder (default-directory)]
    (if (contains? @projects folder)
      (get @projects folder)
      (let [found (project-of folder)]
        (swap! projects assoc folder found)
        found))))

(def (current-session)
  (let [here (buffer-name)
        own (if (known? here) here (or (owner here) (linked here)))]
    (if (and own (known? own))
      own
      (let [project (project-here)
            mine (find (fn [console] (= (get-in @state [:roots console]) project)) (sessions))]
        (or mine (when (not project) (first (sessions))))))))

(def (running? session) (and session (console-running? session)))

(def script-languages #{"R" "R Markdown"})

(def (script? name) (contains? script-languages (buffer-language name)))

(def (link! console)
  (when (script? (buffer-name))
    (swap! state assoc-in [:links (buffer-name)] console)))

(def (helper-of session) (get-in @state [:helpers session]))

(def (windows?) (= (process/os) :windows))

(def (arguments)
  (if (windows?)
    ["--ess" "--no-save" "--no-restore" "--quiet"]
    ["--no-save" "--no-restore" "--quiet" "--interactive" "--no-readline"]))

(def (first-existing paths)
  (cond ((empty? paths) nil)
        ((await (file/exists? (first paths))) (first paths))
        (else (first-existing (rest paths)))))

(def (binary home)
  (let [bin (path/join home "bin")]
    (first-existing (if (windows?)
                      [(path/join bin "x64" "Rterm.exe") (path/join bin "Rterm.exe")]
                      [(path/join bin "R")]))))

(def (version install)
  (map (fn [part] (or (string/to-number part) 0))
       (string/split (string/replace (or (path/name install) "") "R-" "") ".")))

(def (newest installs)
  (let [found (keep (fn [install] (when-let [found (binary install)] [(version install) found])) installs)]
    (when (not (empty? found))
      (second (last (sort-by first found))))))

(def (folders root)
  (try (map (fn [entry] entry.path) (filter (fn [entry] entry.dir) (await (file/list root))))
       (catch error e [])))

(def (registry)
  (if (windows?)
    (keep (fn [key]
            (try (let [found (await (process/run "reg" ["query" key "/v" "InstallPath"]))
                       line (find (fn [line] (string/includes? line "REG_SZ")) (string/lines found.stdout))]
                   (when line (string/trim (second (string/split line "REG_SZ" 2)))))
                 (catch error e nil)))
          ["HKLM\\SOFTWARE\\R-core\\R" "HKCU\\SOFTWARE\\R-core\\R"])
    []))

(def (installs)
  (let [roots (concat (keep (fn [variable] (when-let [root (process/env variable)] (path/join root "R")))
                            ["ProgramFiles" "ProgramW6432"])
                      (if-let [local (process/env "LOCALAPPDATA")] [(path/join local "Programs" "R")] []))]
    (concat (mapcat folders roots)
            (registry)
            (if (= (process/os) :macos) ["/Library/Frameworks/R.framework/Resources"] []))))

(def (locate configured sources)
  (cond (configured (if (await (file/exists? configured)) configured ((get sources :which) configured)))
        (else (or ((get sources :which) (if (windows?) "Rterm" "R"))
                  (when-let [home ((get sources :home))] (binary home))
                  (newest ((get sources :installs)))))))

(def (find-r)
  (locate (plugin-setting 'r :program)
          {which: process/which
           home: (fn [] (process/env "R_HOME"))
           installs: installs}))

(def (session-name directory)
  (let [folder (or (path/name directory) "R")
        named (fn [number] (if (= number 1) "*R:{folder}*" "*R:{folder}:{number}*"))]
    (named (find (fn [number] (not (buffer-exists? (named number))))
                 (range 1 (+ 2 (count (buffer-list))))))))

(def (heard session kind payload)
  (when-let [handle (get @handlers kind)]
    (handle session payload)))

(def (request session call)
  (and session (console-request session call)))

(def (fit session)
  (let [width (buffer-width session)]
    (when (and width (>= width 10) (console-running? session) (not (helper-of session))
               (not= width (get-in @state [:widths session])))
      (when (request session "getOption(\"oxmacs\")$width({width}L)")
        (swap! state assoc-in [:widths session] width)))))

(def kept-history 1000)

(def (history-file)
  (when-let [home (oxmacs-home)] (path/join home "r-history")))

(def (load-history session)
  (when-let [file (history-file)]
    (let [lines (try (string/lines (await (file/read file))) (catch error e []))
          inputs (keep (fn [line] (try (json/read line) (catch error e nil))) lines)
          kept (vec (drop (max 0 (- (count inputs) kept-history)) inputs))]
      (set-console-history session kept)
      (swap! state assoc-in [:saved session] (count kept))
      (when (> (count inputs) (* 2 kept-history))
        (await (file/write file (apply str (map (fn [input] (str (json/write input) "\n")) kept))))))))

(def (save-history session)
  (when-let [file (history-file)]
    (let [inputs (console-history session)
          saved (or (get-in @state [:saved session]) 0)
          fresh (drop (min saved (count inputs)) inputs)]
      (when (not (empty? fresh))
        (swap! state assoc-in [:saved session] (count inputs))
        (await (file/append file (apply str (map (fn [input] (str (json/write input) "\n")) fresh))))))))

(def (idle session)
  (save-history session)
  (when-let [file (helper-of session)]
    (swap! state update :helpers dissoc session)
    (try (await (file/remove file)) (catch error e nil)))
  (map (fn [hook] (hook session)) (vals @ready-hooks))
  (fit session))

(def (launch existing)
  (if-let [program (find-r)]
    (let [profile (await (helper/write))
          launched (get (swap! state update :launched inc) :launched)
          plots (path/join (path/temp) "oxmacs" "plots-{(crypto/random 4)}-{launched}")
          original (process/env "R_PROFILE_USER")
          env (merge {R_PROFILE_USER: profile OXMACS_R_PLOTS: plots}
                     (if original {OXMACS_R_PROFILE_USER: original} {}))
          restarted (and existing (known? existing))
          root (if restarted (get-in @state [:roots existing]) (project-here))
          directory (or root (default-directory))
          name (if restarted existing (session-name directory))]
      (console-start name :program program :args (arguments) :directory directory :env env :label label :keys @keys
                     :on-output (fn [kind payload] (heard name kind payload))
                     :on-idle (fn [] (idle name))
                     :on-resize (fn [_] (fit name)))
      (swap! state (fn [old] (-> old
                                 (assoc-in [:helpers name] profile)
                                 (assoc-in [:roots name] root)
                                 (update :widths dissoc name))))
      (load-history name)
      name)
    (do (message missing) nil)))

(def (ensure)
  (let [session (current-session)]
    (if (running? session)
      session
      (when-let [started (launch session)]
        (link! started)
        started))))

(def (show-console session select)
  (display-buffer session :select select))

(def (evaluate session code)
  (console-send session code))

(def (send code)
  (when-let [running (ensure)]
    (link! running)
    (show-console running false)
    (evaluate running code)
    running))

(def escapes {"\"" "\\\"" "\\" "\\\\" "\n" "\\n" "\r" "\\r" "\t" "\\t"})

(def (r-string text)
  (str "\"" (apply str (map (fn [character] (or (get escapes (str character)) character))
                            (string/chars text)))
       "\""))
