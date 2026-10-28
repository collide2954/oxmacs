(module
  (import (ox string) (ox regex))
  (export page symbol-in symbol-at-point topic-of example-at examples))

(def backspace \u{8})

(def (runs line)
  "The text of a line of R's help as [text struck] runs, where struck text was overstruck to show it as a heading."
  (let [chars (string/chars line)
        total (count chars)]
    ((fn step [at found]
       (if (>= at total)
         found
         (let [struck (and (< (+ at 2) total) (= (nth chars (+ at 1)) backspace))
               shown (if struck (nth chars (+ at 2)) (nth chars at))
               next (if struck (+ at 3) (+ at 1))]
           (cond ((= shown backspace) (step next found))
             ((and (not (empty? found)) (= (second (last found)) struck))
              (step next (conj (pop found) [(str (first (last found)) shown) struck])))
             (else (step next (conj found [(str shown) struck])))))))
     0 [])))

(def (plain parts) (apply str (map first parts)))

(def (heading? parts)
  (and (not (empty? parts)) (second (first parts)) (string/ends-with? (string/trim-end (plain parts)) ":")))

(def (linked shown links)
  (let [cut (reduce (fn [built link]
                      (let [before (string/slice shown built.at link.start)]
                        {at: link.end
                         pieces: (-> built.pieces
                                     (conj before)
                                     (conj [(string/slice shown link.start link.end) :link :link link.topic]))}))
                    {at: 0 pieces: []}
                    links)]
    (conj cut.pieces (string/slice shown cut.at))))

(def (page text)
  "R's help text laid out for open-buffer: its lines with headings styled and the topics named in See Also as links to them, and the lines of its examples."
  (let [built (reduce
               (fn [built line]
                 (let [parts (runs line)
                       shown (plain parts)
                       number (count built.lines)]
                   (if (heading? parts)
                     (assoc built
                            :section (string/trim (string/replace (string/trim-end shown) ":" ""))
                            :lines (conj built.lines (map (fn [[piece struck]] (if struck [piece :heading] piece)) parts)))
                     (let [links (if (= built.section "See Also")
                                   (map (fn [found] {start: (+ found.start 1) end: (- found.end 1)
                                                     topic: (first found.groups)})
                                        (regex/find-all #"['‘]([\p{L}\p{N}._:]+)['’]" shown))
                                   [])]
                       (assoc built
                              :lines (conj built.lines (if (empty? links) shown (linked shown links)))
                              :examples (if (= built.section "Examples")
                                          (conj built.examples number)
                                          built.examples))))))
               {lines: [] examples: [] section: nil}
               (string/lines text))]
    {lines: built.lines examples: built.examples texts: (map plain (map runs (string/lines text)))}))

(def (topic-of text)
  (first (string/words (or (first (string/lines text)) ""))))

(def (dedented lines)
  (let [indents (keep (fn [line] (when (not (string/blank? line))
                                   (- (string/length line) (string/length (string/trim-start line)))))
                      lines)
        cut (if (empty? indents) 0 (apply min indents))]
    (string/join "\n" (map (fn [line] (if (string/blank? line) "" (string/slice line (min cut (string/length line))))) lines))))

(def (example-at page line)
  "The example paragraph around line, without its indentation, or nil outside the examples."
  (when (and (contains? (set page.examples) line) (not (string/blank? (nth page.texts line))))
    (let [inside (set page.examples)
          blank? (fn [at] (or (not (contains? inside at)) (string/blank? (nth page.texts at))))
          first-line ((fn up [at] (if (blank? (- at 1)) at (up (- at 1)))) line)
          last-line ((fn down [at] (if (blank? (+ at 1)) at (down (+ at 1)))) line)]
      (dedented (map (fn [at] (nth page.texts at)) (range first-line (+ last-line 1)))))))

(def (examples page)
  (let [lines (map (fn [at] (nth page.texts at)) page.examples)
        kept (reverse (drop-while string/blank? (reverse (drop-while string/blank? lines))))]
    (when (not (empty? kept))
      (dedented kept))))

(def (symbol-in line column)
  "The R name in line around column, or nil."
  (when-let [found (find (fn [word] (and (<= word.start column) (<= column word.end)))
                         (regex/find-all #"[\p{L}\p{N}._]+" line))]
    (when (regex/matches? #"^[^0-9_]" found.text)
      found.text)))

(def (symbol-at-point)
  (symbol-in (or (line-text (current-line)) "") (current-column)))
