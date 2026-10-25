(module
  (import (ox string) (ox regex))
  (export inline wrap render))

(def token #"`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^)\s]+)\)")

(def (styled groups)
  (cond
    [(nth groups 0) {text: (nth groups 0) face: :constant}]
    [(nth groups 1) {text: (nth groups 1) face: :keyword}]
    [else {text: (nth groups 2) face: :link target: (nth groups 3)}]))

(def (inline text)
  "The runs of a line of Markdown: maps of :text with a :face for `code`, **strong** text and [links](targets), which also have a :target."
  (let [found (reduce (fn [acc match]
                        (let [plain (string/slice text acc.at match.start)
                              runs (if (= plain "") acc.runs (conj acc.runs {text: plain}))]
                          {at: match.end runs: (conj runs (styled match.groups))}))
                      {at: 0 runs: []}
                      (regex/find-all token text))
        rest (string/slice text found.at)]
    (if (= rest "") found.runs (conj found.runs {text: rest}))))

(def (chunks runs)
  (get (reduce (fn [acc run]
                 (reduce (fn [acc [index part]]
                           (let [pending (or acc.pending (> index 0))]
                             (if (= part "")
                               (assoc acc :pending pending)
                               {pending: false
                                out: (conj acc.out (assoc run :text part :space (and pending (not (empty? acc.out)))))})))
                         acc
                         (map vector (range (count (string/split run.text " "))) (string/split run.text " "))))
               {pending: false out: []}
               runs)
       :out))

(def (words runs)
  (reduce (fn [found chunk]
            (if (or (empty? found) chunk.space)
              (conj found [chunk])
              (conj (pop found) (conj (peek found) chunk))))
          []
          (chunks runs)))

(def (width-of word)
  (reduce + 0 (map (fn [chunk] (string/length chunk.text)) word)))

(def (linked links start end target)
  (let [previous (peek links)]
    (if (and previous (= previous.target target) (<= (- start previous.end) 1))
      (conj (pop links) (assoc previous :end end))
      (conj links {start: start end: end target: target}))))

(def (lay prefix line)
  (reduce (fn [acc chunk]
            (let [gap (if (and chunk.space (> (count acc.pieces) 1)) 1 0)
                  start (+ acc.column gap)
                  end (+ start (string/length chunk.text))
                  pieces (if (= gap 1) (conj acc.pieces " ") acc.pieces)]
              {column: end
               pieces: (conj pieces (if chunk.face [chunk.text chunk.face] chunk.text))
               links: (if chunk.target (linked acc.links start end chunk.target) acc.links)}))
          {column: (string/length prefix) pieces: [prefix] links: []}
          line))

(def (wrap runs width {lead "" indent ""})
  "Lays runs out in lines no wider than width, the first starting with lead and the others with indent: a vector of maps of :pieces, ready for open-buffer, and :links with the :start, :end and :target of each link."
  (let [filled (reduce (fn [acc word]
                         (let [size (width-of word)
                               prefix (if (empty? acc.done) lead indent)
                               used (+ (string/length prefix) acc.length)]
                           (if (and (not (empty? acc.line)) (> (+ used 1 size) width))
                             {done: (conj acc.done acc.line) line: word length: size}
                             {done: acc.done
                              line: (into acc.line word)
                              length: (+ acc.length size (if (empty? acc.line) 0 1))})))
                       {done: [] line: [] length: 0}
                       (words runs))
        lines (if (empty? filled.line) filled.done (conj filled.done filled.line))]
    (map (fn [[index line]] (lay (if (= index 0) lead indent) line))
         (map vector (range (count lines)) (if (empty? lines) [[]] lines)))))

(def (add-lines page laid)
  (reduce (fn [page line]
            (let [at (count page.lines)]
              (assoc page
                     :lines (conj page.lines line.pieces)
                     :links (into page.links (map (fn [link] (assoc link :line at)) line.links)))))
          page laid))

(def (blank page)
  (if (or (empty? page.lines) (= (peek page.lines) [""]))
    page
    (update page :lines conj [""])))

(def (flush page width)
  (let [para page.para
        page (assoc page :para nil)]
    (cond
      [(nil? para) page]
      [(= para.kind :bullet) (add-lines page (wrap (inline para.text) width :lead "  • " :indent "    "))]
      [else (add-lines page (wrap (inline para.text) width))])))

(def ox-fences #{"" "ox" "clojure"})

(def (code-block page)
  (let [source page.code
        top (count page.lines)
        ox (contains? ox-fences page.fence)
        shown (if ox
                (map (fn [pieces] (vec (cons "    " pieces))) (ox-highlight source))
                (map (fn [line] ["    " [line :string]]) source))]
    (assoc page
           :code nil
           :lines (into page.lines shown)
           :blocks (if ox
                     (conj page.blocks {first: top last: (+ top (count source) -1) source: (string/join "\n" source)})
                     page.blocks))))

(def (heading page text)
  (let [page (blank page)]
    (assoc page
           :lines (conj page.lines [[text :heading]])
           :headings (conj page.headings {line: (count page.lines) title: text}))))

(def (step page line width)
  (cond
    [(some? page.code) (if (string/starts-with? line "```")
                         (code-block page)
                         (update page :code conj line))]
    [(string/starts-with? line "```") (assoc (flush page width) :code [] :fence (string/trim (string/slice line 3)))]
    [(string/blank? line) (blank (flush page width))]
    [(string/starts-with? line "# ") (blank (heading (flush page width) (string/slice line 2)))]
    [(string/starts-with? line "## ") (heading (flush page width) (string/slice line 3))]
    [(string/starts-with? line "- ") (assoc (flush page width) :para {kind: :bullet text: (string/slice line 2)})]
    [page.para (assoc-in page [:para :text] (str page.para.text " " (string/trim line)))]
    [else (assoc page :para {kind: :text text: (string/trim line)})]))

(def (render text width)
  "Renders a page of Markdown at width: a map of :lines for open-buffer, :links with their :line, :start, :end and :target, :headings with their :line and :title, and :blocks of Ox code with their :first and :last line and :source. Fences marked text, or any language but ox, show as plain text."
  (let [page (reduce (fn [page line] (step page line width))
                     {lines: [] links: [] headings: [] blocks: [] para: nil code: nil fence: nil}
                     (string/lines text))
        page (if (some? page.code) (code-block page) (flush page width))]
    (dissoc page :para :code :fence)))

(test "lines are read into code, strong text and links"
  (expect (= (inline "Use `x` with **care**, see [the start](start).")
             [{text: "Use "} {text: "x" face: :constant} {text: " with "} {text: "care" face: :keyword}
              {text: ", see "} {text: "the start" face: :link target: "start"} {text: "."}])))

(test "a link of several words is one link"
  (expect (= (get (render "See [the start page](start), then [more](next)." 80) :links)
             [{start: 4 end: 18 target: "start" line: 0} {start: 25 end: 29 target: "next" line: 0}])))
