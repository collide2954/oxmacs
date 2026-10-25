(module
  (import (ox path) (ox file)
          (customize form :as form)
          (customize written :as written))
  (export plugin-setting!))

(def (plugin-setting! plugin key value)
  "Sets plugin's key setting to value and writes it to custom.ox with the settings saved there before, leaving other unsaved changes unsaved. True when init.ox sets it too and wins."
  (when (nil? (oxmacs-home))
    (raise (error "There is no oxmacs home folder to save custom.ox in")))
  (configure-plugin plugin (assoc {} key value))
  (let [file (path/join (oxmacs-home) "custom.ox")
        mine? (fn [entry] (and (= entry.plugin (name plugin)) (= entry.key key)))
        found (form/entries (settings-schema) (settings) (saved-settings) (custom-settings) (plugin-settings))
        unsaved (some (fn [entry] (and entry.changed (not (mine? entry)))) found)
        entry (find mine? found)
        overridden (not= entry.saved entry.custom)
        entries (map (fn [entry] (assoc entry :changed (mine? entry))) found)]
    (await (file/make-dir (path/parent file)))
    (await (file/write file (written/text entries)))
    (unless (or unsaved overridden)
      (reload-init))
    overridden))
