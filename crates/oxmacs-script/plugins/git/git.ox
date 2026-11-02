(module
  (import (ox file) (ox path) (ox string) (ox time)
          (git repo)
          (git status)
          (git pane)
          (git diff)
          (git commit)
          (git log)
          (git branches)
          (git process)))

(def status-keys
  {"n" 'git-next-section "p" 'git-previous-section "TAB" 'git-toggle "RET" 'git-visit
   "g" 'git-refresh "s" 'git-stage "u" 'git-unstage "k" 'git-discard "c" 'git-commit-popup
   "l" 'git-log-popup "b" 'git-branch-popup "y" 'git-branches "$" 'git-process
   "q" 'quit-window})

(def pane-keys
  {"n" 'git-next-hunk "p" 'git-previous-hunk "t" 'git-toggle-layout "RET" 'git-visit
   "g" 'git-refresh "s" 'git-stage "u" 'git-unstage "k" 'git-discard "c" 'git-commit-popup
   "$" 'git-process "q" 'quit-window})

(def (here)
  (if-let [git (buffer-value :git)]
    {git: git state: @git pane: false}
    (when-let [git (buffer-value :git-pane)]
      {git: git state: @git pane: true})))

(def (spot row)
  (when (and row (not (= row.kind :info)) (not (= row.kind :blank)))
    (dissoc row :line :background)))

(def (last-line)
  (line-of (buffer-size)))

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

(def (ensure-diffs git)
  (let [state @git
        missing (filter (fn [key] (nil? (get state.diffs key))) (keys (wanted state)))]
    (if (empty? missing)
      state
      (let [found (repo/diffs state.root state.snapshot
                              (reduce (fn [keys key] (assoc keys key true)) {} missing))]
        (swap! git (fn [state]
                     (assoc state :diffs (reduce (fn [diffs key] (assoc diffs key (get found key)))
                                                 state.diffs (keys found)))))))))

(def (resized git)
  (when (= (get-in @git [:pane :layout]) :split)
    (draw-pane git false nil)))

(def (draw-pane git reveal line)
  (let [state @git
        key state.pane.file
        pane-name state.pane.name]
    (when (and (or key state.pane.staged) (or reveal (buffer-live? pane-name)))
      (let [state (ensure-diffs git)
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
        (if reveal
          (with-buffer (apply open-buffer pane-name out.lines
                              (concat [:keys pane-keys :directory state.root :beside true :label "git diffs"
                                       :on-resize (fn [_width] (resized git))]
                                      placed))
            (set-buffer-value! :git-pane git))
          (apply set-lines pane-name out.lines placed))))))

(def (follow git line)
  (let [state @git]
    (when-let [row (line-item line)]
      (when (and (some? row.path) (some? row.section))
        (let [key [row.section row.path]]
          (unless (and (= key state.pane.file) (not state.pane.staged))
            (swap! git (fn [state] (assoc state :pane (assoc state.pane :file key :staged false))))
            (when (buffer-shown? state.pane.name)
              (draw-pane git false 0))))))))

(def (redraw git focus)
  (let [state (ensure-diffs git)
        name state.name
        shown (status/render state.snapshot state.folds state.diffs (now))
        here (= (buffer-name) name)
        line (if here (current-line) 0)
        wanted (or focus (and here (spot (line-item line))))]
    (set-lines name shown.lines)
    (when (and here (not (and wanted (goto-item wanted))))
      (goto-line (min line (last-line))))
    (draw-pane git false nil)))

(def (reread git)
  (let [snapshot (repo/read-status (get @git :root))]
    (swap! git
           (fn [state]
             (assoc state
                    :snapshot snapshot
                    :diffs {}
                    :pane (assoc state.pane :file
                                 (when state.pane.file
                                   (or (repo/located snapshot state.pane.file) state.pane.file))))))))

(def (refresh git)
  (reread git)
  (redraw git nil))

(def (refresh-all)
  (each [name (buffer-list)]
    (when-let [git (with-buffer name (buffer-value :git))]
      (try (refresh git) (catch error e (message e.message)))))
  nil)

(def (show found)
  (let [name $"git: {(path/name found.root)}"
        known (when (buffer-live? name) (with-buffer name (buffer-value :git)))
        git (or known
                (atom {name: name root: found.root metadata: found.metadata
                       folds: {closed: {} open: {} hidden: {}} diffs: {} snapshot: nil
                       pane: {name: $"git diff: {(path/name found.root)}" file: nil layout: :unified staged: false}}))]
    (reread git)
    (let [state (ensure-diffs git)
          shown (status/render state.snapshot state.folds state.diffs (now))]
      (with-buffer (open-buffer name shown.lines
                                :keys status-keys :directory state.root :watch state.metadata :label "git status"
                                :on-move (fn [line] (follow git line)))
        (set-buffer-value! :git git))
      (unless known
        (or (goto-item {kind: :file}) (goto-item {kind: :section}) (goto-line 0)))
      (let [row (line-item)]
        (when (and row (some? row.path))
          (swap! git (fn [state] (assoc state :pane (assoc state.pane :file [row.section row.path]))))
          (draw-pane git true 0))))))
(defcommand git-status "Show the Git status of the current repository"
  (if-let [found (repo/locate (default-directory))]
    (show found)
    (message "Not inside a Git repository")))
(def (step kinds forward)
  (let [line (current-line)
        lines (if forward
                (range (+ line 1) (inc (last-line)))
                (reverse (range 0 line)))
        at (first (filter (fn [i] (some (fn [kind] (= (get (line-item i) :kind) kind)) kinds)) lines))]
    (when (some? at) (goto-line at))))

(def items [:section :file :hunk :stash :commit])

(defcommand git-next-section "Move to the next section, file or hunk" (step items true))
(defcommand git-previous-section "Move to the previous section, file or hunk" (step items false))
(defcommand git-next-hunk "Move to the next hunk" (step [:file :hunk] true))
(defcommand git-previous-hunk "Move to the previous hunk" (step [:file :hunk] false))

(defcommand git-toggle-layout "Switch the diff between unified and side by side"
  (when-let [found (here)]
    (swap! found.git (fn [state]
                       (assoc state :pane (assoc state.pane :layout
                                                 (if (= state.pane.layout :split) :unified :split)))))
    (draw-pane found.git false nil)))

(def (flip folds key)
  (if (get folds key) (dissoc folds key) (assoc folds key true)))

(def (fold git kind change)
  (swap! git (fn [state] (assoc state :folds (assoc state.folds kind (change (get state.folds kind)))))))

(defcommand git-toggle "Fold or unfold the section, file or hunk at point"
  (when-let [git (buffer-value :git)]
    (when-let [row (line-item)]
      (let [hunk {kind: :hunk section: row.section path: row.path hunk: row.hunk}]
        (cond
          [(= row.kind :section)
           (fold git :closed (fn [closed] (flip closed row.section)))
           (redraw git nil)]
          [(= row.kind :file)
           (fold git :open (fn [open] (flip open [row.section row.path])))
           (redraw git nil)]
          [(= row.kind :hunk)
           (fold git :hidden (fn [hidden] (flip hidden [row.section row.path row.hunk])))
           (redraw git hunk)]
          [(= row.kind :line)
           (fold git :hidden (fn [hidden] (assoc hidden [row.section row.path row.hunk] true)))
           (redraw git hunk)]
          [else nil])))))

(def (row-line row)
  (if (= row.kind :pair) (or row.right row.left) row.at))

(def (hunk-line hunk at)
  (let [before (if (some? at)
                 (count (filter (fn [i] (not (= (get (nth hunk.lines i) :kind) :removed)))
                                (range at)))
                 0)]
    (+ (max 1 hunk.new-start) before)))

(def (line-number state row)
  (when (some? row.hunk)
    (hunk-line (nth (get state.diffs [row.section row.path]) row.hunk) (row-line row))))

(defcommand git-visit "Visit the file at point"
  (when-let [found (here)]
    (let [state found.state
          row (line-item)]
      (if (and row (some? row.path))
        (let [target (path/join state.root row.path)
              number (line-number state row)]
          (if (file/exists? target)
            (do
              (open-file target)
              (when number (goto-line (- number 1))))
            (message $"{row.path} is not in the working tree")))
        (message "Nothing to visit here")))))

(defcommand git-refresh "Read the repository again"
  (when-let [found (here)]
    (refresh found.git)))

(def (line-indexes row)
  (cond
    [(= row.kind :line) [row.at]]
    [(= row.kind :pair) (filter some? [row.left row.right])]
    [else nil]))

(def (same-hunk? lead row)
  (and (some? (line-indexes row))
       (= row.section lead.section) (= row.path lead.path) (= row.hunk lead.hunk)))

(def (region-target first-line final-line)
  (let [picked (keep line-item (range first-line (inc (min final-line (last-line)))))
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

(def (target)
  (if-let [region (region-lines)]
    (region-target (nth region 0) (nth region 1))
    (row-target (line-item))))

(def (acting nothing act)
  (when-let [found (here)]
    (let [chosen (target)]
      (cond
        [(nil? chosen) (message nothing)]
        [(= chosen.kind :invalid) (message "Select lines of one hunk or files of one section")]
        [else (act found.git chosen)]))))

(def (describe paths)
  (if (= (count paths) 1) (first paths) $"{(count paths)} files"))

(def (change git args)
  (deactivate-mark)
  (repo/act (get @git :root) args)
  (refresh git))

(def (apply-hunk git chosen reverse args)
  (let [state @git
        hunk (nth (get state.diffs [chosen.section chosen.path]) chosen.hunk)]
    (deactivate-mark)
    (repo/act-with state.root args (diff/patch chosen.path hunk chosen.lines reverse))
    (refresh git)))

(def (stage git chosen)
  (let [state @git
        section chosen.section]
    (cond
      [(= section :staged) (message "Already staged")]
      [(= chosen.kind :hunk)
       (do
         (when (= section :untracked)
           (repo/act state.root ["add" "--intent-to-add" "--" chosen.path]))
         (apply-hunk git chosen false ["apply" "--cached" "-"]))]
      [(and (= chosen.kind :section) (= section :untracked))
       (change git (concat ["add" "--"] state.snapshot.untracked))]
      [(and (= chosen.kind :section) (= section :unstaged))
       (change git ["add" "--update" "--" "."])]
      [(and (= chosen.kind :files) (or (= section :untracked) (= section :unstaged)))
       (change git (concat ["add" "--"] chosen.paths))]
      [else (message "Nothing to stage here")])))

(defcommand git-stage "Stage the file, section, hunk or lines at point"
  (acting "Nothing to stage here" stage))

(def (renamed-from state paths)
  (keep (fn [change] (when (some (fn [path] (= path change.path)) paths) change.from))
        state.snapshot.staged))

(def (forget git paths)
  (change git (concat (if (get-in @git [:snapshot :born]) ["reset" "-q" "--"] ["rm" "--cached" "-r" "-q" "--"])
                      paths)))

(def (unstage git chosen)
  (cond
    [(not (= chosen.section :staged)) (message "Nothing staged here")]
    [(= chosen.kind :hunk) (apply-hunk git chosen true ["apply" "--cached" "--reverse" "-"])]
    [(= chosen.kind :section) (forget git ["."])]
    [else (forget git (concat chosen.paths (renamed-from @git chosen.paths)))]))

(defcommand git-unstage "Unstage the file, section, hunk or lines at point"
  (acting "Nothing staged here" unstage))

(def (throw-away git paths)
  (let [trash (get (settings) :delete-to-trash)]
    (when (yes-or-no (if trash
                       $"Move {(describe paths)} to the trash?"
                       $"Delete {(describe paths)}?"))
      (deactivate-mark)
      (map (fn [path]
             (let [full (path/join (get @git :root) path)]
               (delete-path full)))
           paths)
      (refresh git))))

(def (discard git chosen)
  (let [state @git
        section chosen.section]
    (cond
      [(= section :staged) (message "Unstage it first with u")]
      [(and (= section :untracked) (= chosen.kind :hunk)) (message "Discard the whole file, or edit it")]
      [(= section :untracked)
       (throw-away git (if (= chosen.kind :section) state.snapshot.untracked chosen.paths))]
      [(= chosen.kind :hunk)
       (when (yes-or-no (if chosen.lines "Discard the selected lines?" "Discard this hunk?"))
         (apply-hunk git chosen true ["apply" "--reverse" "-"]))]
      [(= chosen.kind :section)
       (when (yes-or-no "Discard all unstaged changes?")
         (change git ["restore" "--" "."]))]
      [else
       (when (yes-or-no $"Discard changes to {(describe chosen.paths)}?")
         (change git (concat ["restore" "--"] chosen.paths)))])))

(defcommand git-discard "Discard the changes in the file, section, hunk or lines at point"
  (acting "Nothing to discard here" discard))

(def (head-message root)
  (string/trim-end (repo/git root ["log" "-1" "--format=%B"])))

(def (messages root)
  (keep (fn [text] (let [trimmed (string/trim text)] (when (not (= trimmed "")) trimmed)))
        (string/split (repo/git root ["log" "-n" "30" "--format=%B%x00"]) "\0")))

(def (report-commit root)
  (let [parts (string/split (string/trim (repo/git root ["log" "-1" "--format=%h%x1f%s"])) "\u{1f}" 2)]
    (message $"Committed {(nth parts 0)} {(nth parts 1)}")))

(def (show-staged git)
  (swap! git (fn [state] (assoc state :pane (assoc state.pane :staged true))))
  (draw-pane git true 0))

(def (unstaged-pane git)
  (let [state @git
        row (line-item)
        key (if (and row (some? row.path)) [row.section row.path] state.pane.file)]
    (swap! git (fn [state] (assoc state :pane (assoc state.pane :staged false :file key))))
    (draw-pane git false 0)))

(def (back-to-status git)
  (let [name (get @git :name)]
    (when (buffer-live? name)
      (switch-to-buffer name)
      (unstaged-pane git))))

(def (write-message git kind flags)
  (let [state @git
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
             args (concat ["commit" "--cleanup=strip" "-F" "-"] flags
                          (cond
                            [(= kind :amend) ["--amend"]]
                            [(= kind :reword) ["--amend" "--only"]]
                            [else []]))
             history (messages state.root)]
         (when (buffer-live? "COMMIT_EDITMSG") (close-buffer "COMMIT_EDITMSG"))
         (file/write file (commit/template text snapshot))
         (open-file file)
         (set-buffer-value! :git-commit {root: state.root git: git args: args history: history recalled: nil})
         (goto-line 0)
         (show-staged git))])))

(def (extend git flags)
  (let [state @git]
    (if (not state.snapshot.born)
      (message "There is no commit to change yet")
      (do
        (repo/act state.root (concat ["commit" "--amend" "--no-edit"] flags))
        (refresh git)
        (report-commit state.root)))))

(defcommand git-commit-popup "Commit, amend, extend or reword"
  (when-let [found (here)]
    (let [git found.git]
      (transient "Commit"
                 [{title: "Arguments"
                   switches: [["a" "--all" "Stage all modified and deleted files"]
                              ["e" "--allow-empty" "Allow a commit without changes"]
                              ["n" "--no-verify" "Skip the commit hooks"]
                              ["s" "--signoff" "Add a Signed-off-by line"]]}
                  {title: "Create"
                   actions: [["c" "Commit" (fn [flags] (write-message git :create flags))]]}
                  {title: "Edit HEAD"
                   actions: [["a" "Amend" (fn [flags] (write-message git :amend flags))]
                             ["e" "Extend" (fn [flags] (extend git flags))]
                             ["w" "Reword" (fn [flags] (write-message git :reword flags))]]}]))))

(defcommand git-commit-finish "Commit with this message"
  (if-let [found (buffer-value :git-commit)]
    (let [text (buffer-text)
          buffer (current-buffer)]
      (if (= (commit/message-text text) "")
        (message "Write a commit message first")
        (do (message "Committing…")
          (repo/act-with found.root found.args text)
          (close-buffer buffer)
          (back-to-status found.git)
          (refresh found.git)
          (report-commit found.root))))
    (message "This buffer is not a commit message")))

(defcommand git-commit-cancel "Cancel this commit"
  (if-let [found (buffer-value :git-commit)]
    (do
      (close-buffer (current-buffer))
      (back-to-status found.git)
      (message "Commit canceled"))
    (message "This buffer is not a commit message")))

(def (recall older)
  (when-let [found (buffer-value :git-commit)]
    (let [total (count found.history)
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
         (set-buffer-value! :git-commit (assoc found :recalled (if (= next :empty) nil next)))]))))
(defcommand git-commit-previous "Show the previous commit message" (recall true))
(defcommand git-commit-next "Show the next commit message" (recall false))

(defmode git-commit "Git commit"
  :highlight commit/highlight)

(def (root-here)
  (or (when-let [found (here)] found.state.root)
      (when-let [log (buffer-value :git-log)] (get @log :root))
      (buffer-value :git-root)
      (when-let [found (repo/locate (default-directory))] found.root)))

(def process-keys {"TAB" 'git-process-toggle "n" 'git-process-next "p" 'git-process-previous
                   "g" 'git-process-refresh "q" 'quit-window})

(defonce process-folds (atom {}))

(def (process-name root)
  $"git process: {(path/name root)}")

(def (draw-process root reveal)
  (let [name (process-name root)
        entries (filter (fn [entry] (= entry.root root)) @repo/history)
        out (process/render entries @process-folds (now))]
    (cond
      [reveal
       (with-buffer (open-buffer name out.lines :keys process-keys :directory root :label "git process")
         (set-buffer-value! :git-root root)
         (goto-line (last-line)))]
      [(buffer-live? name) (set-lines name out.lines)]
      [else nil])))

(def (redraw-processes)
  (each [name (buffer-list)]
    (when (string/starts-with? name "git process: ")
      (when-let [root (with-buffer name (buffer-value :git-root))]
        (draw-process root false)))))

(defcommand git-process "Show the Git commands run in this repository and their output"
  (if-let [root (root-here)]
    (draw-process root true)
    (message "Not inside a Git repository")))

(defcommand git-process-toggle "Show or hide the output of the command at point"
  (when-let [row (line-item)]
    (when-let [root (buffer-value :git-root)]
      (let [entry (find (fn [entry] (= entry.at row.at)) @repo/history)]
        (when entry
          (swap! process-folds assoc row.at (not (process/open? entry @process-folds)))
          (draw-process root false)
          (goto-item {kind: :entry at: row.at}))))))

(defcommand git-process-next "Move to the next command" (step [:entry] true))
(defcommand git-process-previous "Move to the previous command" (step [:entry] false))
(defcommand git-process-refresh "Show the latest commands"
  (when-let [root (buffer-value :git-root)]
    (draw-process root false)))

(defonce heads (atom {}))
(defonce rooted (atom {}))
(defonce showing (atom nil))

(def (root-of directory)
  (let [known (get @rooted directory)]
    (if (some? known)
      (when known known)
      (let [found (repo/locate directory)
            root (when found found.root)]
        (swap! rooted assoc directory (or root false))
        root))))

(def (head-text head)
  (str (or head.branch head.id "HEAD")
       (if head.upstream $" ↑{head.upstream.ahead} ↓{head.upstream.behind}" "")
       (if head.dirty " ●" "")))

(def (open-status root)
  (when-let [found (repo/locate root)]
    (show found)))

(def (show-head)
  (let [root @showing
        head (when root (get @heads root))]
    (set-mode-line :git (when (and (setting :mode-line) head) (head-text head))
                   :on-click (fn [] (open-status root)))))

(def (look-head root)
  (when-let [head (repo/read-head root)]
    (swap! heads assoc root head))
  (show-head))

(def (follow-head)
  (if (setting :mode-line)
    (let [root (root-of (default-directory))]
      (reset! showing root)
      (if (and root (nil? (get @heads root)))
        (look-head root)
        (show-head)))
    (set-mode-line :git nil)))

(def (heads-changed)
  (reset! heads {})
  (when (and (setting :mode-line) @showing)
    (look-head @showing)))

(def log-keys
  {"n" 'git-next-commit "p" 'git-previous-commit "RET" 'git-log-show "+" 'git-log-more
   "g" 'git-log-refresh "l" 'git-log-popup "b" 'git-branch-popup "$" 'git-process "q" 'quit-window})

(def commit-keys
  {"n" 'git-next-hunk "p" 'git-previous-hunk "t" 'git-log-toggle-layout "RET" 'git-log-visit
   "$" 'git-process "q" 'quit-window})

(def (draw-log log)
  (let [state @log
        out (log/render state.commits state.more (or (buffer-width state.name) 120) (now))]
    (set-lines state.name out.lines)))

(def (read-log log)
  (let [state @log
        found (repo/read-log state.root (+ state.limit 1) state.all)]
    (swap! log assoc :commits (take state.limit found) :more (> (count found) state.limit))))

(def (draw-commit log reveal)
  (let [state @log
        pane-name state.pane]
    (when (and state.commit (or reveal (buffer-live? pane-name)))
      (let [out (log/detail state.commit state.layout (or (buffer-width pane-name) 100) (now))]
        (if reveal
          (with-buffer (open-buffer pane-name out.lines :keys commit-keys :directory state.root
                                    :beside true :label "git commit" :line 0 :select (= reveal :select)
                                    :on-resize (fn [_width] (draw-commit log false)))
            (set-buffer-value! :git-log log))
          (set-lines pane-name out.lines :line 0))))))

(def (show-commit log id reveal)
  (let [state @log]
    (when (or reveal (not (= id state.shown)))
      (swap! log assoc :shown id :commit (repo/read-commit state.root id))
      (draw-commit log (or reveal (not (buffer-shown? state.pane)))))))

(def (follow-log log line)
  (when-let [row (line-item line)]
    (when (= row.kind :commit)
      (show-commit log row.id false))))

(def (show-log found all)
  (let [name $"git log: {(path/name found.root)}"
        known (when (buffer-live? name) (with-buffer name (buffer-value :git-log)))
        log (or known (atom {root: found.root name: name pane: $"git commit: {(path/name found.root)}"
                             limit: 200 all: all commits: [] more: false layout: :unified
                             shown: nil commit: nil}))]
    (swap! log assoc :all all)
    (read-log log)
    (let [state @log
          out (log/render state.commits state.more (or (buffer-width name) 120) (now))]
      (with-buffer (open-buffer name out.lines :keys log-keys :directory state.root :label "git log"
                                :cursor :line
                                :on-move (fn [line] (follow-log log line))
                                :on-resize (fn [_width] (draw-log log)))
        (set-buffer-value! :git-log log))
      (when (goto-item {kind: :commit})
        (show-commit log (get (line-item) :id) true)))))

(def (logged)
  (or (buffer-value :git-log) (message "Not in a Git log")))

(defcommand git-log "Show the history of the current branch with its graph"
  (if-let [found (repo/locate (default-directory))]
    (show-log found false)
    (message "Not inside a Git repository")))

(defcommand git-log-popup "Show the history of this branch or of all branches"
  (if-let [found (repo/locate (default-directory))]
    (transient "Log"
               [{title: "Log"
                 actions: [["l" "Current branch" (fn [_flags] (show-log found false))]
                           ["a" "All branches" (fn [_flags] (show-log found true))]]}])
    (message "Not inside a Git repository")))

(defcommand git-next-commit "Move to the next commit" (step [:commit] true))
(defcommand git-previous-commit "Move to the previous commit" (step [:commit] false))

(defcommand git-log-show "Show the commit at point beside the log and go to it"
  (when-let [log (logged)]
    (when-let [row (line-item)]
      (when (= row.kind :commit)
        (show-commit log row.id :select)))))

(defcommand git-log-more "Show more of the history"
  (when-let [log (logged)]
    (swap! log update :limit (fn [limit] (* limit 2)))
    (read-log log)
    (draw-log log)))

(defcommand git-log-refresh "Read the history again"
  (when-let [log (logged)]
    (read-log log)
    (draw-log log)))

(defcommand git-log-toggle-layout "Switch the commit's diff between unified and side by side"
  (when-let [log (logged)]
    (swap! log update :layout (fn [layout] (if (= layout :split) :unified :split)))
    (draw-commit log false)))

(defcommand git-log-visit "Visit the file at point as it is in the working tree"
  (when-let [log (logged)]
    (let [state @log
          row (line-item)
          file (when (and row (some? row.path) state.commit)
                 (find (fn [file] (= file.path row.path)) state.commit.files))]
      (if (nil? file)
        (message "Nothing to visit here")
        (let [target (path/join state.root file.path)]
          (if (file/exists? target)
            (do
              (open-file target)
              (when (some? row.hunk)
                (goto-line (- (hunk-line (nth file.hunks row.hunk) (row-line row)) 1))))
            (message $"{file.path} is not in the working tree")))))))

(def branch-keys
  {"RET" 'git-branch-checkout-here "TAB" 'git-branch-toggle "DEL" 'git-branch-filter-delete
   "C-g" 'git-branch-quit "C-c c" 'git-branch-new-here "C-c m" 'git-branch-rename-here
   "C-c k" 'git-branch-delete-here "C-c b" 'git-branch-popup "C-c g" 'git-branches-refresh
   "C-c $" 'git-process})

(defonce branch-views (atom {}))

(def (branches-name root)
  $"git branches: {(path/name root)}")

(def (draw-branches root reveal)
  (let [name (branches-name root)
        view (or (get @branch-views root) {branches: [] typed: "" closed: {}})
        out (branches/render view.branches view.typed view.closed (now) (or (buffer-width name) 100))]
    (cond
      [reveal
       (with-buffer (open-buffer name out.lines :keys branch-keys :directory root :label "git branches"
                                 :cursor :line :on-type (fn [text] (branch-typed root text))
                                 :on-resize (fn [_width] (draw-branches root false)))
         (set-buffer-value! :git-root root)
         (or (goto-item {kind: :branch head: true}) (goto-item {kind: :branch})))]
      [(buffer-live? name)
       (set-lines name out.lines)
       (with-buffer name
         (unless (= (get (line-item) :kind) :branch)
           (goto-item {kind: :branch})))]
      [else nil])))

(def (read-branches root)
  (swap! branch-views update root
         (fn [view] (assoc (or view {typed: "" closed: {}}) :branches (repo/read-branches root)))))

(def (branches-changed root)
  (read-branches root)
  (draw-branches root false)
  (heads-changed)
  (refresh-all)
  (redraw-processes))

(def (branch-typed root text)
  (swap! branch-views update-in [root :typed] (fn [typed] (str (or typed "") text)))
  (draw-branches root false))

(defcommand git-branches "List the branches and tags, filtering as you type"
  (if-let [root (root-here)]
    (do (read-branches root) (draw-branches root true))
    (message "Not inside a Git repository")))

(defcommand git-branches-refresh "Read the branches again"
  (when-let [root (buffer-value :git-root)]
    (read-branches root)
    (draw-branches root false)))

(defcommand git-branch-filter-delete "Take back the last character of the filter"
  (when-let [root (buffer-value :git-root)]
    (swap! branch-views update-in [root :typed]
           (fn [typed] (if (string/blank? typed) "" (string/slice typed 0 (- (count typed) 1)))))
    (draw-branches root false)))

(defcommand git-branch-quit "Clear the filter, or leave the branches"
  (when-let [root (buffer-value :git-root)]
    (if (string/blank? (get-in @branch-views [root :typed]))
      (run-command 'quit-window)
      (do (swap! branch-views assoc-in [root :typed] "")
        (draw-branches root false)))))

(defcommand git-branch-toggle "Fold or unfold the group at point"
  (when-let [root (buffer-value :git-root)]
    (when-let [row (line-item)]
      (when (= row.kind :group)
        (swap! branch-views update-in [root :closed]
               (fn [closed] (if (get closed row.group) (dissoc closed row.group) (assoc closed row.group true))))
        (draw-branches root false)
        (goto-item {kind: :group group: row.group})))))

(def (branch-names root)
  (map (fn [branch] branch.name) (repo/read-branches root)))

(def (local-names root)
  (map (fn [branch] branch.name) (filter (fn [branch] (= branch.kind :local)) (repo/read-branches root))))

(def (current-branch root)
  (get (repo/read-head root) :branch))

(def (checkout root name)
  (let [branches (repo/read-branches root)
        chosen (or (find (fn [branch] (and (= branch.name name) (= branch.kind :local))) branches)
                   (find (fn [branch] (= branch.name name)) branches))
        local (fn [wanted] (find (fn [branch] (and (= branch.kind :local) (= branch.name wanted))) branches))]
    (cond
      [(nil? chosen) (repo/act root ["switch" name])]
      [(= chosen.kind :local) (repo/act root ["switch" name])]
      [(= chosen.kind :tag) (repo/act root ["switch" "--detach" name])]
      [else
       (let [short (string/slice name (+ 1 (or (string/index-of name "/") -1)))]
         (if (local short)
           (repo/act root ["switch" short])
           (repo/act root ["switch" "--track" name])))])
    (branches-changed root)
    (message $"Switched to {(or (current-branch root) name)}")))

(def (create root name start switch)
  (if (string/blank? name)
    (message "A branch needs a name")
    (do
      (repo/act root (if switch
                       (concat ["switch" "-c" name] (if (string/blank? start) [] [start]))
                       (concat ["branch" name] (if (string/blank? start) [] [start]))))
      (branches-changed root)
      (message (if switch $"Switched to a new branch {name}" $"Made branch {name}")))))

(def (rename root old)
  (let [new (prompt $"Rename {old} to: " :initial old)]
    (if (or (string/blank? new) (= new old))
      (message "Kept the name")
      (do (repo/act root ["branch" "-m" old new])
        (branches-changed root)
        (message $"Renamed {old} to {new}")))))

(def (deleted? root name)
  (try
    (do (repo/act root ["branch" "-d" name]) true)
    (catch error e
      (if (string/includes? e.message "not fully merged")
        (when (yes-or-no $"{name} isn't merged; delete it anyway?")
          (repo/act root ["branch" "-D" name])
          true)
        (raise e)))))

(def (delete-branch root name)
  (when (yes-or-no $"Delete branch {name}?")
    (let [gone (deleted? root name)]
      (branches-changed root)
      (message (if gone $"Deleted branch {name}" $"Kept branch {name}")))))

(def (asked-branch root question)
  (prompt question :choices (branch-names root)))

(defcommand git-branch-popup "Check out, make, rename, delete or list branches"
  (if-let [root (root-here)]
    (transient "Branch"
               [{title: "Check out"
                 actions: [["b" "Branch or tag" (fn [_flags] (checkout root (asked-branch root "Check out: ")))]
                           ["c" "New branch" (fn [_flags]
                                               (let [name (prompt "New branch: ")]
                                                 (create root name (prompt $"Start {name} at: " :initial (or (current-branch root) "")
                                                                           :choices (branch-names root))
                                                         true)))]]}
                {title: "Change"
                 actions: [["n" "Make a branch" (fn [_flags]
                                                  (let [name (prompt "Make branch: ")]
                                                    (create root name (prompt $"Start {name} at: " :initial (or (current-branch root) "")
                                                                              :choices (branch-names root))
                                                            false)))]
                           ["m" "Rename" (fn [_flags]
                                           (rename root (prompt "Rename branch: " :initial (or (current-branch root) "")
                                                                :choices (local-names root))))]
                           ["k" "Delete" (fn [_flags]
                                           (delete-branch root (prompt "Delete branch: " :choices (local-names root))))]]}
                {title: "List"
                 actions: [["l" "Branches and tags" (fn [_flags] (read-branches root) (draw-branches root true))]]}])
    (message "Not inside a Git repository")))

(def (branch-here)
  (let [row (line-item)]
    (when (and row (= row.kind :branch)) row)))

(defcommand git-branch-checkout-here "Check out the branch or tag at point"
  (when-let [root (buffer-value :git-root)]
    (if-let [row (branch-here)]
      (checkout root row.name)
      (message "No branch here"))))

(defcommand git-branch-new-here "Make a branch from the one at point and switch to it"
  (when-let [root (buffer-value :git-root)]
    (let [row (branch-here)
          name (prompt (if row $"New branch from {row.name}: " "New branch: "))]
      (create root name (if row row.name "") true))))

(defcommand git-branch-rename-here "Rename the branch at point"
  (when-let [root (buffer-value :git-root)]
    (let [row (branch-here)]
      (if (and row (= row.group :local))
        (rename root row.name)
        (message "Only local branches can be renamed here")))))

(defcommand git-branch-delete-here "Delete the branch at point"
  (when-let [root (buffer-value :git-root)]
    (let [row (branch-here)]
      (if (and row (= row.group :local))
        (if row.head
          (message "Switch to another branch before deleting this one")
          (delete-branch root row.name))
        (message "Only local branches can be deleted here")))))

(on-disk-change :git (fn [_folders] (refresh-all) (heads-changed)))
(on-saved :git (fn [_path] (refresh-all) (heads-changed)))
(on-window-change :git (fn [] (follow-head)))
(on-settings-change :git (fn [] (follow-head)))

(follow-head)
