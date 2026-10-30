(module
  (import (ox regex) (ox string) (ox xml)
          (epub package :as package))
  (export blocks))

(def heading-levels {"h1" 1 "h2" 2 "h3" 3 "h4" 4 "h5" 5 "h6" 6})

(def blocky
  #{"p" "div" "section" "article" "aside" "header" "footer" "main" "nav" "figure" "figcaption"
    "dl" "dt" "dd" "table" "thead" "tbody" "tfoot" "tr" "td" "th" "body" "address"})

(def skipped #{"script" "style" "head" "title"})
(def italic-tags #{"em" "i" "cite" "dfn" "var"})
(def bold-tags #{"strong" "b"})
(def code-tags #{"code" "kbd" "samp" "tt"})

(def (plain? run)
  (and (not run.italic) (not run.bold) (not run.code) (nil? run.link)))

(def (same-style? a b)
  (= (dissoc a :text) (dissoc b :text)))

(def (add-text state text style)
  (let [text (regex/replace #"\s+" text " ")
        previous (last state.runs)
        text (if (and previous (string/ends-with? previous.text " ") (string/starts-with? text " "))
               (string/slice text 1 (count text))
               text)]
    (if (= text "")
      state
      (update state :runs conj (assoc style :text text)))))

(def (cleaned runs)
  (let [merged (reduce (fn [out run]
                         (let [previous (last out)]
                           (if (and previous (same-style? previous run))
                             (conj (pop out) (assoc previous :text (str previous.text run.text)))
                             (conj out run))))
                       [] runs)
        n (count merged)
        trimmed (map (fn [at]
                       (let [run (nth merged at)
                             text (if (= at 0) (string/trim-start run.text) run.text)
                             text (if (= at (dec n)) (string/trim-end text) text)]
                         (assoc run :text text)))
                     (range n))]
    (vec (keep (fn [run]
                 (cond
                   [(= run.text "") nil]
                   [(plain? run) run.text]
                   [else run]))
               trimmed))))

(def (flush state kind)
  (let [runs (cleaned state.runs)
        state (assoc state :runs [])]
    (if (empty? runs)
      state
      (let [shape (if (and state.verse (= kind.kind :paragraph)) {kind: :verse} kind)]
        (update state :blocks conj (assoc shape :runs runs))))))

(def (external? href)
  (regex/matches? #"^[a-zA-Z][a-zA-Z0-9+.-]*:" href))

(def (linked entry href)
  (if (external? href)
    href
    (let [part (package/fragment href)
          place (if (string/starts-with? href "#") entry (package/resolve (package/folder-of entry) href))]
      (if part (str place "#" part) place))))

(def (styled style tag node entry)
  (cond
    [(contains? italic-tags tag) (assoc style :italic true)]
    [(contains? bold-tags tag) (assoc style :bold true)]
    [(contains? code-tags tag) (assoc style :code true)]
    [(and (= tag "a") (get node.attrs :href)) (assoc style :link (linked entry (get node.attrs :href)))]
    [else style]))

(def (inner kind)
  (if (contains? #{:quote :center :item} kind.kind) kind {kind: :paragraph}))

(def (element? child tag)
  (and (map? child) (= (string/lower child.tag) tag)))

(def (lines-of text)
  (let [lines (string/split (string/replace text "\r" "") "\n")
        lines (if (and (not (empty? lines)) (string/blank? (first lines))) (vec (rest lines)) lines)]
    (if (and (not (empty? lines)) (string/blank? (last lines))) (pop lines) lines)))

(def (walk state node style kind entry)
  (if (string? node)
    (add-text state node style)
    (let [tag (string/lower node.tag)
          id (get node.attrs :id)
          state (if id (update state :ids assoc id (count state.blocks)) state)
          into (fn [state shape]
                 (let [state (assoc (flush state kind) :verse false)
                       state (reduce (fn [state child] (walk state child style shape entry)) state node.children)]
                   (assoc (flush state shape) :verse false)))]
      (cond
        [(contains? skipped tag) state]
        [(= tag "br") (assoc (flush (assoc state :verse true) kind) :verse true)]
        [(or (= tag "img") (= tag "image"))
         (let [source (or (get node.attrs :src) (get node.attrs :href))
               state (flush state kind)]
           (if source
             (update state :blocks conj {kind: :image
                                         src: (package/resolve (package/folder-of entry) source)
                                         alt: (or (get node.attrs :alt) "")})
             state))]
        [(= tag "hr") (update (flush state kind) :blocks conj {kind: :rule})]
        [(= tag "pre")
         (reduce (fn [state line] (update state :blocks conj {kind: :pre text: line}))
                 (flush state kind)
                 (lines-of (xml/text node)))]
        [(contains? heading-levels tag) (into state {kind: :heading level: (get heading-levels tag)})]
        [(= tag "blockquote") (into state {kind: :quote})]
        [(= tag "center") (into state {kind: :center})]
        [(or (= tag "ul") (= tag "ol"))
         (let [depth (if (= kind.kind :item) (inc kind.depth) 0)
               items (filter (fn [child] (element? child "li")) node.children)
               ordered (= tag "ol")
               state (flush state kind)]
           (reduce (fn [state [at item]]
                     (let [marker (if ordered $"{(inc at)}." (if (even? depth) "•" "◦"))]
                       (walk state item style {kind: :item marker: marker depth: depth} entry)))
                   state
                   (zip (range (count items)) items)))]
        [(= tag "li") (into state (if (= kind.kind :item) kind {kind: :item marker: "•" depth: 0}))]
        [(contains? blocky tag) (into state (inner kind))]
        [else (reduce (fn [state child] (walk state child (styled style tag node entry) kind entry))
                      state
                      node.children)]))))

(def (blocks root entry)
  "The blocks of the XHTML document root, read from the zip entry entry, for ui/document, with image sources as entries; and :ids, each element id mapped to the index of the block it starts."
  (let [body (or (xml/find root "body") root)
        state (walk {blocks: [] runs: [] ids: {} verse: false} body {} {kind: :paragraph} entry)
        state (flush state {kind: :paragraph})]
    {blocks: state.blocks ids: state.ids}))

(def (read text)
  (blocks (xml/parse text :html true) "OEBPS/text/one.xhtml"))

(test "paragraphs keep their emphasis and links with whitespace run together"
  (expect (= (get (read "<html><body><p>  It   <em>was</em> a\n <a href=\"two.xhtml#s\">dark</a> <b><i>night</i></b>. </p></body></html>") :blocks)
             [{kind: :paragraph
               runs: ["It " {italic: true text: "was"} " a " {link: "OEBPS/text/two.xhtml#s" text: "dark"} " "
                      {bold: true italic: true text: "night"} "."]}])))

(test "headings, quotes, rules and centered text are their own blocks"
  (expect (= (get (read "<body><h2 id=\"c\">Two</h2><blockquote><p>Said.</p></blockquote><hr/><center>Fin</center></body>") :blocks)
             [{kind: :heading level: 2 runs: ["Two"]}
              {kind: :quote runs: ["Said."]}
              {kind: :rule}
              {kind: :center runs: ["Fin"]}])))

(test "lists number their items and nest"
  (expect (= (get (read "<body><ol><li>One<ul><li>Inner</li></ul></li><li>Two</li></ol></body>") :blocks)
             [{kind: :item marker: "1." depth: 0 runs: ["One"]}
              {kind: :item marker: "◦" depth: 1 runs: ["Inner"]}
              {kind: :item marker: "2." depth: 0 runs: ["Two"]}])))

(test "line breaks keep verse lines and pre keeps its lines"
  (expect (= (get (read "<body><p>Tyger Tyger,<br/>burning bright</p><pre>\nfn main() {\n}\n</pre></body>") :blocks)
             [{kind: :verse runs: ["Tyger Tyger,"]}
              {kind: :verse runs: ["burning bright"]}
              {kind: :pre text: "fn main() {"}
              {kind: :pre text: "}"}])))

(test "images point at their entries and ids at their blocks"
  (let [read (read "<body><p>a</p><div id=\"pic\"><img src=\"../images/x.png\" alt=\"X\"/></div><p id=\"end\">b</p><script>no()</script></body>")]
    (expect (= read.blocks
               [{kind: :paragraph runs: ["a"]}
                {kind: :image src: "OEBPS/images/x.png" alt: "X"}
                {kind: :paragraph runs: ["b"]}]))
    (expect (= read.ids {"pic" 1 "end" 2}))))
