(module
  (import (ox file) (ox path) (ox string) (ox time)
          (git repo :as repo)
          (git status :as status)
          (git pane :as pane)
          (git diff :as diff)
          (git commit :as commit)))

(defonce statuses (atom {}))

(def status-keys
  {"n" 'git-next-section "p" 'git-previous-section "TAB" 'git-toggle "RET" 'git-visit
   "g" 'git-refresh "s" 'git-stage "u" 'git-unstage "k" 'git-discard "c" 'git-commit-popup
   "q" 'quit-window})

(def pane-keys
  {"n" 'git-next-hunk "p" 'git-previous-hunk "t" 'git-toggle-layout "RET" 'git-visit
   "g" 'git-refresh "s" 'git-stage "u" 'git-unstage "k" 'git-discard "c" 'git-commit-popup
   "q" 'quit-window})

(defonce pending (atom {}))

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(def (row-at rows line)
  (when (and (>= line 0) (< line (count rows)))
    (nth rows line)))

(def (state-of name)
  (get @statuses name))

(def (change-state name change)
  (swap! statuses update name (fn [state] (when state (change state))))
  (state-of name))

(def (here)
  (let [name (buffer-name)]
    (if-let [state (state-of name)]
      {state: state rows: state.rows pane: false}
      (when-let [state (first (filter (fn [state] (and state (= state.pane.name name))) (vals @statuses)))]
        {state: state rows: state.pane.rows pane: true}))))

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

(def (wanted state)
  (let [open (if state.pane.file (assoc state.folds.open state.pane.file true) state.folds.open)]
    (if (and state.pane.staged state.snapshot)
      (reduce (fn [keys change] (assoc keys [:staged change.path] true)) open state.snapshot.staged)
      open)))

(def (staged-files state)
  (map (fn [change]
         (let [key [:staged change.path]]
           {file: {section: :staged path: change.path counted: (get state.snapshot.counts key)}
            hunks: (or (get state.diffs key) [])}))
       state.snapshot.staged))

(def (ensure-diffs name)
  (let [state (state-of name)
        missing (filter (fn [key] (nil? (get state.diffs key))) (keys (wanted state)))]
    (if (empty? missing)
      state
      (let [found (repo/diffs state.root state.snapshot
                              (reduce (fn [keys key] (assoc keys key true)) {} missing))]
        (change-state name (fn [state]
                             (assoc state :diffs (reduce (fn [diffs key] (assoc diffs key (get found key)))
                                                         state.diffs (keys found)))))))))

(def (resized name)
  (when-let [state (state-of name)]
    (when (= state.pane.layout :split)
      (draw-pane name false nil))))

(def (draw-pane name reveal line)
  (let [state (state-of name)
        key state.pane.file
        pane-name state.pane.name]
    (when (and (or key state.pane.staged) (or reveal (buffer-exists? pane-name)))
      (let [state (ensure-diffs name)
            columns (or (buffer-width pane-name) 100)
            change (when key (repo/find-change state.snapshot (nth key 0) (nth key 1)))
            out (cond
                  [state.pane.staged (pane/overview (staged-files state) state.pane.layout columns)]
                  [change (pane/render {section: (nth key 0) path: (nth key 1)
                                        counted: (get state.snapshot.counts key)}
                                       (or (get state.diffs key) [])
                                       state.pane.layout
                                       columns)]
                  [else (pane/nothing (nth key 1))])
            placed (if (some? line) [:line line] [])]
        (change-state name (fn [state] (assoc state :pane (assoc state.pane :rows out.rows))))
        (if reveal
          (apply open-buffer pane-name out.lines
                 (concat [:keys pane-keys :directory state.root :beside true
                          :on-resize (fn [width] (resized name))]
                         placed))
          (apply set-lines pane-name out.lines placed))))))

(def (follow name line)
  (when-let [state (state-of name)]
    (when-let [row (row-at state.rows line)]
      (when (and (some? row.path) (some? row.section))
        (let [key [row.section row.path]]
          (unless (and (= key state.pane.file) (not state.pane.staged))
            (change-state name (fn [state] (assoc state :pane (assoc state.pane :file key :staged false))))
            (when (buffer-shown? state.pane.name)
              (draw-pane name false 0))))))))

(def (redraw name focus)
  (let [state (ensure-diffs name)
        shown (status/render state.snapshot state.folds state.diffs (now))
        here (= (buffer-name) name)
        line (if here (current-line) 0)
        wanted (or focus (and here (spot (row-at state.rows line))))]
    (change-state name (fn [state] (assoc state :rows shown.rows)))
    (set-lines name shown.lines)
    (when here
      (goto-line (or (line-of shown.rows wanted)
                     (min line (max 0 (- (count shown.rows) 1))))))
    (draw-pane name false nil)))

(def (reread name)
  (let [state (state-of name)
        snapshot (repo/read-status state.root)]
    (change-state name
                  (fn [state]
                    (assoc state
                           :snapshot snapshot
                           :diffs {}
                           :pane (assoc state.pane :file
                                        (when state.pane.file
                                          (or (repo/located snapshot state.pane.file) state.pane.file))))))))

(def (refresh name)
  (when (state-of name)
    (reread name)
    (redraw name nil)))

(def (refresh-all)
  (map (fn [name]
         (if (buffer-exists? name)
           (try (refresh name) (catch error e (message e.message)))
           (swap! statuses dissoc name)))
       (keys @statuses))
  nil)

(def (show found)
  (let [name "git: {(path/name found.root)}"
        known (state-of name)]
    (unless known
      (swap! statuses assoc name
             {name: name root: found.root metadata: found.metadata
              folds: {closed: {} open: {} hidden: {}} diffs: {} rows: [] snapshot: nil
              pane: {name: "git diff: {(path/name found.root)}" file: nil layout: :unified rows: [] staged: false}}))
    (reread name)
    (let [state (ensure-diffs name)
          shown (status/render state.snapshot state.folds state.diffs (now))
          placed (if known [] [:line (first-line shown.rows)])]
      (change-state name (fn [state] (assoc state :rows shown.rows)))
      (apply open-buffer name shown.lines
             (concat [:keys status-keys :directory state.root :watch state.metadata
                      :on-move (fn [line] (follow name line))]
                     placed))
      (let [row (row-at shown.rows (current-line))]
        (when (and row (some? row.path))
          (change-state name (fn [state] (assoc state :pane (assoc state.pane :file [row.section row.path]))))
          (draw-pane name true 0))))))

(defcommand git-status "Show the Git status of the current repository"
  (quietly
    (fn []
      (if-let [found (repo/locate (default-directory))]
        (show found)
        (message "Not inside a Git repository")))))
(def (step kinds forward)
  (when-let [found (here)]
    (let [line (current-line)
          rows found.rows
          lines (if forward
                  (range (+ line 1) (count rows))
                  (reverse (range 0 line)))
          at (first (filter (fn [i] (some (fn [kind] (= (get (nth rows i) :kind) kind)) kinds)) lines))]
      (when (some? at) (goto-line at)))))

(def items [:section :file :hunk :stash :commit])

(defcommand git-next-section "Move to the next section, file or hunk" (step items true))
(defcommand git-previous-section "Move to the previous section, file or hunk" (step items false))
(defcommand git-next-hunk "Move to the next hunk" (step [:file :hunk] true))
(defcommand git-previous-hunk "Move to the previous hunk" (step [:file :hunk] false))

(defcommand git-toggle-layout "Switch the diff between unified and side by side"
  (when-let [found (here)]
    (let [name found.state.name]
      (change-state name (fn [state]
                           (assoc state :pane (assoc state.pane :layout
                                                     (if (= state.pane.layout :split) :unified :split)))))
      (quietly (fn [] (draw-pane name false nil))))))

(def (flip folds key)
  (if (get folds key) (dissoc folds key) (assoc folds key true)))

(def (fold name kind change)
  (change-state name (fn [state] (assoc state :folds (assoc state.folds kind (change (get state.folds kind)))))))

(defcommand git-toggle "Fold or unfold the section, file or hunk at point"
  (when-let [state (state-of (buffer-name))]
    (when-let [row (row-at state.rows (current-line))]
      (let [name state.name
            hunk {kind: :hunk section: row.section path: row.path hunk: row.hunk}]
        (quietly
          (fn []
            (cond
              [(= row.kind :section)
               (fold name :closed (fn [closed] (flip closed row.section)))
               (redraw name nil)]
              [(= row.kind :file)
               (fold name :open (fn [open] (flip open [row.section row.path])))
               (redraw name nil)]
              [(= row.kind :hunk)
               (fold name :hidden (fn [hidden] (flip hidden [row.section row.path row.hunk])))
               (redraw name hunk)]
              [(= row.kind :line)
               (fold name :hidden (fn [hidden] (assoc hidden [row.section row.path row.hunk] true)))
               (redraw name hunk)]
              [else nil])))))))

(def (row-line row)
  (if (= row.kind :pair) (or row.right row.left) row.line))

(def (line-number state row)
  (when (some? row.hunk)
    (let [hunk (nth (get state.diffs [row.section row.path]) row.hunk)
          at (row-line row)
          before (if (some? at)
                   (count (filter (fn [i] (not (= (get (nth hunk.lines i) :kind) :removed)))
                                  (range at)))
                   0)]
      (+ (max 1 hunk.new-start) before))))

(defcommand git-visit "Visit the file at point"
  (when-let [found (here)]
    (let [state found.state
          row (row-at found.rows (current-line))]
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
  (when-let [found (here)]
    (quietly (fn [] (refresh found.state.name)))))

(def (line-indexes row)
  (cond
    [(= row.kind :line) [row.line]]
    [(= row.kind :pair) (filter some? [row.left row.right])]
    [else nil]))

(def (same-hunk? lead row)
  (and (some? (line-indexes row))
       (= row.section lead.section) (= row.path lead.path) (= row.hunk lead.hunk)))

(def (region-target rows first-line last-line)
  (let [picked (map (fn [i] (nth rows i)) (range first-line (+ (min last-line (- (count rows) 1)) 1)))
        lead (first picked)]
    (cond
      [(nil? lead) nil]
      [(every? (fn [row] (same-hunk? lead row)) picked)
       {kind: :hunk section: lead.section path: lead.path hunk: lead.hunk
        lines: (distinct (sort (mapcat line-indexes picked)))}]
      [(every? (fn [row] (and (= row.kind :file) (= row.section lead.section))) picked)
       {kind: :files section: lead.section paths: (map (fn [row] row.path) picked)}]
      [else {kind: :invalid}])))

(def (row-target row)
  (cond
    [(nil? row) nil]
    [(= row.kind :section) {kind: :section section: row.section}]
    [(= row.kind :file) {kind: :files section: row.section paths: [row.path]}]
    [(some (fn [kind] (= row.kind kind)) [:hunk :line :pair])
     {kind: :hunk section: row.section path: row.path hunk: row.hunk lines: nil}]
    [else nil]))

(def (target found)
  (if-let [region (region-lines)]
    (region-target found.rows (nth region 0) (nth region 1))
    (row-target (row-at found.rows (current-line)))))

(def (acting nothing act)
  (when-let [found (here)]
    (let [chosen (target found)]
      (cond
        [(nil? chosen) (message nothing)]
        [(= chosen.kind :invalid) (message "Select lines of one hunk or files of one section")]
        [else (quietly (fn [] (act found.state chosen)))]))))

(def (describe paths)
  (if (= (count paths) 1) (first paths) "{(count paths)} files"))

(def (change state args)
  (deactivate-mark)
  (repo/git state.root args)
  (refresh state.name))

(def (apply-hunk state chosen reverse args)
  (let [hunk (nth (get state.diffs [chosen.section chosen.path]) chosen.hunk)]
    (deactivate-mark)
    (repo/git-with state.root args (diff/patch chosen.path hunk chosen.lines reverse))
    (refresh state.name)))

(def (stage state chosen)
  (let [section chosen.section]
    (cond
      [(= section :staged) (message "Already staged")]
      [(= chosen.kind :hunk)
       (do
         (when (= section :untracked)
           (repo/git state.root ["add" "--intent-to-add" "--" chosen.path]))
         (apply-hunk state chosen false ["apply" "--cached" "-"]))]
      [(and (= chosen.kind :section) (= section :untracked))
       (change state (vec (concat ["add" "--"] state.snapshot.untracked)))]
      [(and (= chosen.kind :section) (= section :unstaged))
       (change state ["add" "--update" "--" "."])]
      [(and (= chosen.kind :files) (or (= section :untracked) (= section :unstaged)))
       (change state (vec (concat ["add" "--"] chosen.paths)))]
      [else (message "Nothing to stage here")])))

(defcommand git-stage "Stage the file, section, hunk or lines at point"
  (acting "Nothing to stage here" stage))

(def (renamed-from state paths)
  (keep (fn [change] (when (some (fn [path] (= path change.path)) paths) change.from))
        state.snapshot.staged))

(def (forget state paths)
  (change state (vec (concat (if state.snapshot.born ["reset" "-q" "--"] ["rm" "--cached" "-r" "-q" "--"])
                             paths))))

(def (unstage state chosen)
  (cond
    [(not (= chosen.section :staged)) (message "Nothing staged here")]
    [(= chosen.kind :hunk) (apply-hunk state chosen true ["apply" "--cached" "--reverse" "-"])]
    [(= chosen.kind :section) (forget state ["."])]
    [else (forget state (vec (concat chosen.paths (renamed-from state chosen.paths))))]))

(defcommand git-unstage "Unstage the file, section, hunk or lines at point"
  (acting "Nothing staged here" unstage))

(def (throw-away state paths)
  (let [trash (setting :discard-to-trash)]
    (when (await (yes-or-no (if trash
                              "Move {(describe paths)} to the trash?"
                              "Delete {(describe paths)}?")))
      (deactivate-mark)
      (map (fn [path]
             (let [full (path/join state.root path)]
               (await (if trash (file/trash full) (file/remove full)))))
           paths)
      (refresh state.name))))

(def (discard state chosen)
  (let [section chosen.section]
    (cond
      [(= section :staged) (message "Unstage it first with u")]
      [(and (= section :untracked) (= chosen.kind :hunk)) (message "Discard the whole file, or edit it")]
      [(= section :untracked)
       (throw-away state (if (= chosen.kind :section) state.snapshot.untracked chosen.paths))]
      [(= chosen.kind :hunk)
       (when (await (yes-or-no (if chosen.lines "Discard the selected lines?" "Discard this hunk?")))
         (apply-hunk state chosen true ["apply" "--reverse" "-"]))]
      [(= chosen.kind :section)
       (when (await (yes-or-no "Discard all unstaged changes?"))
         (change state ["restore" "--" "."]))]
      [else
       (when (await (yes-or-no "Discard changes to {(describe chosen.paths)}?"))
         (change state (vec (concat ["restore" "--"] chosen.paths))))])))

(defcommand git-discard "Discard the changes in the file, section, hunk or lines at point"
  (acting "Nothing to discard here" discard))

(def (head-message root)
  (string/trim-end (repo/git root ["log" "-1" "--format=%B"])))

(def (messages root)
  (keep (fn [text] (let [trimmed (string/trim text)] (when (not (= trimmed "")) trimmed)))
        (string/split (repo/git root ["log" "-n" "30" "--format=%B%x00"]) "\0")))

(def (report-commit root)
  (let [parts (string/split (string/trim (repo/git root ["log" "-1" "--format=%h%x1f%s"])) "\u{1f}" 2)]
    (message "Committed {(nth parts 0)} {(nth parts 1)}")))

(def (show-staged name)
  (change-state name (fn [state] (assoc state :pane (assoc state.pane :staged true))))
  (draw-pane name true 0))

(def (unstaged-pane name)
  (when-let [state (state-of name)]
    (let [row (row-at state.rows (current-line))
          key (if (and row (some? row.path)) [row.section row.path] state.pane.file)]
      (change-state name (fn [state] (assoc state :pane (assoc state.pane :staged false :file key))))
      (draw-pane name false 0))))

(def (back-to-status name)
  (when (buffer-exists? name)
    (switch-to-buffer name)
    (unstaged-pane name)))

(def (write-message name kind flags)
  (let [state (state-of name)
        snapshot state.snapshot
        nothing (and (empty? snapshot.staged)
                     (not (some (fn [flag] (or (= flag "--all") (= flag "--allow-empty"))) flags)))]
    (cond
      [(and (= kind :create) nothing)
       (message "Nothing staged to commit; stage changes with s or use -a")]
      [(and (not (= kind :create)) (not snapshot.born))
       (message "There is no commit to change yet")]
      [else
       (let [file (path/join state.metadata "COMMIT_EDITMSG")
             text (if (= kind :create) "" (head-message state.root))
             args (vec (concat ["commit" "--cleanup=strip" "-F" "-"] flags
                               (cond
                                 [(= kind :amend) ["--amend"]]
                                 [(= kind :reword) ["--amend" "--only"]]
                                 [else []])))
             history (messages state.root)]
         (when (buffer-exists? "COMMIT_EDITMSG") (close-buffer "COMMIT_EDITMSG"))
         (await (file/write file (commit/template text snapshot)))
         (swap! pending assoc file {root: state.root name: name args: args history: history recalled: nil})
         (open-file file)
         (goto-line 0)
         (show-staged name))])))

(def (extend name flags)
  (let [state (state-of name)]
    (if (not state.snapshot.born)
      (message "There is no commit to change yet")
      (do
        (repo/git state.root (vec (concat ["commit" "--amend" "--no-edit"] flags)))
        (refresh name)
        (report-commit state.root)))))

(defcommand git-commit-popup "Commit, amend, extend or reword"
  (when-let [found (here)]
    (let [name found.state.name
          act (fn [run] (fn [flags] (quietly (fn [] (run flags)))))]
      (transient "Commit"
                 [{title: "Arguments"
                   switches: [["a" "--all" "Stage all modified and deleted files"]
                              ["e" "--allow-empty" "Allow a commit without changes"]
                              ["n" "--no-verify" "Skip the commit hooks"]
                              ["s" "--signoff" "Add a Signed-off-by line"]]}
                  {title: "Create"
                   actions: [["c" "Commit" (act (fn [flags] (write-message name :create flags)))]]}
                  {title: "Edit HEAD"
                   actions: [["a" "Amend" (act (fn [flags] (write-message name :amend flags)))]
                             ["e" "Extend" (act (fn [flags] (extend name flags)))]
                             ["w" "Reword" (act (fn [flags] (write-message name :reword flags)))]]}]))))

(def (pending-here)
  (when-let [file (buffer-file)]
    (when-let [found (get @pending file)]
      [file found])))

(defcommand git-commit-finish "Commit with this message"
  (if-let [here (pending-here)]
    (let [file (nth here 0)
          found (nth here 1)
          text (buffer-text)
          buffer (buffer-name)]
      (if (= (commit/message-text text) "")
        (message "Write a commit message first")
        (quietly
          (fn []
            (message "Committing…")
            (repo/git-with found.root found.args text)
            (swap! pending dissoc file)
            (close-buffer buffer)
            (back-to-status found.name)
            (refresh found.name)
            (report-commit found.root)))))
    (message "This buffer is not a commit message")))

(defcommand git-commit-cancel "Cancel this commit"
  (if-let [here (pending-here)]
    (let [found (nth here 1)]
      (swap! pending dissoc (nth here 0))
      (close-buffer (buffer-name))
      (quietly (fn [] (back-to-status found.name)))
      (message "Commit canceled"))
    (message "This buffer is not a commit message")))

(def (recall older)
  (when-let [here (pending-here)]
    (let [file (nth here 0)
          found (nth here 1)
          total (count found.history)
          at found.recalled
          next (cond
                 [(= total 0) :none]
                 [(and (nil? at) older) 0]
                 [(nil? at) :stay]
                 [older (min (+ at 1) (- total 1))]
                 [(= at 0) :empty]
                 [else (- at 1)])]
      (cond
        [(= next :none) (message "No earlier messages")]
        [(= next :stay) nil]
        [else
         (set-buffer-text (commit/replace-message (buffer-text)
                                                  (if (= next :empty) "" (nth found.history next))))
         (goto-line 0)
         (swap! pending update file (fn [found] (assoc found :recalled (if (= next :empty) nil next))))]))))

(defcommand git-commit-previous "Show the previous commit message" (recall true))
(defcommand git-commit-next "Show the next commit message" (recall false))

(defmode git-commit "Git commit"
  :file-names ["COMMIT_EDITMSG"]
  :comment "#"
  :keys {"C-c C-c" 'git-commit-finish "C-c C-k" 'git-commit-cancel
         "M-p" 'git-commit-previous "M-n" 'git-commit-next}
  :highlight commit/highlight)

(on-disk-change :git (fn [] (refresh-all)))
(on-saved :git (fn [path] (refresh-all)))
