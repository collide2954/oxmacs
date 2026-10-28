(module
  (import (ox file) (ox image) (ox json) (ox path) (ox string) (ox time) (ox ui) (ox xml) (ox zip)
          (epub package :as package)
          (epub html :as html)
          (epub reading :as reading)
          (epub library :as library)))

(defonce saved (atom nil))
(defonce shelf (atom {folder: nil books: [] width: 900}))

(def notes-buffer "*epub notes*")

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(def (places-file)
  (when-let [folder (plugin-folder 'epub)]
    (path/join folder "books.json")))

(def (remembered)
  (when (nil? @saved)
    (reset! saved (or (when-let [file (places-file)]
                        (maybe (json/read (await (file/read file)))))
                      [])))
  @saved)

(def (remembered-book place)
  (find (fn [kept] (= kept.path place)) (remembered)))

(def (save! book)
  (let [kept {path: book.path title: book.title creator: book.creator
              chapter: book.chapter at: (if (vector? book.at) book.at [0 0])
              chapters: (count book.chapters) progress: (or book.progress 0)
              size: book.size highlights: book.highlights opened: (time/now)}
        others (remove (fn [old] (= old.path book.path)) (remembered))]
    (reset! saved (vec (cons kept others)))
    (when-let [file (places-file)]
      (await (file/make-dir (path/parent file)))
      (await (file/write file (json/write @saved))))))

(def (current)
  (buffer-value :epub))

(def (chapter-title book)
  (let [entry (nth book.chapters book.chapter)
        found (when (string? entry) (find (fn [item] (= item.entry entry)) book.toc))]
    (cond
      [found found.title]
      [(map? entry) "Cover"]
      [else ""])))

(def (decoded book source)
  (maybe (await (image/decode (await (zip/read-base64 book.path source))))))

(def (load-chapter book index)
  (let [entry (nth book.chapters index)]
    (if (map? entry)
      (let [cover (decoded book entry.cover)]
        {blocks: (if cover [{kind: :image image: cover alt: "Cover"}] []) ids: {} sources: []})
      (let [read (html/blocks (xml/parse (await (zip/read book.path entry)) :html true) entry)
            blocks (vec (map (fn [block]
                               (if (= block.kind :image)
                                 (if-let [image (decoded book block.src)]
                                   {kind: :image image: image alt: block.alt}
                                   {kind: :center runs: [{text: "[{(if (string/blank? block.alt) "image" block.alt)}]" italic: true}]})
                                 block))
                             read.blocks))]
        {blocks: blocks ids: read.ids sources: (keep :src read.blocks)}))))

(def (show-progress)
  (let [state (current)
        book (when state @state)]
    (set-mode-line :epub (when (and book book.chapters)
                           "{(inc book.chapter)}/{(count book.chapters)} · {(or book.progress 0)}%"))))

(def (ensure-chapter state index)
  (let [book @state]
    (when (nil? (get book.docs index))
      (let [loaded (load-chapter book index)]
        (swap! state (fn [book]
                       (assoc book
                              :docs (assoc book.docs index (ui/document loaded.blocks))
                              :blocks (assoc book.blocks index loaded.blocks)
                              :ids (assoc book.ids index loaded.ids))))))))

(def (go state chapter at)
  (ensure-chapter state chapter)
  (swap! state (fn [book]
                 (assoc book :chapter chapter :at at :starts [[0 0]]
                        :progress (round (* 100 (/ chapter (max 1 (count book.chapters))))))))
  (show-progress))

(def (paged state starts turn)
  (swap! state (fn [book]
                 (let [at (if (= book.at :end) (last starts) book.at)
                       page (reading/page-index starts at)]
                   (assoc book :starts starts :turn turn :at at
                          :progress (reading/progress book.chapter page (count starts) (count book.chapters))))))
  (show-progress)
  (let [book @state]
    (when (vector? book.at) (quietly (fn [] (save! book))))))

(def (turn-by state by)
  (let [book @state]
    (if-let [place (reading/turn {chapter: book.chapter at: book.at} by book.starts book.turn (count book.chapters))]
      (if (= place.chapter book.chapter)
        (swap! state assoc :at place.at)
        (quietly (fn [] (go state place.chapter place.at))))
      (message (if (> by 0) "End of book" "Beginning of book")))))

(def (chapter-of book entry)
  (first (keep (fn [at] (when (= (nth book.chapters at) entry) at)) (range (count book.chapters)))))

(def (follow state href)
  (if (re-external? href)
    (open-externally href)
    (let [entry (package/resolve "" href)
          part (package/fragment href)
          index (chapter-of @state entry)]
      (if (nil? index)
        (message "This link leads outside the book: {href}")
        (quietly (fn []
                   (ensure-chapter state index)
                   (let [block (get (get @state :ids) index)
                         start (or (when part (get block part)) 0)]
                     (go state index [start 0]))))))))

(def (re-external? href)
  (or (string/starts-with? href "http:") (string/starts-with? href "https:") (string/starts-with? href "mailto:")))

(def (selected state (from nil) (to nil))
  (swap! state assoc :selection (when from [from to])))

(def (render state)
  (fn []
    (let [book @state
          doc (get book.docs book.chapter)]
      (if (nil? doc)
        (ui/column (ui/space 40) (ui/text "Opening {book.title}…" :face :comment))
        (ui/pages doc
                  :at book.at
                  :typeface (setting :typeface)
                  :size (or book.size (setting :size))
                  :spread (setting :spread)
                  :marks (reading/chapter-marks book.chapter book.highlights (or book.matches []) (setting :highlight))
                  :header [book.title (chapter-title book)]
                  :on-pages (fn [starts turn] (paged state starts turn))
                  :on-turn (fn [by] (turn-by state by))
                  :on-select (fn [& ends] (apply selected state ends))
                  :on-link (fn [href] (follow state href)))))))

(def book-keys
  {"SPC" 'epub-next-page "<next>" 'epub-next-page "<right>" 'epub-next-page "C-v" 'epub-next-page "n" 'epub-next-page
   "DEL" 'epub-previous-page "S-SPC" 'epub-previous-page "<prior>" 'epub-previous-page "<left>" 'epub-previous-page
   "M-v" 'epub-previous-page "p" 'epub-previous-page
   "]" 'epub-next-chapter "[" 'epub-previous-chapter "M-<" 'epub-beginning "M->" 'epub-end
   "o" 'epub-contents "M-g i" 'epub-contents
   "s" 'epub-search "C-s" 'epub-search-next "C-r" 'epub-search-previous
   "h" 'epub-highlight "N" 'epub-note "H" 'epub-notes "M-w" 'epub-copy
   "+" 'epub-larger "=" 'epub-larger "-" 'epub-smaller "i" 'epub-info "q" 'quit-window})

(def (open-book place)
  (let [state (atom {path: place title: (path/stem place) docs: {}})
        buffer (open-view (path/name place) (render state) :keys book-keys :file place)]
    (with-buffer buffer (set-buffer-value! :epub state))
    (try
      (let [rootfile (package/rootfile (await (zip/read place "META-INF/container.xml")))
            found (package/package (await (zip/read place rootfile)) rootfile)
            toc (cond
                  [found.nav (package/toc-from-nav (await (zip/read place found.nav)) found.nav)]
                  [found.ncx (package/toc-from-ncx (await (zip/read place found.ncx)) found.ncx)]
                  [else []])
            kept (remembered-book place)
            book {path: place title: (or found.title (path/stem place)) creator: found.creator
                  chapters: found.chapters toc: toc cover: found.cover
                  docs: {} blocks: {} ids: {} chapter: 0 at: [0 0] starts: [[0 0]] turn: 1
                  highlights: (vec (or (when kept kept.highlights) [])) size: (when kept kept.size)}
            first-chapter (when (not (empty? book.chapters)) (load-chapter book 0))
            covered (or (nil? found.cover)
                        (some (fn [source] (= source found.cover)) (if first-chapter first-chapter.sources [])))
            book (if covered
                   (assoc book
                          :docs {0 (ui/document first-chapter.blocks)}
                          :blocks {0 first-chapter.blocks}
                          :ids {0 first-chapter.ids})
                   (assoc book :chapters (vec (cons {cover: found.cover} book.chapters))))
            chapter (min (or (when kept kept.chapter) 0) (max 0 (dec (count book.chapters))))
            at (or (when kept kept.at) [0 0])]
        (reset! state book)
        (go state chapter at)
        (message "{book.title}{(if book.creator " by {book.creator}" "")}"))
      (catch error e
        (close-buffer buffer)
        (message "Couldn't open {(path/name place)}: {e.message}")))))

(on-open-file "epub" (fn [place] (open-book place)))

(def (with-book work)
  (if-let [state (current)]
    (work state @state)
    (message "This isn't a book")))

(defcommand epub-next-page "Turn to the next page"
  (with-book (fn [state _] (turn-by state 1))))

(defcommand epub-previous-page "Turn back a page"
  (with-book (fn [state _] (turn-by state -1))))

(defcommand epub-next-chapter "Go to the start of the next chapter"
  (with-book (fn [state book]
               (if (< (inc book.chapter) (count book.chapters))
                 (quietly (fn [] (go state (inc book.chapter) [0 0])))
                 (message "This is the last chapter")))))

(defcommand epub-previous-chapter "Go to the start of this chapter, or the one before"
  (with-book (fn [state book]
               (cond
                 [(> (reading/page-index book.starts book.at) 0) (go state book.chapter [0 0])]
                 [(> book.chapter 0) (quietly (fn [] (go state (dec book.chapter) [0 0])))]
                 [else (message "This is the first chapter")]))))

(defcommand epub-beginning "Go to the beginning of the book"
  (with-book (fn [state _] (quietly (fn [] (go state 0 [0 0]))))))

(defcommand epub-end "Go to the end of the book"
  (with-book (fn [state book] (quietly (fn [] (go state (dec (count book.chapters)) :end))))))

(defcommand epub-contents "Jump to a part of the book from its table of contents"
  (with-book
    (fn [state book]
      (if (empty? book.toc)
        (message "This book has no table of contents")
        (quietly
         (fn []
           (let [labels (map (fn [item] (str (string/repeat "  " item.depth) item.title)) book.toc)
                 picked (await (prompt "Contents: " :choices labels))
                 at (first (keep (fn [at] (when (= (nth labels at) picked) at)) (range (count labels))))]
             (when at
               (let [item (nth book.toc at)]
                 (follow state (if item.fragment (str item.entry "#" item.fragment) item.entry)))))))))))

(def (chapter-texts book)
  (let [total (count book.chapters)]
    (vec (map (fn [index]
                (when (= 0 (mod index 5)) (message "Searching {(inc index)}/{total}…"))
                (let [entry (nth book.chapters index)
                      blocks (or (get book.blocks index)
                                 (if (map? entry)
                                   []
                                   (get (html/blocks (xml/parse (await (zip/read book.path entry)) :html true) entry) :blocks)))]
                  (vec (map reading/block-text blocks))))
              (range total)))))

(def (show-match state step)
  (let [book @state
        matches (or book.matches [])]
    (if (empty? matches)
      (message "No match for {book.query}")
      (let [at (mod (+ (or book.match -1) step) (count matches))
            found (nth matches at)]
        (swap! state assoc :match at)
        (quietly (fn []
                   (go state found.chapter [found.block found.start])
                   (message "{(inc at)} of {(count matches)} for {book.query}")))))))

(defcommand epub-search "Search the whole book, showing every match"
  (with-book
    (fn [state book]
      (quietly
       (fn []
         (let [query (await (prompt "Search book: " :initial (or book.query "")))]
           (when (not (string/blank? query))
             (let [texts (or book.texts (chapter-texts book))
                   matches (reading/find-matches texts query)]
               (swap! state assoc :texts texts :query query :matches matches :match nil)
               (show-match state 1)))))))))

(defcommand epub-search-next "Go to the next match of the search, or search"
  (with-book (fn [state book] (if book.query (show-match state 1) (run-command 'epub-search)))))

(defcommand epub-search-previous "Go to the match of the search before this one"
  (with-book (fn [state book] (if book.query (show-match state -1) (run-command 'epub-search)))))

(def (selection-text book)
  (when-let [ends book.selection]
    (let [[from to] ends]
      (reading/between (map reading/block-text (get book.blocks book.chapter)) from to))))

(def (mark state note)
  (if-let [ends (get @state :selection)]
    (let [[from to] ends]
      (swap! state (fn [book]
                     (assoc book
                            :highlights (conj book.highlights {chapter: book.chapter from: from to: to
                                                               text: (selection-text book) note: note})
                            :selection nil)))
      (quietly (fn [] (save! @state)))
      (message (if note "Noted" "Highlighted")))
    (message "Drag over some text first")))

(defcommand epub-highlight "Highlight the text dragged over"
  (with-book (fn [state _] (mark state nil))))

(defcommand epub-note "Write a note on the text dragged over"
  (with-book
    (fn [state book]
      (if book.selection
        (quietly (fn [] (mark state (await (prompt "Note: ")))))
        (message "Drag over some text first")))))

(defcommand epub-copy "Copy the text dragged over"
  (with-book
    (fn [_ book]
      (if-let [text (selection-text book)]
        (do (copy-text text) (message "Copied"))
        (message "Drag over some text first")))))

(def (notes-lines book)
  (vec (mapcat (fn [kept]
                 (let [entry (nth book.chapters kept.chapter)
                       found (when (string? entry) (find (fn [item] (= item.entry entry)) book.toc))
                       title (if found found.title "Chapter {(inc kept.chapter)}")]
                   (concat [[[title :heading]]
                            ["  " [(string/truncate (package/tidy kept.text) 120) (if kept.note :added :marked)]]]
                           (if kept.note [["    " [kept.note :comment]]] [])
                           [""])))
               book.highlights)))

(defcommand epub-notes "List the highlights and notes of this book"
  (with-book
    (fn [state book]
      (if (empty? book.highlights)
        (message "No highlights or notes yet; drag over text, then h or N")
        (let [reading (current-buffer)
              notes (open-buffer notes-buffer (notes-lines book) :beside true :select true
                                 :keys {"RET" 'epub-notes-visit "q" 'quit-window})]
          (with-buffer notes (set-buffer-value! :epub-notes {buffer: reading state: state}))
          (set-header notes [[["Highlights and notes in " :comment] [book.title :heading]]]))))))

(defcommand epub-notes-visit "Turn to the highlight on this line"
  (when-let [notes (buffer-value :epub-notes)]
    (when (buffer-live? notes.buffer)
      (let [state notes.state
            book @state
            line (current-line)
            counts (map (fn [kept] (if kept.note 4 3)) book.highlights)
            ends (reduce (fn [out n] (conj out (+ (or (last out) 0) n))) [] counts)
            at (count (take-while (fn [end] (<= end line)) ends))]
        (when (< at (count book.highlights))
          (let [kept (nth book.highlights at)]
            (switch-to-buffer notes.buffer)
            (quietly (fn [] (go state kept.chapter kept.from)))))))))

(def (resize state by)
  (swap! state (fn [book] (assoc book :size (max 10 (min 48 (+ (or book.size (setting :size)) by))))))
  (quietly (fn [] (save! @state))))

(defcommand epub-larger "Make the text larger" (with-book (fn [state _] (resize state 1))))
(defcommand epub-smaller "Make the text smaller" (with-book (fn [state _] (resize state -1))))

(defcommand epub-info "Say what book this is and how far through it you are"
  (with-book (fn [_ book]
               (message "{book.title}{(if book.creator " by {book.creator}" "")} · chapter {(inc book.chapter)} of {(count book.chapters)} · {(or book.progress 0)}%"))))

(defcommand epub-recent "Open a book read lately"
  (quietly
   (fn []
     (let [kept (remembered)]
       (if (empty? kept)
         (message "No books read yet")
         (let [labels (map (fn [book] "{book.title} ({(path/name book.path)})") kept)
               picked (await (prompt "Open book: " :choices labels))
               at (first (keep (fn [at] (when (= (nth labels at) picked) at)) (range (count labels))))]
           (when at (open-file (get (nth kept at) :path)))))))))

(def (show-shelf)
  (open-view library/buffer (fn [] (library/view @shelf (fn [place] (open-file place))))
             :on-resize (fn [width _] (swap! shelf assoc :width width))
             :keys {"g" 'epub-library-refresh "q" 'quit-window}))

(def (fill-shelf folder)
  (swap! shelf assoc :folder folder :books [])
  (show-shelf)
  (let [kept (remembered)]
    (each [place (library/found folder)]
      (let [card (library/card place (find (fn [book] (library/same-place? book.path place)) kept))]
        (swap! shelf update :books conj card)))
    (message "{(count (get @shelf :books))} books in {(path/abbreviate folder)}")))

(defcommand epub-library "Show the books in a folder as covers"
  (quietly
   (fn []
     (let [folder (or (setting :library) (await (prompt-path "Library folder: " :directories true)))]
       (fill-shelf (path/expand folder))))))

(defcommand epub-library-refresh "Read the library folder again"
  (when-let [folder (get @shelf :folder)]
    (quietly (fn [] (fill-shelf folder)))))

(on-window-change :epub (fn [] (show-progress)))
