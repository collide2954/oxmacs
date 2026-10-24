(module
  (import (ox string) (ox ui) (ox image)
          (r session :as session))
  (export view gallery pick arrived thumbnails zoomed columns plots-buffer unpinned-of))

(def size [672 480])
(def zoom-width 1200)
(def pinned-text "This plot is pinned; s saves it, z zooms and q closes it")

(defonce views (atom {}))

(def (stem session)
  (if (string/ends-with? session "*") (string/slice session 0 (- (count session) 1)) session))

(def (columns total)
  (or (find (fn [across] (>= (* across across) total)) (range 1 (+ (max total 1) 1))) 1))

(def (alive)
  (filter (fn [name] (buffer-exists? name)) (keys @views)))

(def (unpinned-of session)
  (find (fn [name] (let [plots (get @views name)] (and (= plots.session session) (not plots.pinned))))
        (alive)))

(def (pinned-of session id)
  (find (fn [name]
          (let [plots (get @views name)]
            (and (= plots.session session) plots.pinned plots.shown (= plots.shown.id id))))
        (alive)))

(def (view name)
  (when-let [shown (get-in @views [name :shown])]
    [shown.index shown.count shown.path]))

(def (gallery name)
  (when-let [grid (get-in @views [name :gallery])]
    [grid.selected grid.columns grid.paths]))

(def (want plots)
  (when-let [index (or plots.wanted (when plots.shown plots.shown.index))]
    [index (or plots.size size)]))

(def (refresh name)
  (let [plots (get @views name)
        wanted (when plots (want plots))]
    (when (and wanted (not plots.asked) (not= plots.rendered wanted))
      (let [[index [width height]] wanted
            call (if (and plots.pinned plots.shown)
                   "getOption(\"oxmacs\")$plot_pinned({plots.shown.id}L, {width}L, {height}L)"
                   "getOption(\"oxmacs\")$plot({index}L, {width}L, {height}L)")]
        (when (session/request plots.session call)
          (swap! views assoc-in [name :asked] wanted))))))

(def (load name path key)
  (spawn
    (try (let [picture (await (image/open path))]
           (swap! views update name
                  (fn [plots]
                    (when plots
                      (assoc plots key (if (= key :thumbs) (assoc (or plots.thumbs {}) path picture) {path picture}))))))
         (catch error e nil))))

(def (shown-image plots)
  (let [pictures (or plots.pictures {})]
    (or (get pictures plots.shown.path) plots.last-picture)))

(def (thumbnail name plots index path)
  (let [[width height] plots.gallery.tile
        picture (get (or plots.thumbs {}) path)]
    (if picture
      (ui/image picture :width width :height height :selected (= index plots.gallery.selected)
                :on-press (fn [] (pick name index)))
      (ui/text "…" :face :comment))))

(def (grid name plots)
  (let [across plots.gallery.columns
        paths plots.gallery.paths]
    (ui/column :spacing 4
               (map (fn [row] (ui/row :spacing 4
                                      (map (fn [index] (thumbnail name plots index (nth paths index)))
                                           (range (* row across) (min (count paths) (* (+ row 1) across))))))
                    (range (// (+ (count paths) across -1) across))))))

(def (render name)
  (let [plots (get @views name)]
    (cond ((not plots) (ui/text ""))
          (plots.gallery (grid name plots))
          (plots.shown
           (let [picture (shown-image plots)
                 counter (ui/row (ui/space) (ui/text "{plots.shown.index} / {plots.shown.count}" :face :comment))]
             (if picture
               (ui/column counter (ui/image picture :width plots.shown.width :height plots.shown.height))
               counter)))
          (else (ui/text (if plots.gone "This plot is no longer in R's history" "No plots") :face :comment)))))

(def plot-keys
  {"n" 'r-plot-next "<right>" 'r-plot-next "p" 'r-plot-previous "<left>" 'r-plot-previous
   "s" 'r-plot-save "d" 'r-plot-delete "z" 'r-plot-zoom "v" 'r-plot-gallery "k" 'r-plot-pin
   "RET" 'r-plot-open "<up>" 'r-plot-above "<down>" 'r-plot-below "q" 'quit-window})

(def (resized name width height)
  (swap! views assoc-in [name :size] [width height])
  (refresh name))

(def (make name session)
  (open-view name (fn [] (render name)) :keys plot-keys :show false
             :on-resize (fn [width height] (resized name width height)))
  (session/own! name session)
  name)

(def (plots-buffer session)
  (let [name (or (unpinned-of session) "{(stem session)} plots*")]
    (unless (get @views name)
      (swap! views assoc name {session: session pinned: false})
      (make name session))
    name))

(def (place session)
  (let [name (plots-buffer session)]
    (unless (buffer-shown? name)
      (let [pinned (find (fn [other] (let [plots (get @views other)]
                                       (and (= plots.session session) plots.pinned (buffer-shown? other))))
                         (alive))]
        (cond (pinned (display-buffer name :right-of pinned))
              ((buffer-shown? session) (display-buffer name :below session))
              (else (display-buffer name)))))
    name))

(def (parse payload)
  (let [fields (string/split payload "\t" 7)]
    (when (and (= (count fields) 7) (contains? #{"new" "asked" "deleted" "pinned"} (first fields)))
      (let [number (fn [at] (string/to-number (nth fields at)))]
        {why: (first fields) index: (number 1) count: (number 2) id: (number 3)
         width: (number 4) height: (number 5) path: (nth fields 6)}))))

(def (settle name arrival)
  (swap! views update name
         (fn [plots]
           (-> plots
               (assoc :asked (if (= arrival.why "new") plots.asked nil))
               (assoc :wanted (if (= arrival.why "asked") plots.wanted arrival.index))
               (assoc :rendered [arrival.index [arrival.width arrival.height]])
               (assoc :gone false)
               (assoc :last-picture (when plots.shown (shown-image plots)))
               (assoc :shown {path: arrival.path index: arrival.index count: arrival.count id: arrival.id
                              width: arrival.width height: arrival.height}))))
  (load name arrival.path :pictures)
  (refresh name))

(def (emptied session)
  (when-let [name (unpinned-of session)]
    (swap! views update name (fn [plots] {session: plots.session pinned: false size: plots.size}))))

(def (gone session id)
  (when-let [name (pinned-of session id)]
    (swap! views update name (fn [plots] (assoc plots :shown nil :asked nil :gone true)))))

(def (arrived session payload)
  (cond ((= payload "none") (emptied session))
        ((string/starts-with? payload "gone\t")
         (gone session (string/to-number (string/slice payload 5))))
        (else
         (when-let [arrival (parse payload)]
           (if (= arrival.why "pinned")
             (when-let [name (pinned-of session arrival.id)] (settle name arrival))
             (settle (place session) arrival))))))

(def (thumbnails session payload)
  (when-let [name (unpinned-of session)]
    (let [paths (filter (fn [path] (not (string/blank? path))) (string/split payload "\t"))]
      (if (empty? paths)
        (swap! views update name dissoc :gallery)
        (let [plots (get @views name)
              shown (if plots.shown plots.shown.index 1)]
          (swap! views assoc-in [name :gallery]
                 {paths: paths selected: (min (max 0 (- shown 1)) (- (count paths) 1))
                  columns: (columns (count paths)) tile: (or plots.tile [100 100])})
          (map (fn [path] (load name path :thumbs)) paths))))))

(def (zoomed _ path) (open-externally path))

(def (here) (let [name (buffer-name)] (when (get @views name) name)))

(def (refused name)
  (let [pinned (get-in @views [name :pinned])]
    (when pinned (message pinned-text))
    pinned))

(def (shown-plot name action)
  (let [plots (get @views name)]
    (if (and plots plots.shown)
      [plots.shown.index plots.shown.id (or plots.size size)]
      (do (message "No plot to {action}") nil))))

(def (move-selection name step)
  (swap! views update-in [name :gallery]
         (fn [grid]
           (let [next (+ grid.selected step)]
             (if (and (>= next 0) (< next (count grid.paths))) (assoc grid :selected next) grid)))))

(def (step forward)
  (when-let [name (here)]
    (unless (refused name)
      (let [plots (get @views name)]
        (if plots.gallery
          (move-selection name (if forward 1 -1))
          (when-let [[current _] (want plots)]
            (when plots.shown
              (let [next (if forward (+ current 1) (- current 1))]
                (if (and (>= next 1) (<= next plots.shown.count))
                  (do (swap! views assoc-in [name :wanted] next)
                    (refresh name))
                  (message "This is the {(if forward "last" "first")} plot"))))))))))

(def (pick name position)
  (when (get-in @views [name :gallery])
    (swap! views update name (fn [plots] (-> plots (dissoc :gallery) (assoc :wanted (+ position 1)))))
    (refresh name)))

(defcommand r-plot-next "Show the next R plot" (step true))

(defcommand r-plot-previous "Show the previous R plot" (step false))

(defcommand r-plot-above "Select the plot above in the gallery"
  (when-let [name (here)]
    (if-let [grid (get-in @views [name :gallery])]
      (move-selection name (- grid.columns))
      (step false))))

(defcommand r-plot-below "Select the plot below in the gallery"
  (when-let [name (here)]
    (if-let [grid (get-in @views [name :gallery])]
      (move-selection name grid.columns)
      (step true))))

(defcommand r-plot-open "Open the plot selected in the gallery"
  (when-let [name (here)]
    (when-let [grid (get-in @views [name :gallery])]
      (pick name grid.selected))))

(defcommand r-plot-save "Save the shown R plot to a file"
  (when-let [name (here)]
    (when-let [[index _ _] (shown-plot name "save")]
      (let [path (await (prompt-path "Save plot as: " :initial "plot-{index}.png"))]
        (when-let [[_ id [width height]] (shown-plot name "save")]
          (session/request (get-in @views [name :session])
                           "getOption(\"oxmacs\")$plot_save({id}L, {(session/r-string path)}, {width}L, {height}L)"))))))

(defcommand r-plot-delete "Delete the shown R plot from the history"
  (when-let [name (here)]
    (unless (refused name)
      (when-let [[index id [width height]] (shown-plot name "delete")]
        (when (session/request (get-in @views [name :session])
                               "getOption(\"oxmacs\")$plot_delete({id}L, {width}L, {height}L)")
          (swap! views update name (fn [plots] (assoc plots :asked [index [width height]] :wanted nil))))))))

(defcommand r-plot-zoom "Open a large copy of the shown R plot"
  (when-let [name (here)]
    (when-let [[_ id [width height]] (shown-plot name "zoom")]
      (let [wide (max (* width 2) zoom-width)
            tall (// (* wide height) (max width 1))]
        (session/request (get-in @views [name :session])
                         "getOption(\"oxmacs\")$plot_zoom({id}L, {wide}L, {tall}L)")))))

(defcommand r-plot-gallery "Show all R plots side by side, or the shown one again"
  (when-let [name (here)]
    (unless (refused name)
      (let [plots (get @views name)]
        (cond (plots.gallery (swap! views update name dissoc :gallery))
              ((not plots.shown) (message "No plots"))
              (else
               (let [total plots.shown.count
                     across (columns total)
                     down (// (+ total across -1) across)
                     [width height] (or plots.size size)
                     tile [(// width (max across 1)) (// height (max down 1))]]
                 (swap! views assoc-in [name :tile] tile)
                 (session/request plots.session
                                  "getOption(\"oxmacs\")$plot_gallery({(first tile)}L, {(second tile)}L)"))))))))

(defcommand r-plot-pin "Keep the shown R plot in its own window"
  (when-let [name (here)]
    (unless (refused name)
      (let [plots (get @views name)]
        (if (not plots.shown)
          (message "No plot to pin")
          (let [pinned "{(string/replace name "plots*" "")}plot {plots.shown.index}*"]
            (swap! views (fn [all] (-> all
                                       (dissoc name)
                                       (assoc pinned (-> plots (assoc :pinned true) (dissoc :gallery))))))
            (make pinned plots.session)
            (switch-to-buffer pinned)
            (close-buffer name)
            (message "Pinned; new plots will open beside it")))))))
