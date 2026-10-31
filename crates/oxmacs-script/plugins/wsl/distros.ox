(module
  (import (ox string) (ox process) (ox file) (ox path))
  (export parse-list listed default-name home-of linux-path situation detect notice))

(def (parse-list text)
  "The distributions wsl --list --verbose names, as maps of :name, :state, :version and :default."
  (vec (keep (fn [line]
               (let [default (string/starts-with? (string/trim-start line) "*")
                     words (string/words (string/replace line "*" " "))]
                 (when (= (count words) 3)
                   (when-let [version (string/to-number (nth words 2))]
                     {name: (first words)
                      state: (keyword (string/lower (second words)))
                      version: version
                      default: default}))))
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

(def (situation os version found)
  "What the wsl plugin can do: :ready, :no-distributions, :wsl-1-only, :no-wsl or :not-windows, from the system, how wsl --version ran (nil when it couldn't) and the distributions found."
  (cond
    [(not= os :windows) :not-windows]
    [(some (fn [distro] (= distro.version 2)) found) :ready]
    [(not (empty? found)) :wsl-1-only]
    [(and version (= version.status 0)) :no-distributions]
    [else :no-wsl]))

(def (ran-quietly program args)
  (try (process/run program args :env {WSL_UTF8: "1"}) (catch error _e nil)))

(def (installed-wsl)
  "The wsl.exe WSL 2 is installed as, or nil; the one in Windows' own folder only offers to install WSL, so it is never run to find out."
  (when-let [programs (process/env "ProgramFiles")]
    (let [wsl (path/join programs "WSL" "wsl.exe")]
      (when (await (file/exists? wsl)) wsl))))

(def (detect)
  "What the wsl plugin can do on this computer, as situation tells it."
  (if (not= (process/os) :windows)
    (situation (process/os) nil [])
    (if-let [wsl (installed-wsl)]
      (situation :windows (ran-quietly wsl ["--version"]) (try (listed) (catch error _e [])))
      (situation :windows nil []))))

(def (system-name os)
  (get {linux: "Linux" macos: "macOS"} os (name os)))

(def (notice found os)
  "What to tell the reader when the wsl plugin can't work, for a situation; nil when it can."
  (get {not-windows: $"WSL is part of Windows, so the wsl plugin has nothing to do on {(system-name os)}"
        no-wsl: "WSL 2 isn't installed: run wsl --install in a terminal as administrator (wsl --update for an older WSL), restart Windows, then try again"
        wsl-1-only: "Your distributions run on WSL 1, and the wsl plugin needs WSL 2: wsl --set-version NAME 2 moves one over"
        no-distributions: "No WSL distributions yet: wsl --install -d Ubuntu adds one"}
       found))

(test "wsl lists its distributions with the default marked"
  (expect (= (parse-list "  NAME              STATE           VERSION\n* FedoraLinux-44    Running         2\n  docker-desktop    Stopped         2\n")
             [{name: "FedoraLinux-44" state: :running version: 2 default: true}
              {name: "docker-desktop" state: :stopped version: 2 default: false}])))

(test "wsl's messages are not distributions"
  (expect (= (parse-list "Windows Subsystem for Linux has no installed distributions.\nUse wsl --list --online\n") []))
  (expect (= (parse-list "Pas de distribution\n") [])))

(test "windows paths reach a distribution under /mnt"
  (expect (= (linux-path "C:\\Users\\ana\\notes.txt") "/mnt/c/Users/ana/notes.txt"))
  (expect (= (linux-path "/home/ana") "/home/ana")))

(test "the plugin works with WSL 2 on Windows and says why not elsewhere"
  (let [two {name: "Fedora" state: :running version: 2 default: true}
        one {name: "Old" state: :stopped version: 1 default: false}
        worked {status: 0 stdout: "WSL version: 2.4.13" stderr: ""}
        failed {status: 1 stdout: "" stderr: ""}]
    (expect (= (situation :linux nil []) :not-windows))
    (expect (= (situation :macos nil []) :not-windows))
    (expect (= (situation :windows worked [one two]) :ready))
    (expect (= (situation :windows failed [two]) :ready))
    (expect (= (situation :windows worked [one]) :wsl-1-only))
    (expect (= (situation :windows worked []) :no-distributions))
    (expect (= (situation :windows failed []) :no-wsl))
    (expect (= (situation :windows nil []) :no-wsl))
    (expect (= (notice :ready :windows) nil))
    (expect (= (notice :not-windows :macos) "WSL is part of Windows, so the wsl plugin has nothing to do on macOS"))
    (expect (string/starts-with? (notice :no-wsl :windows) "WSL 2 isn't installed"))))
