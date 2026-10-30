(module
  (import (ox file) (ox path) (ox process) (ox string)
          (explorer tree :as tree)))

(def tree-buffer "*explorer*")
(def most-files 5000)

(defonce state
  (atom {root: nil open: #{} entries: {} hidden: nil ignored: #{} changes: {} problems: {}
         filter: "" current: nil width: 32 repo: false}))

(defonce files (atom nil))
(defonce refreshing (atom false))
(defonce stale (atom false))
(def settle-ms 150)

(def (windows?)
  (= (process/os) :windows))

(def (key-of place)
  (if (windows?) (string/lower place) place))

(def (same? place folder)
  (and place folder (path/inside? place folder) (path/inside? folder place)))

(def (inside? place folder)
  (and place folder (path/inside? place folder) (not (path/inside? folder place))))

(def (shown?)
  (when-let [side (side-window)]
    (= side.buffer tree-buffer)))

(def (root-of start)
  (or (project-root start) (language-root start) start))

(def (load folder)
  (let [found (try (await (file/list folder)) (catch error _e []))]
    (swap! state assoc-in [:entries folder] (vec found))))

(def (absolute root rel)
  (apply path/join root (string/split rel "/")))

(def (git-status root)
  (try
    (let [result (await (process/run "git" ["--no-optional-locks" "status" "--porcelain=v1" "-z" "--ignored=matching"]
                                     :cwd root))]
      (if (= result.status 0) (tree/parse-status result.stdout) {}))
    (catch error _e {})))

(def (load-git)
  (let [root (get @state :root)
        states (if (get @state :repo) (await (git-status root)) {})
        keyed (fn [found] (reduce (fn [out [rel change]] (assoc out (key-of (absolute root rel)) change)) {} (entries found)))
        ignored (set (keep (fn [[rel change]] (when (= change :ignored) (absolute root rel))) (entries states)))]
    (swap! state assoc :changes (merge (keyed states) (keyed (tree/roll-up states))) :ignored ignored)))

(def (load-problems)
  (when-let [root (get @state :root)]
    (let [mine (reduce (fn [out [place counts]] (if (inside? place root) (assoc out place counts) out))
                       {} (entries (diagnostic-counts)))]
      (swap! state assoc :problems
             (reduce (fn [out [place counts]] (assoc out (key-of place) counts))
                     {} (entries (tree/roll-up-problems mine root)))))))

(def (walk-files folder rel out)
  (reduce (fn [out entry]
            (cond
              [(>= (count out) most-files) out]
              [(or entry.hidden (= entry.name ".git")) out]
              [entry.dir (walk-files entry.path (str rel entry.name "/") out)]
              [else (conj out (str rel entry.name))]))
          out
          (try (await (file/list folder)) (catch error _e []))))

(def (all-files root repo)
  (if repo
    (let [result (try (await (process/run "git" ["ls-files" "--cached" "--others" "--exclude-standard" "-z"] :cwd root))
                   (catch error _e nil))]
      (if (and result (= result.status 0))
        (vec (remove string/blank? (string/split result.stdout "\u{0}")))
        []))
    (walk-files root "" [])))

(def (known-files)
  (let [s @state]
    (when (nil? @files)
      (reset! files (spawn (all-files s.root s.repo))))
    (await @files)))

(def (filtering? s)
  (not (string/blank? s.filter)))

(def (compute s)
  (if (filtering? s)
    (tree/filtered (or s.found []) s.filter (fn [parts] (apply path/join s.root parts)))
    (tree/rows s)))

(def (header s rows)
  (let [title [(str " " (string/upper (or (path/name s.root) s.root))) :heading]]
    (if (filtering? s)
      [[title]
       [["  Filter: " :comment] [s.filter :link] ["  {(count (filter :match rows))} found" :comment]]]
      [[title]])))

(def (row-index rows place)
  (when place
    (first (keep (fn [at] (when (= (key-of (get (nth rows at) :path)) (key-of place)) at))
                 (range (count rows))))))

(def (draw (focus nil))
  (let [s @state
        rows (compute s)
        width (max 12 (or s.width 32))
        lines (vec (map (fn [row]
                          (merge (tree/line row (get s.changes (key-of row.path)) (get s.problems (key-of row.path))
                                            width (and s.current (= (key-of row.path) (key-of s.current))))
                                 row))
                        rows))
        at (row-index rows focus)]
    (set-header tree-buffer (header s rows))
    (if at
      (set-lines tree-buffer (if (empty? lines) [""] lines) :line at)
      (set-lines tree-buffer (if (empty? lines) [""] lines)))))

(def (ancestors place root)
  (loop [folder (path/parent place)
         out []]
    (if (or (nil? folder) (not (inside? folder root)))
      out
      (again (path/parent folder) (conj out folder)))))

(def (reveal place)
  (let [root (get @state :root)]
    (each [folder (reverse (ancestors place root))]
      (unless (get-in @state [:entries folder]) (load folder))
      (swap! state update :open conj folder))
    (swap! state assoc :current place)
    (draw place)))

(def (open-root root place)
  (swap! state assoc :root root :open #{} :entries {} :filter "" :found nil :current place
         :repo (await (file/exists? (path/join root ".git"))))
  (reset! files nil)
  (ensure-buffer root)
  (load root)
  (load-git)
  (load-problems)
  (if place (reveal place) (draw)))

(def (follow-to place directory)
  (let [root (get @state :root)]
    (cond
      [(and place (inside? place root)) (reveal place)]
      [(or (nil? root) (setting :follow)) (open-root (root-of (if place (path/parent place) directory)) place)]
      [else nil])))

(def tree-keys
  {"RET" 'explorer-visit "TAB" 'explorer-toggle "SPC" 'explorer-peek
   "<right>" 'explorer-expand "<left>" 'explorer-collapse
   "DEL" 'explorer-filter-delete "C-g" 'explorer-quit
   "C-c C-n" 'explorer-new-file "C-c C-d" 'explorer-new-folder
   "C-c C-r" 'explorer-rename "<f2>" 'explorer-rename "C-c C-c" 'explorer-copy
   "C-c C-k" 'explorer-delete "<delete>" 'explorer-delete
   "C-c C-o" 'explorer-open-externally "C-c C-w" 'explorer-copy-path
   "C-c C-h" 'explorer-toggle-hidden "C-c C-g" 'explorer-refresh})

(def (ensure-buffer root)
  (open-buffer tree-buffer [""] :keys tree-keys :show false :directory root :cursor :line
               :on-click clicked :on-drop dropped :on-type typed :on-resize resized))

(def (show)
  (let [place (buffer-file)
        directory (default-directory)]
    (when (nil? (get @state :hidden))
      (swap! state assoc :hidden (setting :hidden)))
    (unless (buffer-exists? tree-buffer)
      (ensure-buffer (or (get @state :root) directory)))
    (show-side-window tree-buffer :edge (setting :edge) :width (setting :width) :select true)
    (follow-to place directory)
    (when @stale (refresh))))

(defcommand explorer "Show the file tree beside the windows, move to it, or hide it"
  (let [side (side-window)]
    (cond
      [(not (shown?)) (show)]
      [side.selected (close-side-window)]
      [else (select-side-window)])))

(def (here)
  (line-item))

(def (toggle row)
  (unless (filtering? @state)
    (if (contains? (get @state :open) row.path)
      (swap! state update :open disj row.path)
      (do (unless (get-in @state [:entries row.path]) (load row.path))
        (swap! state update :open conj row.path)))
    (draw row.path)))

(def (visit place stay)
  (when (and (not stay) (filtering? @state))
    (swap! state assoc :filter "")
    (draw place))
  (select-main-window)
  (open-file place)
  (when stay (select-side-window)))

(defcommand explorer-visit "Open the file on this line, or open or close the folder"
  (when-let [row (here)]
    (if row.dir (toggle row) (visit row.path false))))

(defcommand explorer-peek "Show the file on this line without leaving the tree"
  (when-let [row (here)]
    (if row.dir (toggle row) (visit row.path true))))

(defcommand explorer-toggle "Open or close the folder on this line"
  (when-let [row (here)]
    (when row.dir (toggle row))))

(defcommand explorer-expand "Open the folder on this line, or move into it"
  (when-let [row (here)]
    (cond
      [(not row.dir) nil]
      [row.open (goto-line (inc (current-line)))]
      [else (toggle row)])))

(defcommand explorer-collapse "Close the folder on this line, or move to the folder holding it"
  (when-let [row (here)]
    (if (and row.dir row.open)
      (toggle row)
      (goto-item {path: (path/parent row.path)}))))

(def (refilter)
  (let [found (known-files)]
    (swap! state assoc :found found)
    (draw)
    (with-buffer tree-buffer (goto-item {match: true}))))

(def (typed text)
  (swap! state update :filter str text)
  (refilter))

(defcommand explorer-filter-delete "Take back the last character of the filter"
  (let [now (get @state :filter)]
    (unless (string/blank? now)
      (swap! state assoc :filter (string/slice now 0 (dec (count now))))
      (if (filtering? @state)
        (refilter)
        (draw (get @state :current))))))

(defcommand explorer-quit "Clear the filter, or go back to the other windows"
  (if (filtering? @state)
    (do (swap! state assoc :filter "")
      (draw (get @state :current)))
    (select-main-window)))

(def (reload)
  (let [s @state]
    (when s.root
      (each [folder (keys s.entries)] (load folder))
      (load-git)
      (load-problems)
      (reset! files nil)
      (if (filtering? @state) (refilter) (draw)))))

(def (refresh)
  (reset! stale true)
  (when (and (shown?) (compare-and-set! refreshing false true))
    (try
      (loop []
        (sleep settle-ms)
        (reset! stale false)
        (reload)
        (when (and @stale (shown?)) (again)))
      (finally (reset! refreshing false)))))

(defcommand explorer-refresh "Read the folders and git status again" (refresh))

(defcommand explorer-toggle-hidden "Show or hide hidden files and the files git ignores"
  (swap! state update :hidden not)
  (draw (when-let [row (here)] row.path))
  (message (if (get @state :hidden) "Showing hidden and ignored files" "Hiding hidden and ignored files")))

(def (relative place)
  (let [root (get @state :root)]
    (if (inside? place root)
      (string/replace (path/relative place root) (path/separator) "/")
      place)))

(def (folder-for row)
  (cond
    [(nil? row) (get @state :root)]
    [row.dir row.path]
    [else (path/parent row.path)]))

(def (reload-around & places)
  (each [place places]
    (when-let [folder (path/parent place)]
      (when (get-in @state [:entries folder]) (load folder))))
  (load-git)
  (reset! files nil))

(defcommand explorer-new-file "Make a file in the folder on this line and open it"
  (let [folder (folder-for (here))
        name (await (prompt "New file in {(relative folder)}/: "))
        target (apply path/join folder (string/split name "/"))]
    (if (await (file/exists? target))
      (message "{(relative target)} already exists")
      (do (await (file/make-dir (path/parent target)))
        (await (file/write target ""))
        (swap! state update :open conj folder)
        (reload-around target)
        (reveal target)
        (visit target false)))))

(defcommand explorer-new-folder "Make a folder in the folder on this line"
  (let [folder (folder-for (here))
        name (await (prompt "New folder in {(relative folder)}/: "))
        target (apply path/join folder (string/split name "/"))]
    (if (await (file/exists? target))
      (message "{(relative target)} already exists")
      (do (await (file/make-dir target))
        (swap! state update :open conj folder)
        (reload-around target)
        (draw target)))))

(def (transfer moving)
  (when-let [row (here)]
    (let [root (get @state :root)
          answer (await (prompt (if moving "Rename or move to: " "Copy to: ") :initial (relative row.path)))
          target (absolute root answer)]
      (await (file/make-dir (path/parent target)))
      (when (if moving
              (await (move-path row.path target))
              (when (or (not (await (file/exists? target)))
                        (await (yes-or-no "{answer} exists; replace it? ")))
                (await (file/copy row.path target))
                true))
        (reload-around row.path target)
        (reveal target)
        (message "{(if moving "Moved" "Copied")} {(relative row.path)} to {answer}")))))

(defcommand explorer-rename "Rename or move the file or folder on this line" (transfer true))
(defcommand explorer-copy "Copy the file or folder on this line" (transfer false))

(defcommand explorer-delete "Delete the file or folder on this line"
  (when-let [row (here)]
    (let [trash (get (settings) :delete-to-trash)]
      (when (await (yes-or-no "{(if trash "Trash" "Delete")} {(relative row.path)}?"))
        (await (delete-path row.path))
        (reload-around row.path)
        (draw (path/parent row.path))
        (message "{(if trash "Trashed" "Deleted")} {(relative row.path)}")))))

(defcommand explorer-open-externally "Open the file or folder on this line with its usual app"
  (when-let [row (here)] (open-externally row.path)))

(defcommand explorer-copy-path "Copy the path of the file or folder on this line"
  (when-let [row (here)]
    (clipboard-set row.path)
    (message "Copied {row.path}")))

(def (clicked line clicks)
  (when (= clicks 1)
    (when-let [row (with-buffer tree-buffer (line-item line))]
      (if row.dir (toggle row) (visit row.path false)))))

(def (dropped from to)
  (let [[moving onto] (with-buffer tree-buffer [(line-item from) (line-item to)])]
    (when (and moving onto)
      (let [into (folder-for onto)
            target (path/join into moving.name)]
        (unless (or (same? (path/parent moving.path) into)
                    (same? moving.path into)
                    (inside? into moving.path))
          (when (await (yes-or-no "Move {(relative moving.path)} into {(relative into)}/?"))
            (if (await (file/exists? target))
              (message "{(relative target)} already exists")
              (do (await (move-path moving.path target))
                (swap! state update :open conj into)
                (reload-around moving.path target)
                (reveal target)))))))))

(def (resized width)
  (swap! state assoc :width width)
  (when (get @state :root) (draw)))

(on-window-change :explorer
                  (fn []
                    (when (and (shown?) (not= (buffer-name) tree-buffer) (or (buffer-file) (nil? (get @state :root))))
                      (follow-to (buffer-file) (default-directory)))))

(on-saved :explorer (fn [_place] (refresh)))
(on-disk-change :explorer (fn [] (refresh)))
(on-diagnostics-change :explorer (fn [] (when (shown?) (load-problems) (draw))))
