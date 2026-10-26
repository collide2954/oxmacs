(module
  (import (ox file) (ox image) (ox path) (ox string) (ox ui) (ox zip)
          (epub package :as package))
  (export buffer found card view per-row))

(def buffer "*epub library*")
(def cover-width 132)
(def cover-height 196)
(def gap 28)

(def (epub? entry)
  (and (not entry.dir) (= (string/lower (or (path/extension entry.name) "")) "epub")))

(def (found folder)
  "The .epub files in folder and the folders just inside it, sorted by path."
  (let [entries (await (file/list folder))
        below (mapcat (fn [entry]
                        (if (and entry.dir (not entry.hidden))
                          (filter epub? (or (maybe (await (file/list entry.path))) []))
                          []))
                      entries)]
    (sort (map :path (concat (filter epub? entries) below)))))

(def (read-book place)
  (let [rootfile (package/rootfile (await (zip/read place "META-INF/container.xml")))
        found (package/package (await (zip/read place rootfile)) rootfile)
        cover (when found.cover
                (maybe (await (image/thumbnail (image/decode (await (zip/read-base64 place found.cover))) 400))))]
    (assoc found :cover cover)))

(def (card place kept)
  "What the library shows of the book at place: its :title, :creator, :cover image and the :progress kept from reading it."
  (let [read (maybe (read-book place))]
    {path: place
     title: (or (when read read.title) (path/stem place))
     creator: (when read read.creator)
     cover: (when read read.cover)
     progress: (when kept kept.progress)}))

(def (per-row width)
  "How many covers fit side by side in width pixels."
  (max 1 (// (- width 40) (+ cover-width gap))))

(defonce blank-cover (atom nil))

(def (blank)
  (or @blank-cover (reset! blank-cover (image/new cover-width cover-height :color "#9a9996"))))

(def (cell book open)
  (ui/column :spacing 4
             (ui/image (or book.cover (blank)) :width cover-width :height cover-height
                       :on-press (fn [] (open book.path)))
             (ui/text (string/truncate book.title 20))
             (ui/text (string/truncate (or book.creator "") 22) :face :comment)
             (ui/text (if book.progress "{book.progress}% read" "New") :face (if book.progress :success :comment))))

(def (view shelf open)
  "The library view of shelf, a map of :folder, :books and the view's :width; clicking a cover calls open with its path."
  (ui/scrollable
   (ui/column :spacing 22
              (ui/text "Library" :face :heading :size 24)
              (ui/text (if shelf.folder (path/abbreviate shelf.folder) "") :face :comment)
              (if (empty? shelf.books)
                (ui/text "Looking for books…" :face :comment)
                (for [row (chunk (per-row shelf.width) shelf.books)]
                  (ui/row :spacing gap (for [book row] (cell book open))))))))

(test "covers fill the width"
  (expect (= (per-row 900) 5))
  (expect (= (per-row 100) 1)))
