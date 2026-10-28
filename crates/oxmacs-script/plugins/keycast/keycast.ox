(module
  (import (ox time)
          (keycast stack :as stack)))

(def large 1.6)

(defonce casts (atom []))
(defonce drawing (atom false))

(def (draw shown now)
  (let [placement (setting :placement)]
    (if (= placement :mode-line)
      (do (set-overlay :keycast nil)
        (set-mode-line :keycast (stack/caption (last shown))))
      (do (set-mode-line :keycast nil)
        (set-overlay :keycast (stack/rows shown now)
                     :place placement
                     :scale (if (= placement :bottom-center) large 1))))))

(def (redraw)
  (let [now (time/now)
        shown (swap! casts stack/live now)]
    (draw shown now)
    (stack/wait shown now)))

(def (keep-drawing)
  (if-let [wait (redraw)]
    (do (sleep wait)
      (keep-drawing))
    (do (reset! drawing false)
      (when (and (not (empty? @casts)) (compare-and-set! drawing false true))
        (keep-drawing)))))

(def (heard event)
  (when (stack/shown? event)
    (let [now (time/now)]
      (draw (swap! casts stack/push event now) now))
    (when (compare-and-set! drawing false true)
      (spawn (keep-drawing)))))

(on-key-command :keycast heard)
