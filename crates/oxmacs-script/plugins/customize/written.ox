(module
  (import (ox string))
  (export text))

(def header ";; Written by M-x customize. Your own settings belong in init.ox,\n;; which runs after this file and wins.\n")

(def (kept entry)
  (let [value (if entry.changed entry.value entry.custom)]
    (when (and (some? value) (not= value entry.default))
      [entry value])))

(def (pair [entry value])
  (str (name entry.key) ": " (pr-str value)))

(def (core-form found)
  (str "(settings {{" (string/join "\n           " (map pair found)) "}})\n"))

(def (plugin-form plugin found)
  (str "(settings '" plugin " {{" (string/join " " (map pair found)) "}})\n"))

(def (text all)
  "custom.ox for all the entries: the settings changed since it was read, the ones it set before, and none that are at their defaults."
  (let [found (keep kept all)
        core (filter (fn [[entry _]] (nil? entry.owner)) found)
        plugins (distinct (keep (fn [[entry _]] entry.owner) found))
        forms (concat (if (empty? core) [] [(core-form core)])
                      (map (fn [plugin] (plugin-form plugin (filter (fn [[entry _]] (= entry.owner plugin)) found)))
                           plugins))]
    (string/join "\n" (concat [header] forms))))
