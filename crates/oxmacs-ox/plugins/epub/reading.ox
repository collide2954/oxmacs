(module
  (import (ox string))
  (export before? page-index turn progress positions find-matches chapter-marks block-text excerpt between))

(def (before? a b)
  "True when the [block character] place a comes no later than b."
  (or (< (first a) (first b))
      (and (= (first a) (first b)) (<= (second a) (second b)))))

(def (page-index starts at)
  "The index of the page holding at, a [block character] place or :end, among the page starts."
  (if (= at :end)
    (max 0 (dec (count starts)))
    (max 0 (dec (count (take-while (fn [start] (before? start at)) starts))))))

(def (turn place by starts per-turn chapters)
  "Where turning by pages, per-turn at a time, leaves place, a map of :chapter and :at: in its chapter, at the start of the next one, at the end of the one before, or nil past either end of the book."
  (let [target (+ (page-index starts place.at) (* by per-turn))]
    (cond
      [(and (>= target 0) (< target (count starts))) (assoc place :at (nth starts target))]
      [(and (< target 0) (> place.chapter 0)) {chapter: (dec place.chapter) at: :end}]
      [(and (>= target (count starts)) (< (inc place.chapter) chapters)) {chapter: (inc place.chapter) at: [0 0]}]
      [else nil])))

(def (progress chapter page pages chapters)
  "How far through a book of chapters the page of pages in chapter is, as a whole percentage."
  (if (= chapters 0)
    0
    (round (* 100 (/ (+ chapter (/ (inc page) (max 1 pages))) chapters)))))

(def (positions text needle)
  "Every character position where needle starts in text."
  (if (= needle "")
    []
    (loop [from 0
           found []]
      (let [at (string/index-of (string/slice text from (count text)) needle)]
        (if (nil? at)
          found
          (again (+ from at (count needle)) (conj found (+ from at))))))))

(def (find-matches texts query)
  "Every place query appears in texts, a vector per chapter of block texts, ignoring case: maps of :chapter, :block, :start and :end."
  (let [needle (string/lower query)]
    (vec (mapcat (fn [chapter]
                   (let [blocks (nth texts chapter)]
                     (mapcat (fn [block]
                               (map (fn [start] {chapter: chapter block: block start: start end: (+ start (count needle))})
                                    (positions (string/lower (nth blocks block)) needle)))
                             (range (count blocks)))))
                 (range (count texts))))))

(def (chapter-marks chapter highlights matches highlight-face)
  "The marks ui/pages draws in chapter: highlights in highlight-face, those with notes in :added, and search matches in :link."
  (vec (concat (keep (fn [mark]
                       (when (= mark.chapter chapter)
                         {from: mark.from to: mark.to face: (if mark.note :added highlight-face)}))
                     highlights)
               (keep (fn [match]
                       (when (= match.chapter chapter)
                         {from: [match.block match.start] to: [match.block match.end] face: :link}))
                     matches))))

(def (block-text block)
  "The text of a block made for ui/document."
  (cond
    [block.text block.text]
    [block.runs (string/join "" (map (fn [run] (if (string? run) run run.text)) block.runs))]
    [else ""]))

(def (between texts from to)
  "The text from the place from to the place to in a chapter's block texts, blocks joined by blank lines."
  (string/join "\n\n"
               (map (fn [block]
                      (let [text (nth texts block)
                            start (if (= block (first from)) (min (second from) (count text)) 0)
                            end (if (= block (first to)) (min (second to) (count text)) (count text))]
                        (string/slice text start (max start end))))
                    (range (first from) (inc (first to))))))

(def (excerpt text start end)
  "The words around start to end in text, for a list of matches."
  (let [from (max 0 (- start 30))
        to (min (count text) (+ end 40))]
    (str (if (> from 0) "…" "") (string/trim (string/slice text from to)) (if (< to (count text)) "…" ""))))

(test "places compare by block, then character"
  (expect (before? [1 5] [1 5]))
  (expect (before? [0 9] [1 0]))
  (expect (not (before? [2 0] [1 9]))))

(test "the page holding a place is the last that starts at or before it"
  (let [starts [[0 0] [3 40] [7 0]]]
    (expect (= (page-index starts [0 0]) 0))
    (expect (= (page-index starts [3 39]) 0))
    (expect (= (page-index starts [3 40]) 1))
    (expect (= (page-index starts [9 9]) 2))
    (expect (= (page-index starts :end) 2))))

(test "turning stays in the chapter until its pages run out"
  (let [starts [[0 0] [3 0] [6 0] [9 0]]]
    (expect (= (turn {chapter: 1 at: [0 0]} 1 starts 1 3) {chapter: 1 at: [3 0]}))
    (expect (= (turn {chapter: 1 at: [3 0]} 1 starts 2 3) {chapter: 1 at: [9 0]}))
    (expect (= (turn {chapter: 1 at: [9 0]} 1 starts 1 3) {chapter: 2 at: [0 0]}))
    (expect (= (turn {chapter: 1 at: [0 0]} -1 starts 1 3) {chapter: 0 at: :end}))
    (expect (= (turn {chapter: 0 at: [0 0]} -1 starts 1 3) nil))
    (expect (= (turn {chapter: 2 at: [9 0]} 1 starts 1 3) nil))))

(test "progress counts chapters and pages"
  (expect (= (progress 0 0 4 2) 13))
  (expect (= (progress 1 3 4 2) 100)))

(test "search finds every match in every chapter, ignoring case"
  (expect (= (positions "abcabc" "bc") [1 4]))
  (expect (= (find-matches [["The Ox" "an ox"] ["no"]] "ox")
             [{chapter: 0 block: 0 start: 4 end: 6} {chapter: 0 block: 1 start: 3 end: 5}])))

(test "marks come from highlights, notes and matches in the chapter"
  (expect (= (chapter-marks 1
                            [{chapter: 1 from: [0 0] to: [0 4]} {chapter: 0 from: [0 0] to: [0 1]} {chapter: 1 from: [2 0] to: [2 3] note: "hm"}]
                            [{chapter: 1 block: 3 start: 1 end: 2}]
                            :marked)
             [{from: [0 0] to: [0 4] face: :marked}
              {from: [2 0] to: [2 3] face: :added}
              {from: [3 1] to: [3 2] face: :link}])))

(test "text between places spans blocks"
  (expect (= (block-text {kind: :paragraph runs: ["It " {text: "was" italic: true}]}) "It was"))
  (expect (= (between ["One two" "three four"] [0 4] [1 5]) "two\n\nthree"))
  (expect (= (excerpt "a short line" 2 7) "a short line")))
