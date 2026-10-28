(module
  (import (ox path) (ox string) (ox ui) (ox pdf)
          (pdf reading :as reading)))

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(def (current)
  (buffer-value :pdf))

(def (show-place)
  (let [state (current)
        doc (when state @state)]
    (set-mode-line :pdf (when (and doc doc.pages) (reading/label doc)))))

(def (move state to)
  (swap! state (fn [doc] (assoc doc :go (assoc to :id (inc (or (get doc.go :id) 0)))))))

(def (start-zoom)
  (cond
    [(= (setting :zoom) :actual) 1]
    [(= (setting :zoom) :fit-page) :fit-page]
    [else :fit-width]))

(def (viewed state page percent)
  (swap! state assoc :page page :percent percent)
  (show-place))

(def (follow state target)
  (cond
    [(number? target) (move state {page: target})]
    [(reading/safe-link? target) (open-externally target)]
    [else (message "Link: {target}")]))

(def (render state)
  (fn []
    (let [doc @state]
      (cond
        [doc.error
         (ui/column (ui/space 40)
                    (ui/text doc.error :face :warning)
                    (when doc.missing
                      (ui/button "Open the tool list" :on-press (fn [] (run-command 'list-tools))))
                    :spacing 14)]
        [(nil? doc.pages) (ui/column (ui/space 40) (ui/text "Opening {(path/name doc.path)}…" :face :comment))]
        [else
         (ui/pdf doc.path
                 :revision doc.revision
                 :zoom doc.zoom
                 :continuous doc.continuous
                 :dark doc.dark
                 :marks (reading/marks (or doc.matches []) doc.match (or doc.page 0))
                 :go doc.go
                 :on-view (fn [page percent] (viewed state page percent))
                 :on-select (fn [text] (swap! state assoc :selection text))
                 :on-link (fn [target] (follow state target)))]))))

(def doc-keys
  {"n" 'pdf-next-page "<right>" 'pdf-next-page "p" 'pdf-previous-page "<left>" 'pdf-previous-page
   "SPC" 'pdf-scroll-page-down "C-v" 'pdf-scroll-page-down "<next>" 'pdf-scroll-page-down
   "DEL" 'pdf-scroll-page-up "S-SPC" 'pdf-scroll-page-up "M-v" 'pdf-scroll-page-up "<prior>" 'pdf-scroll-page-up
   "C-n" 'pdf-scroll-down "<down>" 'pdf-scroll-down "C-p" 'pdf-scroll-up "<up>" 'pdf-scroll-up
   "M-<" 'pdf-first-page "<home>" 'pdf-first-page "M->" 'pdf-last-page "<end>" 'pdf-last-page
   "M-g g" 'pdf-goto-page "M-g M-g" 'pdf-goto-page
   "+" 'pdf-zoom-in "-" 'pdf-zoom-out "=" 'pdf-actual-size "W" 'pdf-fit-width "H" 'pdf-fit-page
   "c" 'pdf-toggle-continuous "m" 'pdf-theme-colors "o" 'pdf-outline "M-g i" 'pdf-outline
   "s" 'pdf-search "C-s" 'pdf-search-next "C-r" 'pdf-search-previous "M-w" 'pdf-copy
   "i" 'pdf-info "g" 'pdf-reload "q" 'quit-window})

(def (library)
  (pdf/library (setting :library)))

(def (load state)
  (let [doc @state]
    (try
      (library)
      (let [read (await (pdf/open doc.path))
            changed (and doc.revision (not (= doc.revision read.revision)))]
        (swap! state (fn [doc]
                       (assoc doc :pages read.pages :revision read.revision :title read.title
                              :author read.author :outline read.outline :error nil :missing false
                              :page (min (or doc.page 0) (max 0 (dec read.pages)))
                              :matches (when (not changed) doc.matches)
                              :match (when (not changed) doc.match))))
        (show-place)
        changed)
      (catch error e
        (swap! state assoc :error e.message :missing (nil? (library)))
        false))))

(def (open-document place)
  (let [state (atom {path: place zoom: (start-zoom) continuous: (setting :continuous)
                     dark: (setting :theme-colors)})]
    (with-buffer (open-view (path/name place) (render state) :keys doc-keys :file place)
      (set-buffer-value! :pdf state))
    (load state)
    (let [doc @state]
      (when doc.pages
        (message "{(or doc.title (path/stem place))}{(if doc.author " by {doc.author}" "")} · {doc.pages} pages")))))

(on-open-file "pdf" (fn [place] (open-document place)))

(on-disk-change :pdf
                (fn []
                  (each [name (buffer-list)]
                    (when-let [state (with-buffer name (buffer-value :pdf))]
                      (when (load state) (message "Reloaded {name}"))))))

(on-window-change :pdf (fn [] (show-place)))

(def (with-doc work)
  (if-let [state (current)]
    (let [doc @state]
      (if doc.pages
        (work state doc)
        (message (or doc.error "The document isn't open yet"))))
    (message "This isn't a PDF")))

(def (to-page state doc page)
  (move state {page: (max 0 (min page (dec doc.pages)))}))

(defcommand pdf-next-page "Go to the next page"
  (with-doc (fn [state doc]
              (if (< (inc (or doc.page 0)) doc.pages)
                (to-page state doc (inc (or doc.page 0)))
                (message "This is the last page")))))

(defcommand pdf-previous-page "Go to the previous page"
  (with-doc (fn [state doc]
              (if (> (or doc.page 0) 0)
                (to-page state doc (dec doc.page))
                (message "This is the first page")))))

(defcommand pdf-scroll-down "Scroll the document down a little"
  (with-doc (fn [state _] (move state {lines: 1}))))

(defcommand pdf-scroll-up "Scroll the document up a little"
  (with-doc (fn [state _] (move state {lines: -1}))))

(defcommand pdf-scroll-page-down "Scroll down by a windowful"
  (with-doc (fn [state _] (move state {screens: 1}))))

(defcommand pdf-scroll-page-up "Scroll up by a windowful"
  (with-doc (fn [state _] (move state {screens: -1}))))

(defcommand pdf-first-page "Go to the first page"
  (with-doc (fn [state doc] (to-page state doc 0))))

(defcommand pdf-last-page "Go to the last page"
  (with-doc (fn [state doc] (to-page state doc (dec doc.pages)))))

(defcommand pdf-goto-page "Go to a page by its number"
  (with-doc
    (fn [state doc]
      (quietly
       (fn []
         (let [answer (await (prompt "Go to page (1-{doc.pages}): "))]
           (if-let [page (reading/page-number answer doc.pages)]
             (to-page state doc page)
             (message "There is no page {answer}"))))))))

(def (zoom-to state zoom)
  (swap! state assoc :zoom zoom))

(defcommand pdf-zoom-in "Make the pages larger"
  (with-doc (fn [state doc] (zoom-to state (reading/zoomed (or doc.percent 100) 1.25)))))

(defcommand pdf-zoom-out "Make the pages smaller"
  (with-doc (fn [state doc] (zoom-to state (reading/zoomed (or doc.percent 100) 0.8)))))

(defcommand pdf-actual-size "Show the pages at their actual size"
  (with-doc (fn [state _] (zoom-to state 1))))

(defcommand pdf-fit-width "Fit the pages to the window's width"
  (with-doc (fn [state _] (zoom-to state :fit-width))))

(defcommand pdf-fit-page "Fit whole pages in the window"
  (with-doc (fn [state _] (zoom-to state :fit-page))))

(defcommand pdf-toggle-continuous "Switch between scrolling through the pages and one page at a time"
  (with-doc (fn [state doc]
              (swap! state assoc :continuous (not doc.continuous))
              (message (if doc.continuous "One page at a time" "Scrolling through the pages")))))

(defcommand pdf-theme-colors "Switch between the theme's colors and black on white"
  (with-doc (fn [state doc] (swap! state assoc :dark (not doc.dark)))))

(defcommand pdf-outline "Jump to a heading in the document's outline"
  (with-doc
    (fn [state doc]
      (if (empty? doc.outline)
        (message "This document has no outline")
        (quietly
         (fn []
           (let [labels (reading/outline-labels doc.outline)
                 picked (await (prompt "Go to heading: " :choices labels))
                 at (find (fn [at] (= (nth labels at) picked)) (range (count labels)))]
             (when at (to-page state doc (get (nth doc.outline at) :page))))))))))

(def (show-match state by)
  (let [doc @state
        at (reading/step doc.matches doc.match (or doc.page 0) by)]
    (if (nil? at)
      (message "No match for {doc.query}")
      (let [found (nth doc.matches at)
            rect (first found.rects)]
        (swap! state assoc :match at)
        (move state {page: found.page y: (if rect (nth rect 1) 0)})
        (message "{(inc at)} of {(count doc.matches)} for {doc.query}")))))

(defcommand pdf-search "Search the document, marking every match"
  (with-doc
    (fn [state doc]
      (quietly
       (fn []
         (let [query (await (prompt "Search: " :initial (or doc.query "")))]
           (if (string/blank? query)
             (swap! state assoc :query nil :matches nil :match nil)
             (let [matches (await (pdf/search doc.path query))]
               (swap! state assoc :query query :matches matches :match nil)
               (show-match state 1)))))))))

(defcommand pdf-search-next "Go to the next match of the search, or search"
  (with-doc (fn [state doc] (if doc.query (show-match state 1) (run-command 'pdf-search)))))

(defcommand pdf-search-previous "Go to the match of the search before this one, or search"
  (with-doc (fn [state doc] (if doc.query (show-match state -1) (run-command 'pdf-search)))))

(defcommand pdf-copy "Copy the text dragged over"
  (with-doc (fn [_ doc]
              (if (string/blank? (or doc.selection ""))
                (message "Drag over some text first")
                (do (copy-text doc.selection) (message "Copied"))))))

(defcommand pdf-info "Say what document this is and where you are in it"
  (with-doc (fn [_ doc]
              (message "{(or doc.title (path/name doc.path))}{(if doc.author " by {doc.author}" "")} · page {(inc (or doc.page 0))} of {doc.pages}"))))

(defcommand pdf-reload "Read the document again"
  (with-doc (fn [state _] (quietly (fn [] (load state) (message "Read {(buffer-name)} again"))))))
