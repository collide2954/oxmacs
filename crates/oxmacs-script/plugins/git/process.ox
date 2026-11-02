(module
  (import (ox string)
          (git status))
  (export render open?))

(def (open? entry folds)
  (let [chosen (get folds entry.at)]
    (if (some? chosen) chosen (not (= entry.status 0)))))

(def (entry-lines entry folds now)
  (let [failed (not (= entry.status 0))
        head {line: [[(if failed $"exit {entry.status}" "ok") (if failed :error :success) :pill]
                     "  " [(str "git " (string/join " " entry.args)) :heading]
                     "  " [(status/ago (// entry.at 1000) now) :comment]]
              kind: :entry at: entry.at}]
    (if (open? entry folds)
      (cons head
            (if (string/blank? entry.output)
              [{line: [["    (no output)" :comment]] kind: :output at: entry.at}]
              (map (fn [text] {line: $"    {text}" kind: :output at: entry.at})
                   (string/lines entry.output))))
      [head])))

(def (render entries folds now)
  (if (empty? entries)
    {lines: [{line: [["Git commands that change this repository show here, with their output" :comment]]
              kind: :blank}]}
    {lines: (mapcat (fn [entry] (entry-lines entry folds now)) entries)}))
