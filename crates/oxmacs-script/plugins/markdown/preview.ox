(module
  (import (ox file) (ox path) (ox string)
          (markdown render :as render))
  (export show))

(defonce previews (atom {}))

(def (width-for name)
  (min 100 (max 20 (- (or (buffer-width name) 80) 2))))

(def (draw name)
  (when-let [preview (get @previews name)]
    (let [page (render/render (render/body preview.text) (width-for name))]
      (swap! previews assoc-in [name :page] page)
      (set-lines name page.lines))))

(def (link-here)
  (when-let [preview (get @previews (buffer-name))]
    (let [line (current-line)
          column (current-column)]
      (or (first (filter (fn [link] (and (= link.line line) (<= link.start column) (< column link.end)))
                         preview.page.links))
          (first (filter (fn [link] (= link.line line)) preview.page.links))))))

(defcommand markdown-follow-link "Open the link on this line"
  (if-let [link (link-here)]
    (let [target link.target
          preview (get @previews (buffer-name))]
      (if (or (string/starts-with? target "http://") (string/starts-with? target "https://"))
        (open-externally target)
        (open-file (path/join preview.folder target))))
    (message "No link on this line")))

(defcommand markdown-preview-refresh "Read the Markdown file again"
  (when-let [preview (get @previews (buffer-name))]
    (when preview.source
      (swap! previews assoc-in [(buffer-name) :text] (await (file/read preview.source))))
    (draw (buffer-name))))

(on-saved :markdown-preview
          (fn [saved]
            (map (fn [[name preview]]
                   (when (= preview.source saved)
                     (swap! previews assoc-in [name :text] (await (file/read saved)))
                     (draw name)))
                 (entries @previews))))

(def (show)
  "Shows the current buffer rendered as Markdown in a buffer beside it, drawn again when its file is saved."
  (let [file (buffer-file)
        name "*Markdown: {(if file (path/name file) (buffer-name))}*"
        text (buffer-text)]
    (swap! previews assoc name {text: text folder: (if file (path/parent file) (default-directory)) source: file})
    (open-buffer name [] :beside true
                 :keys {"RET" 'markdown-follow-link "g" 'markdown-preview-refresh "q" 'quit-window}
                 :on-resize (fn [_] (draw name)))
    (draw name)))
