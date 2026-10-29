(module
  (import (ox string) (ox regex))
  (export fence heading quoted bullet tokens step headings))

(def fence-line #"^\s*(`{3,}|~{3,})\s*([^`\s]*)")
(def heading-line #"^(#{1,6})(?:\s+(.*?))?\s*#*\s*$")
(def quote-line #"^\s*>\s?(.*)$")
(def bullet-line #"^(\s*)([-*+]|\d+[.)])\s+(.*)$")
(def inline #"`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|\[([^\]]+)\]\(([^)\s]+)\)")

(def (fence text)
  "A fence line's :marker, such as ``` or ~~~, and :info, the word after it naming the code's language, or nil."
  (when-let [found (regex/find fence-line text)]
    {marker: (nth found.groups 0) info: (or (nth found.groups 1) "")}))

(def (closes? opened text)
  (when-let [found (fence text)]
    (and (= found.info "")
         (= (string/slice found.marker 0 1) (string/slice opened.marker 0 1))
         (>= (string/length found.marker) (string/length opened.marker)))))

(def (heading text)
  "A heading line's :level, 1 to 6, and :title, or nil."
  (when-let [found (regex/find heading-line text)]
    {level: (string/length (nth found.groups 0)) title: (or (nth found.groups 1) "")}))

(def (quoted text)
  "A quoted line's :text after its >, or nil."
  (when-let [found (regex/find quote-line text)]
    {text: (nth found.groups 0)}))

(def (bullet text)
  "A list item's :marker, such as - or 1., its :start and :end columns and its :text, or nil."
  (when-let [found (regex/find bullet-line text)]
    (let [start (string/length (nth found.groups 0))
          marker (nth found.groups 1)]
      {marker: marker start: start end: (+ start (string/length marker)) text: (nth found.groups 2)})))

(def (token-of found)
  (let [groups found.groups
        span {start: found.start end: found.end}]
    (cond
      [(nth groups 0) (assoc span :kind :code :text (nth groups 0))]
      [(nth groups 1) (assoc span :kind :strong :text (nth groups 1))]
      [(nth groups 2) (assoc span :kind :strong :text (nth groups 2))]
      [else (assoc span :kind :link :text (nth groups 3) :target (nth groups 4))])))

(def (tokens text)
  "The inline tokens of a line: maps of :start, :end, :kind (:code, :strong or :link), :text and, for links, :target."
  (vec (map token-of (regex/find-all inline text))))

(def (step state text)
  "Reads one line of a document, given the state the line before left: nil, or the fence of the code block it is in. A map of the line's :kind (:fence-open, :fence-close, :code, :heading, :quote, :bullet, :blank or :text), the fields of that kind, and the :state for the next line."
  (cond
    [state (if (closes? state text)
             {kind: :fence-close state: nil}
             {kind: :code state: state})]
    [(fence text) (let [opened (fence text)] (assoc opened :kind :fence-open :state opened))]
    [(string/blank? text) {kind: :blank state: nil}]
    [(heading text) (assoc (heading text) :kind :heading :state nil)]
    [(quoted text) (assoc (quoted text) :kind :quote :state nil)]
    [(bullet text) (assoc (bullet text) :kind :bullet :state nil)]
    [else {kind: :text text: text state: nil}]))

(def (headings text)
  "The headings of a document outside its code blocks: maps of :line, counting from 0, :level and :title."
  (let [lines (string/lines text)]
    (get (reduce (fn [acc at]
                   (let [read (step acc.state (nth lines at))]
                     {state: read.state
                      found: (if (= read.kind :heading)
                               (conj acc.found {line: at level: read.level title: read.title})
                               acc.found)}))
                 {state: nil found: []}
                 (range (count lines)))
         :found)))

(test "a document's headings skip the ones inside code blocks"
  (expect (= (headings "# One\ntext\n```\n# not a heading\n```\n## Two")
             [{line: 0 level: 1 title: "One"} {line: 5 level: 2 title: "Two"}])))

(test "headings carry their level and title, and need a space after the hashes"
  (expect (= (heading "### Notes ##") {level: 3 title: "Notes"}))
  (expect (= (heading "#") {level: 1 title: ""}))
  (expect (= (heading "#hashtag") nil))
  (expect (= (heading "####### seven") nil)))

(test "quotes, bullets and fences are told apart"
  (expect (= (quoted "> said so") {text: "said so"}))
  (expect (= (bullet "  * item") {marker: "*" start: 2 end: 3 text: "item"}))
  (expect (= (bullet "12) twelfth") {marker: "12)" start: 0 end: 3 text: "twelfth"}))
  (expect (= (fence "~~~ r") {marker: "~~~" info: "r"}))
  (expect (= (fence "``` {{r setup}") {marker: "```" info: "{{r"})))

(test "inline tokens are code, strong text and links"
  (expect (= (tokens "a `b` **c** __d__ [e](f)")
             [{start: 2 end: 5 kind: :code text: "b"}
              {start: 6 end: 11 kind: :strong text: "c"}
              {start: 12 end: 17 kind: :strong text: "d"}
              {start: 18 end: 24 kind: :link text: "e" target: "f"}])))

(test "a fence stays open until one of its own kind closes it"
  (let [kinds (get (reduce (fn [acc text]
                             (let [read (step acc.state text)]
                               {state: read.state kinds: (conj acc.kinds read.kind)}))
                           {state: nil kinds: []}
                           ["````ox" "~~~" "```" "````" "# after"])
                   :kinds)]
    (expect (= kinds [:fence-open :code :code :fence-close :heading]))))
