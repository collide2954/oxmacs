(module
  (import (ox string) (ox process) (ox wsl)
          (wsl distros :as distros)
          (remote hosts :as hosts)
          (remote shell :as shell)))

(def page "*WSL*")
(def output "*wsl output*")

(wsl/route! true)

(def (host-of distro)
  {name: distro.name protocol: :wsl shell: ["wsl.exe" "-d" distro.name]
   folder: (wsl/share distro.name "/") from: "WSL"})

(hosts/add-hosts! (fn [] (try (vec (map host-of (distros/listed))) (catch error _ []))))

(def (distro-line distro)
  {line: [[(if distro.default "* " "  ") :keyword]
          (string/pad-end distro.name 26)
          [(string/pad-end (name distro.state) 12) (if (= distro.state :running) :string :comment)]
          [$"WSL {distro.version}" :comment]]
   distro: distro})

(def (show-page)
  (let [found (distros/listed)
        buffer (open-buffer page
                            (if (empty? found)
                              [[["  No distributions yet: wsl --install adds one" :comment]]]
                              (vec (map distro-line found)))
                            :keys {"RET" 'wsl-open-home "s" 'wsl-shell "r" 'wsl-run "f" 'wsl-find-file
                                   "S" 'wsl-start "k" 'wsl-terminate "d" 'wsl-set-default "X" 'wsl-shutdown
                                   "g" 'wsl "q" 'quit-window}
                            :line 0)]
    (set-header buffer [[["WSL distributions" :heading] ["   * is the default" :comment]]
                        [["RET opens home  f finds a file  s shell  r runs  S starts  k stops  d makes default  X shuts WSL down  g refreshes  q quits" :comment]]])))

(defcommand wsl "List the WSL distributions to open, start, stop and work in"
  (show-page))

(def (refresh)
  (when (buffer-live? page)
    (show-page)))

(def (here)
  "Where the current folder lies inside a distribution, or nil."
  (wsl/place (default-directory)))

(def (target)
  "The distribution on this line of *WSL*, else the current folder's, else WSL's default."
  (or (get-in (line-item) [:distro :name])
      (get (here) :distro)
      (distros/default-name)))

(defcommand wsl-open-home "Open your home folder in the distribution here"
  (let [distro (target)]
    (open-file (wsl/share distro (distros/home-of distro)))))

(defcommand wsl-start "Start the distribution here"
  (let [distro (target)]
    (process/run "wsl.exe" ["-d" distro "--exec" "true"])
    (message $"Started {distro}")
    (refresh)))

(defcommand wsl-terminate "Stop the distribution here and everything running in it"
  (let [distro (target)]
    (when (yes-or-no $"Stop {distro} and everything running in it?")
      (process/run "wsl.exe" ["--terminate" distro])
      (message $"Stopped {distro}")
      (refresh))))

(defcommand wsl-set-default "Make the distribution here WSL's default"
  (let [distro (target)]
    (process/run "wsl.exe" ["--set-default" distro])
    (message $"{distro} is WSL's default now")
    (refresh)))

(defcommand wsl-shutdown "Stop every WSL distribution and WSL itself"
  (when (yes-or-no "Shut WSL down, stopping every distribution?")
    (process/run "wsl.exe" ["--shutdown"])
    (message "WSL is shut down")
    (refresh)))

(def (folder-in distro)
  "The current folder's Linux path when it lies inside distro, else ~."
  (let [place (here)]
    (if (and place (= place.distro distro)) place.path "~")))

(defcommand wsl-shell "Open a shell in a WSL distribution, at the current folder when it lies inside it"
  (let [distro (target)
        name $"*{distro} shell*"]
    (unless (console-running? name)
      (console-start name :program "wsl.exe" :args ["-d" distro "--cd" (folder-in distro)]
                     :label "Shell" :prompter shell/prompter))
    (display-buffer name :select true)))

(defcommand wsl-run "Run a command in a WSL distribution, in the current folder when it lies inside it"
  (let [distro (target)
        folder (folder-in distro)
        cwd (wsl/share distro (if (= folder "~") (distros/home-of distro) folder))
        command (prompt $"Run in {distro}: ")]
    (run-job output "sh" ["-c" command] :cwd cwd :title $"{distro}:{folder}$ {command}")))

(defcommand wsl-find-file "Open a file by its Linux path in a WSL distribution"
  (let [distro (target)
        place (here)
        start (cond ((nil? place) "~/") ((not= place.distro distro) "~/") ((string/ends-with? place.path "/") place.path) (else (str place.path "/")))
        path (prompt $"Find file in {distro}: " :initial start)
        home (when (string/starts-with? path "~") (distros/home-of distro))
        linux (if home (str home (string/slice path 1 (count path))) path)]
    (open-file (wsl/share distro linux))))

(defcommand wsl-copy-linux-path "Copy the Linux path of the current file or folder, under /mnt for Windows files"
  (let [path (or (buffer-file) (default-directory))
        place (wsl/place path)
        linux (if place place.path (distros/linux-path path))]
    (copy-text linux)
    (message $"Copied {linux}")))
