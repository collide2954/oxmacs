(module
  (import (ox string) (ox path) (ox file) (ox process) (ox json) (ox wsl)
          (pixi project :as project)))

(def page "*pixi*")
(def output "*pixi run*")

(defonce projects (atom {}))
(defonce roots (atom {}))
(defonce told (atom false))
(defonce shown (atom nil))

(def (root-here)
  "The pixi project the current folder lies in, or nil, looked for once per folder."
  (let [directory (default-directory)
        known (get @roots directory :unknown)]
    (if (= known :unknown)
      (let [root (project/root-of directory)]
        (swap! roots assoc directory root)
        root)
      known)))

(def (here!)
  (or (root-here)
      (raise (error "This folder isn't in a pixi project; pixi init makes one"))))

(def (choices-file)
  (when-let [folder (plugin-folder 'pixi)]
    (path/join folder "environments.json")))

(def (choices)
  (try
    (let [file (choices-file)]
      (if (and file (await (file/exists? file)))
        (json/read (await (file/read file)))
        []))
    (catch error _e [])))

(def (chosen root)
  "The environment programs in the project at root run in: the one picked for it, else pixi's default."
  (or (get (find (fn [choice] (= choice.root root)) (choices)) :environment)
      "default"))

(def (remember! root environment)
  (when-let [file (choices-file)]
    (await (file/make-dir (path/parent file)))
    (await (file/write file (json/write (vec (concat (filter (fn [choice] (not= choice.root root)) (choices))
                                                     [{root: root environment: environment}])))))))

(def (stamp manifest root)
  "When the manifest and lock file of the project at root last changed."
  (vec (map (fn [path] (try (get (await (file/metadata path)) :modified) (catch error _e nil)))
            [manifest (path/join root "pixi.lock")])))

(def (outside root)
  "Where to start pixi itself for the project at root, so it doesn't get root's own environment."
  (or (path/parent root) root))

(def (label found)
  (get {ready: $"pixi:{found.environment}"
        activating: $"pixi:{found.environment}…"
        failed: $"pixi:{found.environment}!"
        no-pixi: "pixi?"}
       found.status))

(def (show-mode-line)
  (let [root (root-here)
        found (when root (get @projects root))]
    (set-mode-line :pixi (when found (label found)))))

(def (failed! root ran)
  (let [place (wsl/place root)]
    (swap! projects assoc-in [root :status] :failed)
    (message (if (and place (= ran.status 127))
               $"pixi isn't installed in {place.distro}; M-x pixi-install-pixi installs it there"
               $"pixi couldn't make {(path/name root)}'s environment: {(project/reason ran)}"))))

(def (activate! root)
  "Gives programs started in the project at root its pixi environment, installing it first when it isn't."
  (let [manifest (project/manifest-in root)
        environment (chosen root)
        pixi (project/program root)]
    (cond
      [(nil? manifest)
       (swap! projects dissoc root)
       (process/environment! root nil)]
      [(nil? pixi)
       (swap! projects assoc root {manifest: manifest environment: environment status: :no-pixi})
       (unless @told
         (reset! told true)
         (message "This folder is a pixi project, but pixi isn't installed; M-x pixi-install-pixi installs it"))]
      [else
       (swap! projects assoc root {manifest: manifest environment: environment status: :activating
                                   stamp: (stamp manifest root)})
       (show-mode-line)
       (let [ran (try (process/run pixi ["shell-hook" "--json" "-e" environment "--manifest-path" manifest]
                                   :cwd (outside root))
                   (catch error e {status: -1 stdout: "" stderr: e.message}))]
         (if (= ran.status 0)
           (do (process/environment! root (project/variables-of ran.stdout))
             (swap! projects assoc-in [root :status] :ready))
           (failed! root ran)))])
    (show-mode-line)
    (refresh-page)))

(def (look!)
  (when-let [root (root-here)]
    (unless (get @projects root)
      (activate! root)))
  (show-mode-line))

(def (changed!)
  "Activates again the projects whose manifest or lock file changed."
  (each [root (keys @projects)]
    (let [found (get @projects root)]
      (when (and found.manifest (not= found.status :activating)
                 (not= (stamp found.manifest root) found.stamp))
        (activate! root)))))

(def manifests ["pixi.toml" "pyproject.toml" "pixi.lock"])

(on-window-change :pixi (fn [] (look!)))

(on-disk-change :pixi (fn [] (changed!)))

(on-saved :pixi (fn [path]
                  (when (some (fn [name] (= name (path/name path))) manifests)
                    (reset! roots {})
                    (activate! (path/parent path)))))

(spawn (look!))

(def (pixi-for root)
  (or (project/program root)
      (raise (error "pixi isn't installed; M-x pixi-install-pixi installs it"))))

(def (manifest-of root)
  (or (project/manifest-in root)
      (raise (error $"{root} has no pixi.toml or [tool.pixi] any more"))))

(def (asked root args)
  "The output of pixi run with args for the project at root, raising what pixi said when it fails."
  (let [ran (process/run (pixi-for root) (vec (concat args ["--manifest-path" (manifest-of root)])) :cwd (outside root))]
    (if (= ran.status 0)
      ran.stdout
      (raise (error $"pixi {(first args)}: {(project/reason ran)}")))))

(def (environments root)
  (project/environments-of (asked root ["info" "--json"])))

(def (tasks root)
  (project/tasks-of (asked root ["task" "list" "--json" "-e" (chosen root)]) (chosen root)))

(def (packages root)
  (project/packages-of (asked root ["list" "--json" "-e" (chosen root)])))

(def (then-activate root)
  (fn [status]
    (when (= status 0)
      (spawn (activate! root)))))

(def (run-task root name)
  (let [environment (chosen root)]
    (run-job output (pixi-for root) ["run" "-e" environment "--manifest-path" (manifest-of root) name]
             :cwd root :title $"pixi run -e {environment} {name}")))

(defcommand pixi-run "Run a task of the pixi project here in its environment"
  (let [root (here!)
        names (vec (map (fn [task] task.name) (tasks root)))]
    (when (empty? names)
      (raise (error "This pixi project has no tasks yet; pixi task add makes one")))
    (run-task root (prompt "Run pixi task: " :choices names))))

(def (switch! root environment)
  (remember! root environment)
  (activate! root)
  (message $"Programs in {(path/name root)} run in pixi's {environment} environment now"))

(defcommand pixi-environment "Choose the pixi environment programs in the project here run in"
  (let [root (here!)
        names (vec (map (fn [environment] environment.name) (environments root)))
        picked (prompt $"Environment for {(path/name root)}: " :choices names)]
    (unless (some (fn [name] (= name picked)) names)
      (raise (error $"{(path/name root)} has no environment named {picked}")))
    (switch! root picked)))

(defcommand pixi-add "Add a package to the pixi project here, such as numpy or python>=3.12"
  (let [root (here!)
        wanted (string/trim (prompt "Add package: "))]
    (unless (string/blank? wanted)
      (run-job output (pixi-for root) ["add" "--manifest-path" (manifest-of root) wanted]
               :cwd root :title $"pixi add {wanted}" :on-exit (then-activate root)))))

(defcommand pixi-remove "Remove a package the pixi project here asks for"
  (let [root (here!)
        item (line-item)
        named (vec (get (or (find (fn [environment] (= environment.name (chosen root))) (environments root)) {})
                        :dependencies []))
        unwanted (if (and item item.explicit) item.package (prompt "Remove package: " :choices named))]
    (unless (string/blank? unwanted)
      (run-job output (pixi-for root) ["remove" "--manifest-path" (manifest-of root) unwanted]
               :cwd root :title $"pixi remove {unwanted}" :on-exit (then-activate root)))))

(defcommand pixi-install "Install the pixi environment of the project here"
  (let [root (here!)
        environment (chosen root)]
    (run-job output (pixi-for root) ["install" "-e" environment "--manifest-path" (manifest-of root)]
             :cwd root :title $"pixi install -e {environment}" :on-exit (then-activate root))))

(defcommand pixi-update "Update the packages of the pixi project here and its lock file"
  (let [root (here!)]
    (run-job output (pixi-for root) ["update" "--manifest-path" (manifest-of root)]
             :cwd root :title "pixi update" :on-exit (then-activate root))))

(defcommand pixi-install-pixi "Install pixi with its official installer, which downloads it from github.com/prefix-dev/pixi"
  (let [root (root-here)
        place (when root (wsl/place root))
        where (if place place.distro "this computer")
        [program args] (project/installer (if place :linux (process/os)))]
    (when (yes-or-no $"Install pixi on {where} with its official installer, downloading it from github.com/prefix-dev/pixi?")
      (run-job output program args :cwd (or root (path/home)) :title $"Installing pixi on {where}"
               :on-exit (fn [status]
                          (when (= status 0)
                            (reset! told false)
                            (message $"pixi is installed on {where}")
                            (when root (spawn (activate! root)))))))))

(def (environment-line environment current)
  {line: [[(if (= environment.name current) "* " "  ") :keyword]
          (string/pad-end environment.name 20)
          [(string/join " " environment.features) :comment]]
   environment: environment.name})

(def (task-line task)
  {line: ["  " (string/pad-end task.name 20) [(or task.description task.command) :comment]]
   task: task.name})

(def (package-line package)
  {line: ["  " (string/pad-end package.name 28) [package.version (if package.explicit :string :comment)]]
   package: package.name
   explicit: package.explicit})

(def (section title rows empty)
  (vec (concat [[[title :heading]]]
               (if (empty? rows) [[[$"  {empty}" :comment]]] rows)
               [""])))

(def (show-page root)
  (let [current (chosen root)
        found (or (get @projects root) {})
        listed (packages root)
        rows (vec (concat (section "Environments" (map (fn [e] (environment-line e current)) (environments root)) "none")
                          (section "Tasks" (map task-line (tasks root)) "none yet: pixi task add makes one")
                          (section "Packages" (map package-line (concat (filter (fn [p] p.explicit) listed)
                                                                        (filter (fn [p] (not p.explicit)) listed)))
                                   "none yet: a adds one")))
        buffer (open-buffer page rows
                            :directory root
                            :keys {"RET" 'pixi-select "r" 'pixi-run "e" 'pixi-environment "a" 'pixi-add
                                   "d" 'pixi-remove "i" 'pixi-install "u" 'pixi-update "g" 'pixi "q" 'quit-window}
                            :line 1)]
    (reset! shown root)
    (set-header buffer [[[(path/name root) :heading]
                         [$"   pixi's {current} environment" :comment]
                         [(get {ready: ", active" activating: ", activating…" failed: ", failed; g tries again"}
                               found.status "")
                          :comment]]
                        [["RET runs a task or switches to an environment  r runs  e switches  a adds  d removes  i installs  u updates  g refreshes  q quits" :comment]]])))

(def (refresh-page)
  (when (and @shown (buffer-live? page))
    (try (show-page @shown) (catch error _e nil))))

(defcommand pixi "Show the pixi project here: its environments, tasks and packages"
  (let [root (here!)]
    (unless (get @projects root)
      (activate! root))
    (show-page root)))

(defcommand pixi-select "Run the task or switch to the environment on this line of *pixi*"
  (let [item (line-item)
        root (here!)]
    (cond
      [(and item item.task) (run-task root item.task)]
      [(and item item.environment) (switch! root item.environment)]
      [(and item item.package) (message $"{item.package}: d removes it")]
      [else (message "RET runs the task or switches to the environment on its line")])))
