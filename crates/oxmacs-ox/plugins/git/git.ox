(module
  (import (ox file) (ox path) (ox string) (ox time)
          (git repo :as repo)
          (git status :as status)))

(defonce statuses (atom {}))

(def status-keys
  {"n" 'git-next-section "p" 'git-previous-section "TAB" 'git-toggle "RET" 'git-visit
   "g" 'git-refresh "s" 'git-stage "u" 'git-unstage "k" 'git-discard "q" 'quit-window})

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(def (current)
  (get @statuses (buffer-name)))

(def (row-at state line)
  (when (and (>= line 0) (< line (count state.rows)))
    (nth state.rows line)))

(def (spot row)
  (when (and row (not (= row.kind :info)) (not (= row.kind :blank)))
    row))

(def (line-of rows wanted)
  (when wanted
    (first (filter (fn [i] (= (nth rows i) wanted)) (range (count rows))))))

(def (first-line rows)
  (or (first (filter (fn [i] (= (get (nth rows i) :kind) :file)) (range (count rows))))
      (first (filter (fn [i] (= (get (nth rows i) :kind) :section)) (range (count rows))))
      0))

(def (now)
  (// (time/now) 1000))

(def (redraw state focus)
  (let [shown (status/render state.snapshot state.folds state.diffs (now))
        here (= (buffer-name) state.name)
        line (if here (current-line) 0)
        wanted (or focus (and here (spot (row-at state line))))
        state (assoc state :rows shown.rows)]
    (swap! statuses assoc state.name state)
    (set-lines state.name shown.lines)
    (when here
      (goto-line (or (line-of shown.rows wanted)
                     (min line (max 0 (- (count shown.rows) 1))))))))

(def (reread state)
  (let [snapshot (repo/read-status state.root)]
    (assoc state
           :snapshot snapshot
           :diffs (repo/diffs state.root snapshot state.folds.open))))

(def (refresh name)
  (when-let [state (get @statuses name)]
    (redraw (reread state) nil)))

(def (refresh-all)
  (map (fn [name]
         (if (buffer-exists? name)
           (try (refresh name) (catch error e (message e.message)))
           (swap! statuses dissoc name)))
       (keys @statuses))
  nil)

(def (buffer-for root)
  "git: {(path/name root)}")

(def (show found)
  (let [name (buffer-for found.root)
        known (get @statuses name)
        state (or known
                  {name: name root: found.root metadata: found.metadata
                   folds: {closed: {} open: {} hidden: {}} diffs: {} rows: []})
        state (reread state)
        shown (status/render state.snapshot state.folds state.diffs (now))
        state (assoc state :rows shown.rows)]
    (swap! statuses assoc name state)
    (open-buffer name shown.lines :keys status-keys :directory state.root :watch state.metadata)
    (unless known
      (goto-line (first-line shown.rows)))))

(defcommand git-status "Show the Git status of the current repository"
  (quietly
    (fn []
      (if-let [found (repo/locate (default-directory))]
        (show found)
        (message "Not inside a Git repository")))))

(def (item? row)
  (some (fn [kind] (= row.kind kind)) [:section :file :hunk :stash :commit]))

(def (step forward)
  (when-let [state (current)]
    (let [line (current-line)
          lines (if forward
                  (range (+ line 1) (count state.rows))
                  (reverse (range 0 line)))
          found (first (filter (fn [at] (item? (nth state.rows at))) lines))]
      (when (some? found) (goto-line found)))))

(defcommand git-next-section "Move to the next section, file or hunk" (step true))
(defcommand git-previous-section "Move to the previous section, file or hunk" (step false))

(def (flip folds key)
  (if (get folds key) (dissoc folds key) (assoc folds key true)))

(def (with-folds state kind change)
  (assoc state :folds (assoc state.folds kind (change (get state.folds kind)))))

(defcommand git-toggle "Fold or unfold the section, file or hunk at point"
  (when-let [state (current)]
    (when-let [row (row-at state (current-line))]
      (quietly
        (fn []
          (cond
            [(= row.kind :section)
             (redraw (with-folds state :closed (fn [closed] (flip closed row.section))) nil)]
            [(= row.kind :file)
             (let [state (with-folds state :open (fn [open] (flip open [row.section row.path])))]
               (redraw (assoc state :diffs (repo/diffs state.root state.snapshot state.folds.open)) nil))]
            [(= row.kind :hunk)
             (redraw (with-folds state :hidden (fn [hidden] (flip hidden [row.section row.path row.hunk])))
                     {kind: :hunk section: row.section path: row.path hunk: row.hunk})]
            [(= row.kind :line)
             (redraw (with-folds state :hidden (fn [hidden] (assoc hidden [row.section row.path row.hunk] true)))
                     {kind: :hunk section: row.section path: row.path hunk: row.hunk})]
            [else nil]))))))

(def (line-number state row)
  (when (some? row.hunk)
    (let [hunk (nth (get state.diffs [row.section row.path]) row.hunk)
          before (if (some? row.line)
                   (count (filter (fn [i] (not (= (get (nth hunk.lines i) :kind) :removed)))
                                  (range row.line)))
                   0)]
      (+ (max 1 hunk.new-start) before))))

(defcommand git-visit "Visit the file at point"
  (when-let [state (current)]
    (let [row (row-at state (current-line))]
      (if (and row (some? row.path))
        (quietly
          (fn []
            (let [target (path/join state.root row.path)
                  number (line-number state row)]
              (if (await (file/exists? target))
                (do
                  (open-file target)
                  (when number (goto-line (- number 1))))
                (message "{row.path} is not in the working tree")))))
        (message "Nothing to visit here")))))

(defcommand git-refresh "Read the repository again"
  (when-let [state (current)]
    (quietly (fn [] (refresh state.name)))))

(def (target state)
  (when-let [row (row-at state (current-line))]
    (cond
      [(= row.kind :section) {kind: :section section: row.section paths: []}]
      [(= row.kind :file) {kind: :files section: row.section paths: [row.path]}]
      [else nil])))

(def (describe paths)
  (if (= (count paths) 1) (first paths) "{(count paths)} files"))

(def (change state args)
  (repo/git state.root args)
  (refresh state.name))

(def (stage state found)
  (let [section found.section]
    (cond
      [(= section :staged) (message "Already staged")]
      [(and (= found.kind :section) (= section :untracked))
       (change state (vec (concat ["add" "--"] state.snapshot.untracked)))]
      [(and (= found.kind :section) (= section :unstaged))
       (change state ["add" "--update" "--" "."])]
      [(and (= found.kind :files) (or (= section :untracked) (= section :unstaged)))
       (change state (vec (concat ["add" "--"] found.paths)))]
      [else (message "Nothing to stage here")])))

(defcommand git-stage "Stage the file or section at point"
  (when-let [state (current)]
    (if-let [found (target state)]
      (quietly (fn [] (stage state found)))
      (message "Nothing to stage here"))))

(def (renamed-from state paths)
  (keep (fn [change] (when (some (fn [path] (= path change.path)) paths) change.from))
        state.snapshot.staged))

(def (forget state paths)
  (change state (vec (concat (if state.snapshot.born ["reset" "-q" "--"] ["rm" "--cached" "-r" "-q" "--"])
                             paths))))

(def (unstage state found)
  (cond
    [(not (= found.section :staged)) (message "Nothing staged here")]
    [(= found.kind :section) (forget state ["."])]
    [else (forget state (vec (concat found.paths (renamed-from state found.paths))))]))

(defcommand git-unstage "Unstage the file or section at point"
  (when-let [state (current)]
    (if-let [found (target state)]
      (quietly (fn [] (unstage state found)))
      (message "Nothing staged here"))))

(def (throw-away state paths)
  (let [trash (setting :discard-to-trash)]
    (when (await (yes-or-no (if trash
                              "Move {(describe paths)} to the trash?"
                              "Delete {(describe paths)}?")))
      (map (fn [path]
             (let [full (path/join state.root path)]
               (await (if trash (file/trash full) (file/remove full)))))
           paths)
      (refresh state.name))))

(def (discard state found)
  (let [section found.section]
    (cond
      [(= section :staged) (message "Unstage it first with u")]
      [(= section :untracked)
       (throw-away state (if (= found.kind :section) state.snapshot.untracked found.paths))]
      [(and (= section :unstaged) (= found.kind :section))
       (when (await (yes-or-no "Discard all unstaged changes?"))
         (change state ["restore" "--" "."]))]
      [(= section :unstaged)
       (when (await (yes-or-no "Discard changes to {(describe found.paths)}?"))
         (change state (vec (concat ["restore" "--"] found.paths))))]
      [else (message "Nothing to discard here")])))

(defcommand git-discard "Discard the changes in the file or section at point"
  (when-let [state (current)]
    (if-let [found (target state)]
      (quietly (fn [] (discard state found)))
      (message "Nothing to discard here"))))

(on-disk-change :git (fn [] (refresh-all)))
(on-saved :git (fn [path] (refresh-all)))
