(module
  (import (ox string)
          (git status))
  (export render matching))

(def groups [[:local "Local branches"] [:remote "Remote branches"] [:tag "Tags"]])

(def (matching typed branch)
  (or (string/blank? typed)
      (string/includes? (string/lower branch.name) (string/lower typed))))

(def (track branch)
  (cond
    [branch.gone [[" gone" :error]]]
    [branch.upstream
     (concat [[$" {branch.upstream}" :comment] " "]
             [[$"↑{branch.ahead}" (if (> branch.ahead 0) :success :comment)] " "
              [$"↓{branch.behind}" (if (> branch.behind 0) :warning :comment)]])]
    [else []]))

(def (width segments)
  (reduce + 0 (map (fn [segment] (count (if (string? segment) segment (nth segment 0)))) segments)))

(def (pad segments column)
  (conj segments (string/repeat " " (max 2 (- column (width segments))))))

(def date-width 14)

(def (lead sizes)
  (+ 6 sizes.name sizes.id 2 (if (> sizes.track 0) (+ sizes.track 2) 0)))

(def (row branch sizes now)
  (let [mark (if branch.head ["● " :success] "  ")
        named (pad ["  " mark [(status/shorten branch.name sizes.name) (if branch.head :heading :link)]]
                   (+ 6 sizes.name))
        start (pad (conj named [branch.id :constant]) (+ 6 sizes.name sizes.id 2))
        start (if (> sizes.track 0) (pad (concat start (track branch)) (lead sizes)) start)
        line (pad (conj start (status/shorten branch.summary sizes.summary)) (+ (lead sizes) sizes.summary 2))]
    {line: (conj line [(status/ago branch.time now) :comment])
     kind: :branch name: branch.name ref: branch.ref group: branch.kind head: branch.head}))

(def (render branches typed closed now columns)
  (let [shown (filter (fn [branch] (matching typed branch)) branches)
        measured {name: (min 32 (apply max 0 (map (fn [branch] (count branch.name)) shown)))
                  id: (apply max 0 (map (fn [branch] (count branch.id)) shown))
                  track: (apply max 0 (map (fn [branch] (width (track branch))) shown))}
        longest (apply max 0 (map (fn [branch] (count branch.summary)) shown))
        sizes (assoc measured :summary (max 10 (min longest (- columns (lead measured) 2 date-width))))
        top (if (string/blank? typed)
              [{line: [["Type to filter, RET checks out, C-c b for more" :comment]] kind: :info}]
              [{line: [["Filter: " :comment] [typed :link] [$"  {(count shown)} found" :comment]] kind: :info}])]
    {lines: (reduce (fn [lines group]
                      (let [kind (nth group 0)
                            found (filter (fn [branch] (= branch.kind kind)) shown)]
                        (if (empty? found)
                          lines
                          (concat lines
                                  [{line: "" kind: :blank}
                                   {line: [[(nth group 1) :heading] [$" ({(count found)})" :comment]
                                           (if (get closed kind) [" …" :comment] "")]
                                    kind: :group group: kind}]
                                  (if (get closed kind) [] (map (fn [branch] (row branch sizes now)) found))))))
                    top groups)
     count: (count shown)}))
