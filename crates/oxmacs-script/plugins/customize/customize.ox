(module
  (import (ox path) (ox file) (ox string)
          (customize form :as form)
          (customize written :as written)))

(def buffer "*Settings*")

(defonce state (atom {entries: []}))

(def form-keys
  {"n" 'customize-next "TAB" 'customize-next "p" 'customize-previous "S-TAB" 'customize-previous
   "RET" 'customize-change "C-x C-s" 'customize-save "g" 'customize-revert "q" 'quit-window})

(def (custom-file)
  (when-let [home (oxmacs-home)]
    (path/join home "custom.ox")))

(def (current-entries)
  (form/entries (settings-schema) (settings) (saved-settings) (custom-settings) (plugin-settings)))

(def (draw opening)
  (let [all (current-entries)
        file (custom-file)
        page (form/page all (when file (path/abbreviate file)))
        unsaved (some? (some :changed all))]
    (swap! state assoc :entries all)
    (if opening
      (open-buffer buffer page.lines :keys form-keys :unsaved unsaved)
      (set-lines buffer page.lines :unsaved unsaved))
    (set-header buffer page.header)))

(on-settings-change :customize
                    (fn []
                      (when (buffer-exists? buffer)
                        (draw false))))

(def (row-here)
  (let [line (current-line)]
    (or (line-item line)
        (when (> line 0) (line-item (dec line))))))

(def (entry-of row)
  (nth (get @state :entries) row.index))

(def (go row)
  (when row
    (goto-line (current-line) row.column)
    (let [hint (get (entry-of row) :hint)]
      (when (and hint (not= hint ""))
        (message hint)))))

(def (change entry value)
  (try
    (if entry.plugin
      (configure-plugin entry.plugin (assoc {} entry.key value))
      (settings (assoc {} entry.key value)))
    (draw false)
    (catch error e
      (message e.message))))

(def (next-choice entry)
  (let [options (vec (concat (if entry.unset [nil] []) entry.choices))
        now (form/effective entry)
        at (or (find (fn [index] (= (nth options index) now)) (range (count options))) -1)]
    (nth options (mod (+ at 1) (count options)))))

(def (ask entry)
  (let [now (form/effective entry)
        key (name entry.key)
        answer (await (prompt "{key}: " :initial (if (nil? now) "" (str now))))
        trimmed (string/trim answer)
        whole (= entry.type :integer)]
    (cond ((and (= trimmed "") (or entry.unset entry.plugin)) (change entry nil))
      ((= entry.type :text) (change entry answer))
      (else
       (if-let [number (string/to-number trimmed)]
         (change entry number)
         (message (str key " needs " (if whole "a whole number" "a number"))))))))

(defcommand customize "Change settings in a form"
  (let [fresh (not (buffer-exists? buffer))]
    (draw true)
    (when fresh
      (go (goto-item {})))))

(defcommand customize-next "Move to the next setting"
  (go (or (next-item) (goto-item {}))))

(defcommand customize-previous "Move to the previous setting"
  (go (or (next-item -1)
          (goto-item {index: (dec (count (get @state :entries)))}))))

(defcommand customize-change "Change the setting on this line"
  (if-let [row (row-here)]
    (let [entry (entry-of row)]
      (cond ((= entry.type :toggle) (change entry (not (form/effective entry))))
        ((= entry.type :choice) (change entry (next-choice entry)))
        ((= entry.type :theme) (run-command 'load-theme))
        ((= entry.type :data) (message "{(name entry.key)} is a list; change it in init.ox with use-plugin"))
        (else (ask entry))))
    (message "No setting here")))

(defcommand customize-save "Write the changed settings to custom.ox"
  (let [all (current-entries)
        file (custom-file)]
    (cond ((nil? file) (message "There is no oxmacs home folder to save custom.ox in"))
      ((not (some :changed all)) (message "(No changes need to be saved)"))
      (else
       (await (file/make-dir (path/parent file)))
       (await (file/write file (written/text all)))
       (reload-init)
       (message "Wrote {(path/abbreviate file)}")))))

(defcommand customize-revert "Put back the settings custom.ox and init.ox give"
  (reload-init))
