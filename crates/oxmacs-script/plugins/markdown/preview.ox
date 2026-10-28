(module
  (import (ox file) (ox path) (ox string)
          (markdown render :as render))
  (export show))

(def (preview-of buffer)
  (when (buffer-live? buffer)
    (with-buffer buffer (buffer-value :markdown-preview))))

(def (width-for buffer)
  (min 100 (max 20 (- (or (buffer-width buffer) 80) 2))))

(def (draw buffer)
  (when-let [state (preview-of buffer)]
    (let [page (render/render (render/body (get @state :text)) (width-for buffer))]
      (swap! state assoc :page page)
      (set-lines buffer page.lines))))

(def (link-here)
  (when-let [state (buffer-value :markdown-preview)]
    (let [line (current-line)
          column (current-column)
          links (get-in @state [:page :links])]
      (or (first (filter (fn [link] (and (= link.line line) (<= link.start column) (< column link.end))) links))
          (first (filter (fn [link] (= link.line line)) links))))))

(defcommand markdown-follow-link "Open the link on this line"
  (if-let [link (link-here)]
    (let [target link.target]
      (if (or (string/starts-with? target "http://") (string/starts-with? target "https://"))
        (open-externally target)
        (open-file (path/join (get @(buffer-value :markdown-preview) :folder) target))))
    (message "No link on this line")))

(defcommand markdown-preview-refresh "Read the Markdown file again"
  (when-let [state (buffer-value :markdown-preview)]
    (when-let [source (get @state :source)]
      (swap! state assoc :text (await (file/read source))))
    (draw (current-buffer))))

(on-saved :markdown-preview
          (fn [saved]
            (each [name (buffer-list)]
              (when-let [state (preview-of name)]
                (when (= (get @state :source) saved)
                  (swap! state assoc :text (await (file/read saved)))
                  (draw name))))))

(def (show)
  "Shows the current buffer rendered as Markdown in a buffer beside it, drawn again when its file is saved."
  (let [file (buffer-file)
        name "*Markdown: {(if file (path/name file) (buffer-name))}*"
        state (atom {text: (buffer-text) folder: (if file (path/parent file) (default-directory)) source: file})
        buffer (open-buffer name [] :beside true
                            :keys {"RET" 'markdown-follow-link "g" 'markdown-preview-refresh "q" 'quit-window}
                            :on-resize (fn [_] (draw name)))]
    (with-buffer buffer (set-buffer-value! :markdown-preview state))
    (draw buffer)))
