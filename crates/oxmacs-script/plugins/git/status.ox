(module
  (import (ox string)
          (git repo :as repo)
          (git diff :as diff))
  (export render ago shorten author-face stats))

(def path-column 60)
(def summary-column 50)

(def titles {untracked: "Untracked files" unstaged: "Unstaged changes" staged: "Staged changes"
             stashes: "Stashes" recent: "Recent commits"})

(def letters {added: ["A" :success] modified: ["M" :warning] deleted: ["D" :error]
              renamed: ["R" :link] copied: ["C" :link] type-change: ["T" :constant]
              conflict: ["U" :error]})

(def nicks [:nick-0 :nick-1 :nick-2 :nick-3 :nick-4 :nick-5 :nick-6 :nick-7])

(def (author-face name)
  (nth nicks (mod (hash name) (count nicks))))

(def (shorten text width)
  (if (<= (string/length text) width)
    text
    (str (string/slice text 0 (max 0 (- width 1))) "…")))

(def units [[60 "minute"] [3600 "hour"] [86400 "day"] [604800 "week"]
            [2629746 "month"] [31556952 "year"]])

(def (ago then now)
  (let [seconds (max 0 (- now then))
        unit (last (filter (fn [unit] (>= seconds (nth unit 0))) units))]
    (if (nil? unit)
      "just now"
      (let [n (// seconds (nth unit 0))]
        "{n} {(nth unit 1)}{(if (= n 1) "" "s")} ago"))))

(def (segment-width segment)
  (string/length (str (if (string? segment) segment (nth segment 0)))))

(def (width segments)
  (reduce + 0 (map segment-width segments)))

(def (pad segments column)
  (let [used (width segments)]
    (if (< used column) (conj segments (string/repeat " " (- column used))) segments)))

(def (pad-past segments column)
  (let [used (width segments)]
    (conj segments (string/repeat " " (if (< used column) (- column used) 2)))))

(def (add out row line)
  {lines: (conj out.lines (if (map? line) (merge line row) (assoc row :line line)))})

(def (blank out)
  (add out {kind: :blank} ""))

(def (head out snapshot)
  (let [branch (if snapshot.branch [snapshot.branch :link :pill] ["(detached)" :warning])
        commit (if (empty? snapshot.recent)
                 [["No commits yet" :comment]]
                 (let [latest (first snapshot.recent)]
                   [[latest.id :constant] " " latest.summary]))
        out (add out {kind: :info} (vec (concat [["Head:     " :comment] branch "  "] commit)))
        upstream snapshot.upstream]
    (if upstream
      (add out {kind: :info}
           [["Upstream: " :comment] [upstream.name :function :pill] "  "
            ["↑{upstream.ahead}" (if (> upstream.ahead 0) :success :comment)] " "
            ["↓{upstream.behind}" (if (> upstream.behind 0) :warning :comment)]])
      out)))

(def (heading out section n folds)
  (add out {kind: :section section: section}
       (vec (concat [[(get titles section) :heading]]
                    (if n [[" ({n})" :comment]] [])
                    (if (get folds.closed section) [[" …" :comment]] [])))))

(def (letter section kind)
  (if (= section :untracked) ["?" :comment] (get letters kind ["M" :warning])))

(def (stats counted)
  (cond
    [(= counted :binary) [["binary" :comment]]]
    [(= counted :large) [["large" :comment]]]
    [else
     (let [added (nth counted 0)
           removed (nth counted 1)]
       (vec (concat (if (> added 0) [["+{added}" :success]] [])
                    (if (and (> added 0) (> removed 0)) [" "] [])
                    (if (> removed 0) [["-{removed}" :error]] []))))]))

(def (file-line section change column counted)
  (let [shown (letter section change.kind)
        segments ["  " [(nth shown 0) (nth shown 1) :pill] "  " change.path]
        segments (if change.from (conj segments [" ← {change.from}" :comment]) segments)]
    (if (and counted (not (= counted [0 0])))
      (vec (concat (pad-past segments column) (stats counted)))
      segments)))

(def (diff-line line)
  (let [text (diff/shown line.text)]
    (cond
      [(= line.kind :added) {line: [["+" :success] text] background: :added}]
      [(= line.kind :removed) {line: [["-" :error] text] background: :removed}]
      [(= line.kind :note) [[text :comment]]]
      [else [" " text]])))

(def (hunks out section path found folds)
  (reduce (fn [out i]
            (let [hunk (nth found i)
                  out (add out {kind: :hunk section: section path: path hunk: i}
                           [[(diff/header hunk) :keyword]])]
              (if (get folds.hidden [section path i])
                out
                (reduce (fn [out j]
                          (add out {kind: :line section: section path: path hunk: i at: j}
                               (diff-line (nth hunk.lines j))))
                        out (range (count hunk.lines))))))
          out (range (count found))))

(def (file out snapshot folds diffs section change column)
  (let [key [section change.path]
        out (add out {kind: :file section: section path: change.path}
                 (file-line section change column (get snapshot.counts key)))]
    (if (get folds.open key)
      (hunks out section change.path (or (get diffs key) []) folds)
      out)))

(def (file-section out snapshot folds diffs section)
  (let [files (repo/files-of snapshot section)]
    (if (empty? files)
      out
      (let [out (heading (blank out) section (count files) folds)]
        (if (get folds.closed section)
          out
          (let [column (+ 5 (min path-column (apply max 0 (map (fn [change] (string/length change.path)) files))) 2)]
            (reduce (fn [out change] (file out snapshot folds diffs section change column))
                    out files)))))))

(def (stash-section out snapshot folds)
  (let [stashes snapshot.stashes]
    (if (empty? stashes)
      out
      (let [out (heading (blank out) :stashes (count stashes) folds)]
        (if (get folds.closed :stashes)
          out
          (reduce (fn [out i]
                    (let [stash (nth stashes i)]
                      (add out {kind: :stash index: i}
                           ["  " [stash.name :constant] "  " stash.summary])))
                  out (range (count stashes))))))))

(def (recent-section out snapshot folds now)
  (let [recent snapshot.recent]
    (if (empty? recent)
      out
      (let [out (heading (blank out) :recent nil folds)]
        (if (get folds.closed :recent)
          out
          (let [summary-width (min summary-column (apply max 0 (map (fn [c] (string/length c.summary)) recent)))
                author-width (apply max 0 (map (fn [c] (string/length c.author)) recent))]
            (reduce (fn [out i]
                      (let [commit (nth recent i)
                            segments ["  " [commit.id :constant] "  " (shorten commit.summary summary-width)]
                            column (+ 2 (string/length commit.id) 2 summary-width 2)
                            segments (conj (pad segments column) [commit.author (author-face commit.author)])
                            segments (pad segments (+ column author-width 2))]
                        (add out {kind: :commit index: i}
                             (conj segments [(ago commit.time now) :comment]))))
                    out (range (count recent)))))))))

(def (render snapshot folds diffs now)
  (let [out (head {lines: []} snapshot)
        out (reduce (fn [out section] (file-section out snapshot folds diffs section))
                    out [:untracked :unstaged :staged])
        out (stash-section out snapshot folds)]
    (recent-section out snapshot folds now)))
