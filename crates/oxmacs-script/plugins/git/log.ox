(module
  (import (ox string) (ox time)
          (git status)
          (git pane))
  (export lanes graph-width render detail ref-pieces))

(def faces [:nick-0 :nick-1 :nick-2 :nick-3 :nick-4 :nick-5 :nick-6 :nick-7])

(def (face-of color)
  (nth faces (mod color (count faces))))

(def (free lanes)
  (or (index-of lanes nil) (count lanes)))

(def (put items at value)
  (if (< at (count items))
    (assoc items at value)
    (conj (into items (repeat (- at (count items)) nil)) value)))

(def (trimmed lanes)
  (if (and (not (empty? lanes)) (nil? (last lanes))) (trimmed (pop lanes)) lanes))

(def (waiting lanes id)
  (filter (fn [i] (= (nth lanes i) id)) (range (count lanes))))

(def (claim state parent)
  (let [known (index-of state.lanes parent)]
    (if (some? known)
      [state known]
      (let [at (free state.lanes)]
        [(assoc state
                :lanes (put state.lanes at parent)
                :colors (put state.colors at state.next)
                :next (+ state.next 1))
         at]))))

(def (edge-face state lane)
  (face-of (or (get state.colors lane) 0)))

(def (step state commit)
  (let [mine (waiting state.lanes commit.id)
        fresh (empty? mine)
        node (if fresh (free state.lanes) (first mine))
        state (if fresh
                (assoc state
                       :lanes (put state.lanes node commit.id)
                       :colors (put state.colors node state.next)
                       :next (+ state.next 1))
                state)
        node-face (edge-face state node)
        through (keep (fn [i]
                        (when (and (some? (nth state.lanes i)) (not (some (fn [j] (= i j)) mine)) (not (= i node)))
                          {from: i to: i face: (edge-face state i)}))
                      (range (count state.lanes)))
        ins (map (fn [i] {from: i face: (edge-face state i)}) mine)
        cleared (reduce (fn [lanes i] (assoc lanes i nil)) state.lanes mine)
        state (assoc state :lanes (assoc cleared node nil))
        parents commit.parents
        later (when (not (empty? parents)) (index-of state.lanes (first parents)))
        pulled (and (some? later) (> later node))
        through (if pulled
                  (map (fn [edge] (if (= edge.from later) (assoc edge :to node) edge)) through)
                  through)
        state (if pulled (assoc state :lanes (assoc state.lanes later nil)) state)
        placed (reduce (fn [found parent]
                         (let [claimed (if (and (empty? found.outs) (nil? (index-of found.state.lanes parent)))
                                         [(assoc found.state
                                                 :lanes (assoc found.state.lanes node parent)
                                                 :colors (assoc found.state.colors node (get state.colors node)))
                                          node]
                                         (claim found.state parent))
                               lane (nth claimed 1)]
                           {state: (nth claimed 0)
                            outs: (conj found.outs {to: lane face: (edge-face (nth claimed 0) lane)})}))
                       {state: state outs: []}
                       parents)]
    {state: (assoc placed.state :lanes (trimmed placed.state.lanes))
     graph: {node: node face: node-face edges: (concat through ins placed.outs)}}))

(def (lanes commits)
  (loop [i 0 state {lanes: [] colors: [] next: 0} found []]
    (if (>= i (count commits))
      found
      (let [stepped (step state (nth commits i))]
        (again (+ i 1) stepped.state (conj found stepped.graph))))))

(def (graph-width graphs)
  (reduce (fn [widest graph]
            (reduce (fn [widest edge] (max widest (+ 1 (or edge.from 0)) (+ 1 (or edge.to 0))))
                    (max widest (+ graph.node 1))
                    graph.edges))
          0 graphs))

(def ref-faces {local: :link remote: :function tag: :constant detached: :warning})

(def (ref-pieces refs)
  (mapcat (fn [ref]
            [[(if ref.head $"{ref.name} ●" ref.name) (get ref-faces ref.kind :link) :pill] " "])
          refs))

(def (width segments)
  (reduce + 0 (map (fn [segment] (count (if (string? segment) segment (nth segment 0)))) segments)))

(def (pad segments column)
  (conj segments (string/repeat " " (max 2 (- column (width segments))))))

(def (row commit graph sizes now)
  (let [refs (ref-pieces commit.refs)
        start (concat [(string/repeat " " sizes.lanes) [commit.short :constant] " "] refs)
        title (status/shorten commit.summary (max 10 (- sizes.summary (width refs))))
        summary-end (+ sizes.lanes sizes.hash 1 sizes.summary 2)
        segments (pad (conj start title) summary-end)
        segments (pad (conj segments [(status/shorten commit.author sizes.author) (status/author-face commit.author)])
                      (+ summary-end sizes.author 2))]
    {line: (conj segments [(status/ago commit.time now) :comment])
     graph: graph kind: :commit id: commit.id}))

(def date-width 14)

(def (render commits more columns now)
  (if (empty? commits)
    {lines: [{line: [["No commits yet" :comment]] kind: :blank}]}
    (let [graphs (lanes commits)
          lanes-columns (+ (* 2 (graph-width graphs)) 1)
          hash (apply max 0 (map (fn [commit] (count commit.short)) commits))
          author (min 18 (apply max 0 (map (fn [commit] (count commit.author)) commits)))
          sizes {lanes: lanes-columns hash: hash author: author
                 summary: (max 20 (- columns lanes-columns hash 1 2 author 2 date-width))}]
      {lines: (concat (map (fn [i] (row (nth commits i) (nth graphs i) sizes now))
                           (range (count commits)))
                      (if more [{line: [["  + shows more commits" :comment]] kind: :more}] []))})))

(def letters {added: "Added" deleted: "Deleted" renamed: "Renamed" copied: "Copied" modified: "Modified"})

(def (when-of seconds)
  (time/format (* seconds 1000) "%Y-%m-%d %H:%M"))

(def (file-row file)
  (concat ["  " [(nth (get status/letters file.kind ["M" :warning]) 0) (nth (get status/letters file.kind ["M" :warning]) 1) :pill]
           "  " file.path]
          (if file.from [[$" ← {file.from}" :comment]] [])
          (if (= file.counted [0 0]) [] (concat ["  "] (status/stats file.counted)))))

(def (detail commit layout columns now)
  (let [message (string/lines commit.message)
        head [{line: (concat [[commit.id :constant] "  "] (ref-pieces commit.refs)) kind: :info}
              {line: [["Author: " :comment] [commit.author (status/author-face commit.author)]
                      [$" <{commit.email}>" :comment]]
               kind: :info}
              {line: [["Date:   " :comment] (when-of commit.time) [$" · {(status/ago commit.time now)}" :comment]]
               kind: :info}]
        head (if (= commit.committer commit.author)
               head
               (conj head {line: [["Commit: " :comment] commit.committer [$" <{commit.committer-email}>" :comment]]
                           kind: :info}))
        body (concat [{line: "" kind: :blank}
                      {line: [[(or (first message) "") :heading]] kind: :info}]
                     (map (fn [text] {line: text kind: :info}) (drop 1 message))
                     [{line: "" kind: :blank}
                      {line: [["Changed files" :heading] [$" ({(count commit.files)})" :comment]] kind: :info}]
                     (map (fn [file] {line: (file-row file) kind: :file section: :commit path: file.path})
                          commit.files))
        diffs (mapcat (fn [file]
                        (let [shown (pane/render {section: :commit path: file.path counted: file.counted
                                                  label: (get letters file.kind "Modified")}
                                                 file.hunks layout columns)]
                          (concat [{line: "" kind: :blank}] shown.lines)))
                      commit.files)]
    {lines: (concat head body diffs)}))
