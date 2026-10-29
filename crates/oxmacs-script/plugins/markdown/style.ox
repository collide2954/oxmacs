(module
  (import (ox string)
          (markdown grammar :as grammar))
  (export prose highlight))

(def faces {code: :constant strong: :keyword link: :link})

(def (list-marker text)
  (if-let [found (grammar/bullet text)]
    [[found.start found.end :keyword]]
    []))

(def (prose text)
  "The spans of a line of Markdown prose: headings, quotes, list markers, `code`, **strong** text and [links](targets)."
  (let [width (string/length text)]
    (cond ((grammar/heading text) [[0 width :heading]])
      ((grammar/quoted text) [[0 width :comment]])
      (else (vec (concat (list-marker text)
                         (map (fn [found] [found.start found.end (get faces found.kind)])
                              (grammar/tokens text))))))))

(def (highlight _line text state)
  "The spans of a line of a Markdown document, and as its state the fence of the code block the next line is in, or nil."
  (let [width (string/length text)
        read (grammar/step state text)
        whole (fn [face] (if (= width 0) [] [[0 width face]]))]
    {spans: (cond ((contains? #{:fence-open :fence-close} read.kind) (whole :keyword))
              ((= read.kind :code) (whole :string))
              (else (prose text)))
     state: read.state}))

(test "prose shows headings, quotes, list markers, code, strong text and links"
  (expect (= (prose "# Title") [[0 7 :heading]]))
  (expect (= (prose "###### Six") [[0 10 :heading]]))
  (expect (= (prose "> said") [[0 6 :comment]]))
  (expect (= (prose "- see `x` and **y**, [z](u)")
             [[0 1 :keyword] [6 9 :constant] [14 19 :keyword] [21 27 :link]]))
  (expect (= (prose "2. second") [[0 2 :keyword]]))
  (expect (= (prose "plain") [])))

(test "fenced code is shaded until its fence closes"
  (let [found (reduce (fn [acc text]
                        (let [done (highlight 0 text acc.state)]
                          {state: done.state spans: (conj acc.spans done.spans)}))
                      {state: nil spans: []}
                      ["text" "~~~ox" "(def x 1)" "```" "" "~~~" "more"])]
    (expect (= found.spans [[] [[0 5 :keyword]] [[0 9 :string]] [[0 3 :string]] [] [[0 3 :keyword]] []]))))
