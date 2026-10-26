(module
  (import (ox string)
          (tabs strip :as strip)))

(def most-closed 10)

(defonce state (atom {tabs: [{key: 1 name: nil seen: nil}] current: 0 next: 2 closed: []}))

(def (key-of tab)
  "tab-{tab.key}")

(def (labels)
  (let [s @state
        shown (buffer-name)]
    (map (fn [at] (strip/label (nth s.tabs at) (when (= at s.current) shown)))
         (range (count s.tabs)))))

(def (draw)
  (let [s @state]
    (if (or (> (count s.tabs) 1) (= (setting :show) :always))
      (set-tab-bar (labels)
                   :selected s.current
                   :on-select select
                   :on-close close-at
                   :on-new new-tab
                   :on-move move-tab)
      (set-tab-bar nil))))

(def (leave s)
  (store-windows (key-of (nth s.tabs s.current)))
  (swap! state assoc-in [:tabs s.current :seen] (buffer-name)))

(def (select at)
  (atomically
    (let [s @state]
      (when (and (not= at s.current) (>= at 0) (< at (count s.tabs)))
        (leave s)
        (recall-windows (key-of (nth s.tabs at)))
        (swap! state assoc :current at))))
  (draw))

(def (fresh-windows)
  (arrange-windows (buffer-name))
  (when (and (= (setting :new-tab) :scratch) (buffer-exists? "*scratch*"))
    (switch-to-buffer "*scratch*")))

(def (open-tab fresh)
  (select-main-window)
  (atomically
    (let [s @state
          at (inc s.current)]
      (leave s)
      (swap! state (fn [s]
                     (assoc s
                            :tabs (strip/insert-at s.tabs at {key: s.next name: nil seen: nil})
                            :current at
                            :next (inc s.next))))
      (when fresh (fresh-windows))))
  (draw))

(def (new-tab)
  (open-tab true))

(def (bury s at)
  (let [tab (nth s.tabs at)
        closed (conj s.closed {tab: tab at: at})
        dropped (take (max 0 (- (count closed) most-closed)) closed)]
    (each [old dropped] (forget-windows (key-of old.tab)))
    (swap! state assoc
           :tabs (strip/remove-at s.tabs at)
           :closed (vec (drop (count dropped) closed)))))

(def (close-at at)
  (atomically
    (let [s @state
          n (count s.tabs)]
      (cond
        [(= n 1) (message "This is the only tab")]
        [(= at s.current)
         (let [tab (assoc (nth s.tabs at) :seen (buffer-name))
               next (strip/after-close at at n)
               target (nth (strip/remove-at s.tabs at) next)]
           (store-windows (key-of tab))
           (recall-windows (key-of target))
           (bury (assoc-in s [:tabs at] tab) at)
           (swap! state assoc :current next))]
        [(and (>= at 0) (< at n))
         (bury s at)
         (swap! state assoc :current (strip/after-close s.current at n))])))
  (draw))

(def (move-tab from to)
  (swap! state (fn [s]
                 (assoc s
                        :tabs (strip/move-item s.tabs from to)
                        :current (strip/follow s.current from to))))
  (draw))

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(defcommand tab-new "Open a new tab"
  (new-tab))

(defcommand tab-duplicate "Open a new tab with the current windows"
  (open-tab false))

(defcommand tab-close "Close the current tab"
  (close-at (get @state :current)))

(defcommand tab-close-other "Close every tab but the current one"
  (atomically
    (let [s @state
          kept (nth s.tabs s.current)]
      (each [tab s.tabs]
        (unless (= tab.key kept.key) (forget-windows (key-of tab))))
      (swap! state assoc :tabs [kept] :current 0)))
  (draw))

(def (step delta)
  (let [s @state]
    (select (mod (+ s.current delta) (count s.tabs)))))

(defcommand tab-next "Go to the next tab" (step 1))
(defcommand tab-previous "Go to the previous tab" (step -1))

(defcommand tab-move "Move the current tab one place to the right"
  (let [s @state]
    (if (< (inc s.current) (count s.tabs))
      (move-tab s.current (inc s.current))
      (message "This tab is already the last"))))

(defcommand tab-undo "Bring back the last closed tab"
  (atomically
    (let [s @state]
      (if-let [closed (last s.closed)]
        (let [at (min closed.at (count s.tabs))]
          (leave s)
          (swap! state (fn [s]
                         (assoc s
                                :tabs (strip/insert-at s.tabs at closed.tab)
                                :closed (pop s.closed)
                                :current at)))
          (recall-windows (key-of closed.tab)))
        (message "No closed tab to bring back"))))
  (draw))

(defcommand tab-rename "Name the current tab"
  (quietly
   (fn []
     (let [s @state
           name (await (prompt "Rename tab to: "
                               :initial (strip/label (nth s.tabs s.current) (buffer-name))))]
       (swap! state (fn [s] (assoc-in s [:tabs s.current :name] (if (string/blank? name) nil name))))
       (draw)))))

(defcommand tab-switch "Go to a tab by its name"
  (quietly
   (fn []
     (let [names (labels)
           picked (await (prompt "Switch to tab: " :choices names))
           at (first (keep (fn [at] (when (= (nth names at) picked) at)) (range (count names))))]
       (if at (select at) (message "No tab is called {picked}"))))))

(defcommand find-file-other-tab "Visit a file in a new tab"
  (quietly
   (fn []
     (let [path (await (prompt-path "Find file in new tab: "))]
       (new-tab)
       (open-file path)
       (draw)))))

(defcommand switch-to-buffer-other-tab "Show a buffer in a new tab"
  (quietly
   (fn []
     (let [name (await (prompt "Switch to buffer in new tab: " :choices (buffer-list)))]
       (new-tab)
       (if (buffer-exists? name)
         (switch-to-buffer name)
         (show-text name ""))
       (draw)))))

(on-window-change :tabs (fn [] (draw)))
(on-settings-change :tabs (fn [] (draw)))
(draw)
