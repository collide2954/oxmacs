(module
  (import (ox string) (ox process))
  (export parse-list listed default-name home-of linux-path))

(def (parse-list text)
  "The distributions wsl --list --verbose names, as maps of :name, :state, :version and :default."
  (vec (keep (fn [line]
               (let [default (string/starts-with? (string/trim-start line) "*")
                     words (string/words (string/replace line "*" " "))]
                 (when (and (= (count words) 3) (not= (first words) "NAME"))
                   {name: (first words)
                    state: (keyword (string/lower (second words)))
                    version: (string/to-number (nth words 2))
                    default: default})))
             (string/lines text))))

(def (listed)
  "The installed distributions."
  (parse-list (get (process/run "wsl.exe" ["--list" "--verbose"] :env {WSL_UTF8: "1"}) :stdout)))

(def (default-name)
  "The name of WSL's default distribution."
  (or (get (find :default (listed)) :name)
      (raise (error "No WSL distribution; wsl --install adds one"))))

(def (home-of distro)
  "The Linux path of your home folder in distro."
  (let [ran (process/run "wsl.exe" ["-d" distro "--exec" "sh" "-lc" "printf %s \"$HOME\""])]
    (if (= ran.status 0)
      (string/trim ran.stdout)
      (raise (error $"{distro} didn't start: {(string/trim ran.stderr)}")))))

(def (linux-path path)
  "The path a Windows path such as C:\\Users\\ana has inside a distribution, under /mnt; other paths as they are."
  (if (= (string/slice path 1 2) ":")
    (str "/mnt/" (string/lower (string/slice path 0 1)) (string/replace (string/slice path 2 (count path)) "\\" "/"))
    path))

(test "wsl lists its distributions with the default marked"
  (expect (= (parse-list "  NAME              STATE           VERSION\n* FedoraLinux-44    Running         2\n  docker-desktop    Stopped         2\n")
             [{name: "FedoraLinux-44" state: :running version: 2 default: true}
              {name: "docker-desktop" state: :stopped version: 2 default: false}])))

(test "windows paths reach a distribution under /mnt"
  (expect (= (linux-path "C:\\Users\\ana\\notes.txt") "/mnt/c/Users/ana/notes.txt"))
  (expect (= (linux-path "/home/ana") "/home/ana")))
