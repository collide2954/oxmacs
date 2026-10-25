(module
  (import (ox string) (ox regex))
  (export prose highlight))

(def fence #"^\s*(```|~~~)")
(def heading #"^#{1,6}(\s|$)")
(def quoted #"^\s*>")
(def bullet #"^(\s*)([-*+]|\d+[.)])\s")
(def token #"`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|\[[^\]]+\]\([^)\s]+\)")

(def (face-of found)
  (cond ((string/starts-with? found.text "`") :constant)
    ((string/starts-with? found.text "[") :link)
    (else :keyword)))

(def (list-marker text)
  (if-let [found (regex/find bullet text)]
    (let [start (string/length (nth found.groups 0))]
      [[start (+ start (string/length (nth found.groups 1))) :keyword]])
    []))

(def (prose text)
  "The spans of a line of Markdown prose: headings, quotes, list markers, `code`, **strong** text and [links](targets)."
  (let [width (string/length text)]
    (cond ((regex/matches? heading text) [[0 width :heading]])
      ((regex/matches? quoted text) [[0 width :comment]])
      (else (vec (concat (list-marker text)
                         (map (fn [found] [found.start found.end (face-of found)])
                              (regex/find-all token text))))))))

(def (highlight _line text fenced)
  "The spans of a line of a Markdown document, and as its state whether the next line is inside a fenced code block."
  (let [width (string/length text)]
    (cond ((regex/matches? fence text) {spans: [[0 width :keyword]] state: (not fenced)})
      (fenced {spans: (if (= width 0) [] [[0 width :string]]) state: true})
      (else {spans: (prose text) state: false}))))

(test "prose shows headings, quotes, list markers, code, strong text and links"
  (expect (= (prose "# Title") [[0 7 :heading]]))
  (expect (= (prose "> said") [[0 6 :comment]]))
  (expect (= (prose "- see `x` and **y**, [z](u)")
             [[0 1 :keyword] [6 9 :constant] [14 19 :keyword] [21 27 :link]]))
  (expect (= (prose "plain") [])))

(test "fenced code is shaded until its fence closes"
  (let [found (reduce (fn [acc text]
                        (let [done (highlight 0 text acc.state)]
                          {state: done.state spans: (conj acc.spans done.spans)}))
                      {state: nil spans: []}
                      ["text" "```ox" "(def x 1)" "" "```" "more"])]
    (expect (= found.spans [[] [[0 5 :keyword]] [[0 9 :string]] [] [[0 3 :keyword]] []]))))
