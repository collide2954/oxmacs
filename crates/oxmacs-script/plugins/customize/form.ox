(module
  (import (ox string))
  (export entries effective page hint-of))

(def help "RET change  C-x C-s save  g revert  q quit")

(def plugin-types {string: :text int: :integer number: :number bool: :toggle choice: :choice list: :data})

(def (core-entry setting now saved custom)
  (let [value (get now setting.key)
        kept (get saved setting.key)]
    (merge setting
           {plugin: nil
            value: value
            saved: kept
            custom: (get custom setting.key)
            changed: (not= value kept)})))

(def (plugin-entry setting)
  {key: setting.key
   plugin: setting.plugin
   group: (str setting.plugin " plugin")
   type: (get plugin-types setting.type)
   hint: (or setting.doc "")
   unset: nil
   default: setting.default
   choices: setting.choices
   value: setting.value
   saved: setting.saved
   custom: setting.custom
   changed: (not= setting.value setting.saved)})

(def ^:shadow (entries schema now saved custom plugins)
  "Every setting to list, the editor's and then each plugin's, with its :value, :saved, :custom and whether it :changed."
  (vec (concat (map (fn [setting] (core-entry setting now saved custom)) schema)
               (map plugin-entry plugins))))

(def (effective entry)
  "The value in use for entry, its default when nothing is set."
  (if (nil? entry.value) entry.default entry.value))

(def (chosen? entry)
  (or (= entry.type :choice) (= entry.type :theme)))

(def (shown entry)
  (let [value (effective entry)
        unset (or entry.unset "unset")]
    (cond ((and (nil? value) (chosen? entry)) (str "[" unset "]"))
      ((nil? value) (str "(" unset ")"))
      ((= value true) "[x]")
      ((= value false) "[ ]")
      ((= entry.type :data) (str "(" (count value) " in init.ox)"))
      ((chosen? entry) (str "[" (name value) "]"))
      (else (str value)))))

(def (hint-of entry)
  "The line under entry: its choices, or what it does."
  (cond ((= entry.type :choice)
         (string/join " | " (concat (if entry.unset [entry.unset] []) (map name entry.choices))))
    ((= entry.type :theme) (str (or entry.unset "") " | RET picks a theme with a live preview"))
    (else entry.hint)))

(def (set-in-init? entry)
  (and (not entry.changed) (not= entry.custom entry.saved)))

(def (setting-lines entry index width)
  (let [marker (if entry.changed ["* " :warning] "  ")
        note (when (set-in-init? entry) ["  set in init.ox" :comment])]
    [{line: (vec (filter some? [marker (string/pad-end (name entry.key) width) [(shown entry) :link] note]))
      index: index
      column: (+ 2 width)}
     [[(str "    " (hint-of entry)) :comment]]]))

(def (page all location)
  "The :header and :lines of *Settings* for all the entries; the line of each setting carries its :index in all and the :column of its value."
  (let [width (+ 2 (reduce max 0 (map (fn [entry] (count (name entry.key))) all)))
        step (fn [built index]
               (let [entry (nth all index)
                     lines (cond
                             [(= entry.group built.group) built.lines]
                             [(empty? built.lines) [[[entry.group :heading]]]]
                             [else (into built.lines [[] [[entry.group :heading]]])])]
                 {group: entry.group
                  lines: (into lines (setting-lines entry index width))}))
        built (reduce step {group: nil lines: []} (range (count all)))]
    {header: [[["Settings" :heading] (str "  " (or location "no oxmacs home folder"))]
              [[help :comment]]]
     lines: built.lines}))
