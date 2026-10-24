(module
  (import (ox string) (ox path) (ox file) (ox process)
          (r session :as session))
  (export rscript define! wanted? check ready?))

(defonce state (atom {defined: nil checked: false}))

(def (wanted?) (plugin-setting 'r :language-server))

(def (rscript program)
  (path/join (path/parent program) (if (session/windows?) "Rscript.exe" "Rscript")))

(def (define! program)
  (let [script (rscript program)]
    (when (and (not= (get @state :defined) script) (await (file/exists? script)))
      (swap! state assoc :defined script)
      (define-language-server "languageserver"
        :languages {"R" "r" "R Markdown" "rmd"}
        :program script
        :args ["-e" "languageserver::run()"]
        :roots ["DESCRIPTION" "renv.lock"]
        :install "Run install.packages(\"languageserver\") in R"))))

(def (installed? script)
  (try (string/includes? (get (await (process/run script ["-e" "cat(nzchar(system.file(package = 'languageserver')))"]))
                              :stdout)
                         "TRUE")
       (catch error e false)))

(def (check)
  (when (and (wanted?) (not (get @state :checked)))
    (swap! state assoc :checked true)
    (when-let [program (session/find-r)]
      (when (and (await (file/exists? (rscript program))) (installed? (rscript program)))
        (define! program)))))

(def (ready?) (and (wanted?) (get @state :defined) (language-server-ready?)))
