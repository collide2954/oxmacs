(module
  (import (ox string) (ox file) (ox path) (ox process)
          (remote hosts :as hosts)
          (remote shell :as shell)
          (remote ops :as ops)
          (customize save :as save)))

(def hosts-buffer "*Remote*")
(def output-buffer "*remote output*")
(def search-buffer "*remote search*")
(def first-row 3)

(defonce state (atom {hosts: [] browsers: {} opened: {} results: [] shells: {}}))

(def (quietly work)
  (try (work)
    (catch error e
      (unless (= e.message "Quit")
        (message e.message)))))

(def (size-text size)
  (cond ((nil? size) "")
    ((< size 1024) (str size))
    ((< size 1048576) (str (// size 1024) "K"))
    (else (str (// size 1048576) "M"))))

(def (host-line host)
  {line: [(string/pad-end (str "  " host.name) 24)
          [(string/pad-end (name (hosts/protocol-of host)) 6) :keyword]
          [(string/pad-end (str (if host.user (str host.user "@") "") (or host.address (if host.shell (string/join " " host.shell) host.name))) 36) :string] " "
          [(or host.from "saved") :comment]]})

(defcommand remote "Browse the files of a host over ssh, sftp or ftp"
  (quietly (fn []
             (let [found (hosts/all-hosts)]
               (swap! state assoc :hosts found)
               (open-buffer hosts-buffer
                            (vec (concat [{line: [["Remote hosts" :heading]]}
                                          {line: [["RET browses  s opens a shell  a adds a host  g refreshes  q quits" :comment]]}
                                          {line: []}]
                                         (if (empty? found)
                                           [{line: [["  No hosts yet: a adds one, or name them in ~/.ssh/config" :comment]]}]
                                           (map host-line found))))
                            :keys {"RET" 'remote-browse "s" 'remote-shell "a" 'remote-add-host "g" 'remote "q" 'quit-window}
                            :line first-row)))))

(def (host-here)
  (cond ((= (buffer-name) hosts-buffer) (get (get @state :hosts) (- (current-line) first-row)))
    ((get-in @state [:browsers (buffer-name)]) (get-in @state [:browsers (buffer-name) :host]))
    (else nil)))

(def (browser-name host dir)
  (str host.name ":" dir))

(def (entry-line entry)
  {line: [[(string/pad-end entry.mode 11) :comment]
          [(string/pad-start (size-text entry.size) 6) :comment] " "
          [(string/pad-end (or entry.date "") 16) :comment] " "
          (cond ((= entry.kind :dir) [(str entry.name "/") :link])
            ((= entry.kind :link) [(str entry.name " -> " entry.target) :symlink])
            (else entry.name))]})

(def browser-keys
  {"RET" 'remote-visit "^" 'remote-up "g" 'remote-refresh "C" 'remote-copy "U" 'remote-upload
   "R" 'remote-rename "D" 'remote-delete "+" 'remote-make-dir "!" 'remote-run "s" 'remote-shell
   "/" 'remote-search "n" 'next-line "p" 'previous-line "q" 'quit-window})

(def (browser-lines host dir entries)
  (vec (concat [{line: [[(browser-name host dir) :heading]]}
                {line: [["RET visits  ^ up  C copies here  U uploads  R renames  D deletes  + folder  ! runs  s shell  / searches  g refreshes" :comment]]}
                {line: []}]
               (if (empty? entries)
                 [{line: [["  (empty)" :comment]]}]
                 (map entry-line entries)))))

(def (browse! host dir)
  (message (str "Listing " (browser-name host dir) "…"))
  (let [entries (ops/list-dir host dir)
        buffer (browser-name host dir)]
    (swap! state assoc-in [:browsers buffer] {host: host dir: dir entries: entries})
    (open-buffer buffer (browser-lines host dir entries) :keys browser-keys :line first-row)
    (message "")))

(defcommand remote-browse "Browse the host on this line"
  (quietly (fn []
             (let [host (or (host-here) (raise (error "No host on this line")))]
               (browse! host (ops/home host))))))

(def (browser)
  (or (get-in @state [:browsers (buffer-name)]) (raise (error "This isn't a remote folder"))))

(def (entry-here)
  (let [found (browser)]
    (get found.entries (- (current-line) first-row))))

(defcommand remote-refresh "List this remote folder again"
  (quietly (fn []
             (let [found (browser)
                   line (current-line)
                   entries (ops/list-dir found.host found.dir)]
               (swap! state assoc-in [:browsers (buffer-name) :entries] entries)
               (set-lines (buffer-name) (browser-lines found.host found.dir entries) :line line)))))

(defcommand remote-up "Browse the folder above this one"
  (quietly (fn []
             (let [found (browser)]
               (browse! found.host (ops/parent-of found.dir))))))

(def (open-remote! host remote {line nil})
  (let [local (ops/local-copy host remote)]
    (message (str "Fetching " remote "…"))
    (await (file/make-dir (path/parent local)))
    (await (file/write local (ops/read-text host remote)))
    (swap! state assoc-in [:opened local] {host: host remote: remote})
    (open-file local)
    (when line
      (goto-line (dec line)))
    (message (str "Editing " (browser-name host remote) "; saving sends it back"))))

(defcommand remote-visit "Open the file or folder on this line"
  (quietly (fn []
             (let [found (browser)
                   entry (or (entry-here) (raise (error "No file on this line")))
                   remote (ops/join-path found.dir entry.name)]
               (if (or (= entry.kind :dir) (and (= entry.kind :link) (string/ends-with? (or entry.target "") "/")))
                 (browse! found.host remote)
                 (open-remote! found.host remote))))))

(on-saved :remote
          (fn [saved]
            (when-let [place (get-in @state [:opened saved])]
              (spawn
                (try
                  (ops/upload! place.host saved place.remote)
                  (message (str "Saved " (browser-name place.host place.remote)))
                  (catch error e
                    (message (str "Couldn't send " place.remote " back: " e.message))))))))

(defcommand remote-copy "Copy the remote file on this line to a local folder"
  (quietly (fn []
             (let [found (browser)
                   entry (or (entry-here) (raise (error "No file on this line")))
                   folder (await (prompt-path (str "Copy " entry.name " to: ") :directories true))
                   local (path/join folder entry.name)
                   remote (ops/join-path found.dir entry.name)]
               (when (= entry.kind :dir)
                 (raise (error "Copy files one at a time; folders aren't copied yet")))
               (message (str "Copying " entry.name "…"))
               (ops/download! found.host remote local
                              :size entry.size
                              :on-progress (fn [share] (message (str "Copying " entry.name "… " (round (* 100 share)) "%"))))
               (message (str "Copied " entry.name " to " (path/abbreviate local)))))))

(defcommand remote-upload "Copy a local file into this remote folder"
  (quietly (fn []
             (let [found (browser)
                   local (await (prompt-path "Upload: "))
                   remote (ops/join-path found.dir (path/name local))]
               (message (str "Uploading " (path/name local) "…"))
               (ops/upload! found.host local remote)
               (run-command 'remote-refresh)
               (message (str "Uploaded " (path/name local)))))))

(defcommand remote-rename "Rename the remote file on this line"
  (quietly (fn []
             (let [found (browser)
                   entry (or (entry-here) (raise (error "No file on this line")))
                   fresh (string/trim (await (prompt "Rename to: " :initial entry.name)))]
               (ops/rename! found.host (ops/join-path found.dir entry.name) (ops/join-path found.dir fresh))
               (run-command 'remote-refresh)
               (message (str "Renamed " entry.name " to " fresh))))))

(defcommand remote-delete "Delete the remote file or folder on this line"
  (quietly (fn []
             (let [found (browser)
                   entry (or (entry-here) (raise (error "No file on this line")))]
               (when (await (yes-or-no (str "Delete " entry.name (if (= entry.kind :dir) " and everything in it" "") " on " found.host.name "? ")))
                 (ops/remove! found.host (ops/join-path found.dir entry.name) (= entry.kind :dir))
                 (run-command 'remote-refresh)
                 (message (str "Deleted " entry.name)))))))

(defcommand remote-make-dir "Make a folder in this remote folder"
  (quietly (fn []
             (let [found (browser)
                   wanted (string/trim (await (prompt "New folder: ")))]
               (ops/make-dir! found.host (ops/join-path found.dir wanted))
               (run-command 'remote-refresh)))))

(defcommand remote-run "Run a command in this remote folder"
  (quietly (fn []
             (let [found (browser)
                   command (await (prompt (str "Run on " found.host.name ": ")))]
               (message (str "Running " command "…"))
               (let [result (ops/run found.host found.dir command)]
                 (open-buffer output-buffer
                              (vec (concat [{line: [[(str found.host.name ":" found.dir "$ " command) :heading]]}]
                                           (map (fn [line] {line: line}) result.lines)
                                           [{line: [[(if (= result.status 0) "Finished" (str "Exited with " result.status)) (if (= result.status 0) :success :error)]]}]))
                              :keys {"q" 'quit-window})
                 (message ""))))))

(defcommand remote-search "Search the files below this remote folder"
  (quietly (fn []
             (let [found (browser)
                   pattern (await (prompt (str "Search " found.dir " for: ")))
                   results (ops/search found.host found.dir pattern)]
               (swap! state assoc :results (vec (map (fn [result] (assoc result :host found.host)) results)))
               (open-buffer search-buffer
                            (vec (concat [{line: [[(str (count results) " lines with " (pr-str pattern) " below " (browser-name found.host found.dir)) :heading]]}
                                          {line: [["RET visits  q quits" :comment]]}
                                          {line: []}]
                                         (map (fn [result] {line: [[(str (string/replace result.path (ops/join-path found.dir "") "") ":" result.line ":") :link] " " result.text]})
                                              results)))
                            :keys {"RET" 'remote-search-visit "q" 'quit-window}
                            :line first-row)))))

(defcommand remote-search-visit "Open the remote file at the line found here"
  (quietly (fn []
             (if-let [result (get (get @state :results) (- (current-line) first-row))]
               (open-remote! result.host result.path :line result.line)
               (message "No result on this line")))))

(def (shell-buffer host)
  (str "*shell " host.name "*"))

(def (pump-shell! buffer channel)
  (when-let [line (receive channel)]
    (append-lines buffer [line])
    (pump-shell! buffer channel)))

(defcommand remote-shell "Open a shell on the host here"
  (quietly (fn []
             (let [host (or (host-here) (raise (error "Put point on a host or in a remote folder first")))
                   buffer (shell-buffer host)
                   running (get-in @state [:shells buffer])]
               (if (and running (not (done? running.exit)))
                 (switch-to-buffer buffer)
                 (let [[program & arguments] (shell/interactive-command host)
                       process (process/start program (vec arguments))]
                   (swap! state assoc-in [:shells buffer] process)
                   (open-buffer buffer [{line: [[(str "Shell on " host.name ", without a terminal: line by line commands work, full-screen programs don't") :comment]]}]
                                :input (str host.name "$ ") :keys {"RET" 'remote-shell-send})
                   (spawn (pump-shell! buffer process.lines))
                   (spawn (pump-shell! buffer process.errors))
                   (spawn (let [status (await process.exit)]
                            (append-lines buffer [[[(str "Shell ended with " status) :comment]]])))))))))

(defcommand remote-shell-send "Send the line typed to the shell"
  (let [process (get-in @state [:shells (buffer-name)])
        line (take-input)]
    (if (and process (not (done? process.exit)))
      (do (append-lines (buffer-name) [[[(str "$ " line) :keyword]]])
        (process/write process (str line "\n")))
      (message "The shell has ended; M-x remote-shell starts another"))))

(defcommand remote-add-host "Save a host to browse"
  (quietly (fn []
             (let [name (string/trim (await (prompt "Name: ")))
                   protocol (await (prompt "Protocol: " :choices ["ssh" "sftp" "ftp"]))
                   address (string/trim (await (prompt "Address: ")))
                   user (string/trim (await (prompt "User (optional): ")))
                   host (merge {name: name protocol: (keyword protocol) address: address}
                               (if (= user "") {} {user: user}))
                   others (vec (remove (fn [known] (= known.name name)) (or (plugin-setting 'remote :hosts) [])))]
               (when (string/blank? name)
                 (raise (error "A host needs a name")))
               (message (if (save/plugin-setting! 'remote :hosts (conj others host))
                          (str "Saved " name " in custom.ox, but init.ox sets the remote hosts too and wins; move them out of init.ox")
                          (str "Saved " name " in custom.ox")))
               (run-command 'remote)))))
