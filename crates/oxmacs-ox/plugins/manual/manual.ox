(module
  (import (ox string)
          (markdown render :as md)
          (manual start) (manual language) (manual data) (manual flow)
          (manual modules) (manual tasks) (manual editor) (manual ui)
          (manual plugins) (manual tools) (manual tutorial)))

(def buffer "*Ox manual*")

(def guide
  [start/page language/page data/page flow/page modules/page tasks/page editor/page ui/page plugins/page tools/page])

(defonce state (atom {page: "top" shown: nil back: [] ahead: [] reference: nil}))

(def (reference)
  (or (get @state :reference)
      (let [found (await (ox-reference))]
        (swap! state assoc :reference found)
        found)))

(def (module-title module)
  (cond
    [(= module "ox") "Core library"]
    [(= module "oxmacs") "Editor API"]
    [else module]))

(def (alias module)
  (last (string/split module "/")))

(def (qualified module name)
  (if (contains? #{"ox" "oxmacs"} module) name (str (alias module) "/" name)))

(def (module-intro module)
  (cond
    [(= module "ox") "Always available, in every module."]
    [(= module "oxmacs") "Always available in oxmacs: these reach the editor."]
    [else (let [spec (string/replace module "/" " ")
                short (alias module)]
            "Import it with `(import ({spec}))`, then call its names as `{short}/name`.")]))

(def (reference-text docs)
  (str "# " (module-title docs.module) "\n\n" (module-intro docs.module) "\n\n"
       (string/join "\n\n"
                    (map (fn [entry] (str "## " entry.signature "\n\n" (or entry.doc "Not documented yet.")))
                         docs.entries))
       "\n"))

(def (contents-text modules)
  (str "# The Ox manual\n\n"
       "Ox is the language oxmacs is extended in. The guide teaches it in a few short chapters; the reference lists every name in the library and the editor API, from their own documentation.\n\n"
       "`RET` follows a link, `l` goes back, `n` and `p` turn pages, `s` searches, `i` looks up a name and `x` runs the example at point in the REPL.\n\n"
       "## Guide\n\n"
       (string/join "\n" (map (fn [page] "- [{page.title}]({page.id})") guide))
       "\n\n## Reference\n\n"
       (string/join "\n" (map (fn [docs] "- [{(module-title docs.module)}](ref:{docs.module})") modules))
       "\n\n## New to oxmacs?\n\n[Start the tutorial](tutorial), also on `C-h t`.\n"))

(def (snippet line)
  (string/truncate (string/trim (string/replace (string/replace (string/replace (string/replace line "[" "") "]" "") "`" "") "#" ""))
                   60))

(def (encode term)
  (string/replace (string/replace term " " "%20") ")" "%29"))

(def (decode term)
  (string/replace (string/replace term "%20" " ") "%29" ")"))

(def (mentions? text term)
  (string/includes? (string/lower text) (string/lower term)))

(def (search-text term modules)
  (let [pages (keep (fn [page]
                      (when-let [line (first (filter (fn [line] (mentions? line term)) (string/lines page.text)))]
                        "- [{page.title}: {(snippet line)}]({page.id}?{(encode term)})"))
                    guide)
        names (mapcat (fn [docs]
                        (keep (fn [entry]
                                (when (or (mentions? entry.name term) (mentions? (or entry.doc "") term))
                                  "- [{(qualified docs.module entry.name)}](ref:{docs.module}#{entry.name})"))
                              docs.entries))
                      modules)
        found (concat pages names)]
    (str "# Search: " term "\n\n" (if (empty? found) "Nothing matches." (string/join "\n" found)) "\n")))

(def (page-of id modules)
  (cond
    [(= id "top") {title: "Contents" text: (contents-text modules)}]
    [(string/starts-with? id "ref:")
     (let [module (string/slice id 4)]
       (when-let [docs (first (filter (fn [docs] (= docs.module module)) modules))]
         {title: (module-title module) text: (reference-text docs) entries: docs.entries}))]
    [(string/starts-with? id "search:")
     {title: "Search" text: (search-text (decode (string/slice id 7)) modules)}]
    [else (first (filter (fn [page] (= page.id id)) guide))]))

(def (order modules)
  (vec (concat ["top"] (map :id guide) (map (fn [docs] (str "ref:" docs.module)) modules))))

(def (spot-of target)
  (cond
    [(string/includes? target "#") (let [[id anchor] (string/split target "#" 2)] {id: id anchor: anchor})]
    [(string/includes? target "?") (let [[id term] (string/split target "?" 2)] {id: id term: (decode term)})]
    [else {id: target}]))

(def (width)
  (min 90 (max 30 (- (or (buffer-width buffer) 80) 2))))

(def (plain pieces)
  (string/join (map (fn [piece] (if (vector? piece) (first piece) piece)) pieces)))

(def (line-for shown entries spot)
  (cond
    [spot.anchor
     (let [at (first (filter (fn [i] (= (get (nth entries i) :name) spot.anchor)) (range (count (or entries [])))))]
       (if at (get (nth shown.headings (+ at 1)) :line) 0))]
    [spot.term
     (or (first (filter (fn [i] (mentions? (plain (nth shown.lines i)) spot.term)) (range (count shown.lines))))
         0)]
    [else (or spot.line 0)]))

(def footer
  [["RET follow  TAB next link  l back  r forward  n next  p previous  u contents  s search  i look up  x run  q quit"
    :comment]])

(def reader-keys
  {"RET" 'ox-manual-follow "TAB" 'ox-manual-next-link "S-TAB" 'ox-manual-previous-link
   "n" 'ox-manual-next "p" 'ox-manual-previous "u" 'ox-manual-contents "t" 'ox-manual-contents
   "l" 'ox-manual-back "r" 'ox-manual-forward "s" 'ox-manual-search "i" 'ox-manual-lookup
   "x" 'ox-manual-run "q" 'quit-window})

(def (lines-of shown)
  (conj (conj shown.lines [""]) footer))

(def (redraw)
  (let [now @state
        page (page-of now.page (reference))]
    (when page
      (let [shown (md/render page.text (width))]
        (swap! state assoc :shown (assoc shown :entries page.entries))
        (set-lines buffer (lines-of shown))))))

(def (remember-here)
  (when (and (= (buffer-name) buffer) (get @state :shown))
    {id: (get @state :page) line: (current-line)}))

(def (show! spot {from nil})
  (let [modules (reference)
        page (page-of spot.id modules)]
    (if (nil? page)
      (message "There is no manual page called {spot.id}")
      (let [shown (md/render page.text (width))
            line (line-for shown page.entries spot)]
        (when from
          (swap! state (fn [now] (assoc now :back (conj now.back from) :ahead []))))
        (swap! state assoc :page spot.id :shown (assoc shown :entries page.entries))
        (open-buffer buffer (lines-of shown) :keys reader-keys :line line :on-resize (fn [_] (redraw)))
        (goto-line line 0)))))

(def (go! target)
  (cond
    [(or (string/starts-with? target "http://") (string/starts-with? target "https://")) (open-externally target)]
    [(= target "tutorial") (run-command 'oxmacs-tutorial)]
    [else (show! (spot-of target) :from (remember-here))]))

(defcommand oxmacs-tutorial "Learn oxmacs hands-on"
  (tutorial/open))

(defcommand ox-manual "Read the Ox manual"
  (if (buffer-exists? buffer)
    (switch-to-buffer buffer)
    (show! {id: (get @state :page)})))

(def (link-at)
  (let [shown (get @state :shown)
        line (current-line)
        column (current-column)
        here (filter (fn [link] (= link.line line)) (or shown.links []))]
    (or (first (filter (fn [link] (and (<= link.start column) (< column link.end))) here))
        (first here))))

(defcommand ox-manual-follow "Follow the link at point"
  (if-let [link (link-at)]
    (go! link.target)
    (message "No link on this line")))

(def (jump-link forward)
  (let [links (or (get-in @state [:shown :links]) [])
        line (current-line)
        column (current-column)
        after (filter (fn [link] (or (> link.line line) (and (= link.line line) (> link.start column)))) links)
        before (filter (fn [link] (or (< link.line line) (and (= link.line line) (< link.start column)))) links)
        target (if forward (or (first after) (first links)) (or (last before) (last links)))]
    (if target
      (goto-line target.line target.start)
      (message "This page has no links"))))

(defcommand ox-manual-next-link "Move to the next link" (jump-link true))
(defcommand ox-manual-previous-link "Move to the previous link" (jump-link false))

(def (turn delta)
  (let [ids (order (reference))
        here (get @state :page)
        at (first (filter (fn [i] (= (nth ids i) here)) (range (count ids))))
        id (when at (get ids (+ at delta)))]
    (if (and id (>= (+ at delta) 0))
      (show! {id: id} :from (remember-here))
      (message (if (> delta 0) "This is the last page" "This is the first page")))))

(defcommand ox-manual-next "Turn to the next page" (turn 1))
(defcommand ox-manual-previous "Turn to the previous page" (turn -1))
(defcommand ox-manual-contents "Go to the contents" (show! {id: "top"} :from (remember-here)))

(defcommand ox-manual-back "Go back to the page before"
  (let [now @state]
    (if (empty? now.back)
      (message "No earlier page")
      (let [spot (peek now.back)]
        (swap! state assoc :back (pop now.back) :ahead (conj now.ahead (remember-here)))
        (show! spot)))))

(defcommand ox-manual-forward "Go forward again after going back"
  (let [now @state]
    (if (empty? now.ahead)
      (message "No later page")
      (let [spot (peek now.ahead)]
        (swap! state assoc :ahead (pop now.ahead) :back (conj now.back (remember-here)))
        (show! spot)))))

(defcommand ox-manual-search "Search the manual"
  (let [term (string/trim (await (prompt "Search the manual: ")))]
    (unless (= term "")
      (show! {id: (str "search:" (encode term))} :from (remember-here)))))

(defcommand ox-manual-lookup "Look up an Ox name in the manual"
  (let [modules (reference)
        found (reduce (fn [found docs]
                        (reduce (fn [found entry]
                                  (assoc found (qualified docs.module entry.name) {module: docs.module name: entry.name}))
                                found docs.entries))
                      {} modules)
        chosen (await (prompt "Look up: " :choices (reduce (fn [notes [label place]] (assoc notes label place.module)) {} (entries found))))]
    (if-let [place (get found chosen)]
      (show! {id: (str "ref:" place.module) anchor: place.name} :from (remember-here))
      (message "Nothing in the manual is called {chosen}"))))

(defcommand ox-manual-run "Run the example at point in the REPL"
  (let [line (current-line)
        blocks (or (get-in @state [:shown :blocks]) [])]
    (if-let [block (first (filter (fn [block] (and (<= block.first line) (<= line block.last))) blocks))]
      (run-in-repl block.source)
      (message "Put point on an example to run it"))))
