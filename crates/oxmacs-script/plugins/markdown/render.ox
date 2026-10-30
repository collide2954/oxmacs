(module
  (import (ox string)
          (markdown grammar :as grammar))
  (export inline wrap render body))

(def (body text)
  "text without the YAML front matter between --- lines that opens many Markdown and R Markdown documents."
  (let [lines (string/lines text)
        end (when (and (> (count lines) 1) (= (string/trim-end (first lines)) "---"))
              (find (fn [at] (contains? #{"---" "..."} (string/trim-end (nth lines at))))
                    (range 1 (count lines))))]
    (if end (string/join "\n" (drop (+ end 1) lines)) text)))

(def (styled found)
  (cond
    [(= found.kind :code) {text: found.text face: :constant}]
    [(= found.kind :strong) {text: found.text face: :keyword}]
    [else {text: found.text face: :link target: found.target}]))

(def (inline text)
  "The runs of a line of Markdown: maps of :text with a :face for `code`, **strong** text and [links](targets), which also have a :target."
  (let [found (reduce (fn [acc match]
                        (let [plain (string/slice text acc.at match.start)
                              runs (if (= plain "") acc.runs (conj acc.runs {text: plain}))]
                          {at: match.end runs: (conj runs (styled match))}))
                      {at: 0 runs: []}
                      (grammar/tokens text))
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

(def (piece-of chunk)
  (cond
    [chunk.target [chunk.text :link :link chunk.target]]
    [chunk.face [chunk.text chunk.face]]
    [else chunk.text]))

(def (joins? previous chunk)
  (and chunk.target (vector? previous) (= (count previous) 4) (= (nth previous 3) chunk.target)))

(def (lay prefix line)
  (reduce (fn [pieces chunk]
            (let [gap (and chunk.space (> (count pieces) 1))
                  previous (peek pieces)]
              (if (joins? previous chunk)
                (conj (pop pieces) [(str (first previous) (if gap " " "") chunk.text) :link :link chunk.target])
                (conj (if gap (conj pieces " ") pieces) (piece-of chunk)))))
          [prefix]
          line))

(def (wrap runs width {lead "" indent ""})
  "Lays runs out in lines no wider than width, the first starting with lead and the others with indent: a vector of lines of pieces, ready for open-buffer, with each link one [text :link :link target] piece."
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
  (update page :lines into laid))

(def (blank page)
  (if (or (empty? page.lines) (= (peek page.lines) [""]))
    page
    (update page :lines conj [""])))

(def (flush page width)
  (let [para page.para
        page (assoc page :para nil)]
    (cond
      [(nil? para) page]
      [(= para.kind :bullet)
       (let [lead (if (contains? #{"-" "*" "+"} para.marker) "  • " $"  {para.marker} ")]
         (add-lines page (wrap (inline para.text) width
                               :lead lead
                               :indent (string/repeat " " (string/length lead)))))]
      [(= para.kind :quote) (add-lines page (wrap (inline para.text) width :lead "  │ " :indent "  │ "))]
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

(def (continues? page read)
  (and page.para
       (or (= read.kind :text)
           (and (= read.kind :quote) (= page.para.kind :quote)))))

(def (step page line width)
  (let [read (grammar/step page.open line)
        page (assoc page :open read.state)]
    (cond
      [(= read.kind :code) (update page :code conj line)]
      [(= read.kind :fence-close) (code-block page)]
      [(= read.kind :fence-open) (assoc (flush page width) :code [] :fence read.info)]
      [(= read.kind :blank) (blank (flush page width))]
      [(= read.kind :heading)
       (let [page (heading (flush page width) read.title)]
         (if (= read.level 1) (blank page) page))]
      [(continues? page read)
       (assoc-in page [:para :text] (str page.para.text " " (string/trim (or read.text line))))]
      [(= read.kind :quote) (assoc (flush page width) :para {kind: :quote text: read.text})]
      [(= read.kind :bullet) (assoc (flush page width) :para {kind: :bullet marker: read.marker text: read.text})]
      [else (assoc (flush page width) :para {kind: :text text: (string/trim line)})])))

(def (render text width)
  "Renders a page of Markdown at width: a map of :lines for open-buffer, whose links are [text :link :link target] pieces, :headings with their :line and :title, and :blocks of Ox code with their :first and :last line and :source. Headings of every level, quotes, bullets and numbered items, and ``` or ~~~ fences are understood; fences marked text, or any language but ox, show as plain text."
  (let [page (reduce (fn [page line] (step page line width))
                     {lines: [] headings: [] blocks: [] para: nil code: nil fence: nil open: nil}
                     (string/lines text))
        page (if (some? page.code) (code-block page) (flush page width))]
    (dissoc page :para :code :fence :open)))

(test "lines are read into code, strong text and links"
  (expect (= (inline "Use `x` with **care**, see [the start](start).")
             [{text: "Use "} {text: "x" face: :constant} {text: " with "} {text: "care" face: :keyword}
              {text: ", see "} {text: "the start" face: :link target: "start"} {text: "."}])))

(test "a link of several words is one link piece"
  (expect (= (first (get (render "See [the start page](start), then [more](next)." 80) :lines))
             ["" "See" " " ["the start page" :link :link "start"] "," " " "then" " " ["more" :link :link "next"] "."])))

(test "headings of any level, quotes, every kind of bullet and ~~~ fences render"
  (let [page (render "### Third\n> wise words\n* star\n2. two\n~~~text\ncode\n~~~" 40)]
    (expect (= page.headings [{line: 0 title: "Third"}]))
    (expect (= page.lines
               [[["Third" :heading]]
                ["  │ " "wise" " " "words"]
                ["  • " "star"]
                ["  2. " "two"]
                ["    " ["code" :string]]]))))

(test "front matter is left out of the page"
  (expect (= (body "---\ntitle: x\n...\n# A\n") "# A"))
  (expect (= (body "# A\n---\nb") "# A\n---\nb"))
  (expect (= (body "---\nnever closed") "---\nnever closed")))
