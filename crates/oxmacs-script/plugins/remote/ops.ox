(module
  (import (ox string) (ox regex) (ox file) (ox path) (ox process) (ox crypto)
          (remote shell :as shell)
          (remote hosts :as hosts)
          (vault store :as vault))
  (export home list-dir parse-listing read-text download! upload! remove! rename! make-dir! run search join-path
          parent-of ftp-url can-run? transfer-file local-copy))

(def listing-pattern
  "^([-dlbcpsD])([-rwxsStT]{{9})\\S*\\s+\\d+\\s+\\S+\\s+\\S+\\s+(\\d+)\\s+(\\d{{4}-\\d{{2}-\\d{{2} \\d{{2}:\\d{{2}|[A-Z][a-z]{{2}\\s+\\d{{1,2}\\s+(?:\\d{{4}|\\d{{1,2}:\\d{{2}))\\s(.*)$")

(def (join-path dir name)
  (if (string/ends-with? dir "/") (str dir name) (str dir "/" name)))

(def (parent-of dir)
  "The folder holding dir, which is itself for /."
  (let [trimmed (if (and (> (count dir) 1) (string/ends-with? dir "/")) (string/slice dir 0 (dec (count dir))) dir)
        back (string/index-of (string/reverse trimmed) "/")]
    (cond ((nil? back) "/")
      (else (let [at (- (count trimmed) 1 back)]
              (if (= at 0) "/" (string/slice trimmed 0 at)))))))

(def (entry-of line)
  (when-let [match (regex/find listing-pattern line)]
    (let [[kind mode size date rest] match.groups
          link (= kind "l")
          [shown target] (if link (string/split rest " -> " 2) [rest nil])
          shown (last (string/split shown "/"))]
      (when (and (not= shown ".") (not= shown ".."))
        {name: shown
         kind: (cond ((= kind "d") :dir) (link :link) (else :file))
         mode: (str kind mode)
         size: (string/to-number size)
         date: date
         target: target}))))

(def (parse-listing lines)
  "The entries of ls -l output, each a map of :name, :kind, :mode, :size, :date and :target, folders first and then by name."
  (vec (sort-by (fn [entry] [(if (= entry.kind :dir) 0 1) (string/lower entry.name)])
                (keep entry-of lines))))

(def (can-run? host)
  "True when host has a shell to run commands in."
  (= (hosts/protocol-of host) :ssh))

(def (checked found what)
  (when (not= found.status 0)
    (raise (error (str what ": " (string/join " " (take 3 found.lines))))))
  found)

(def (credentials host)
  (if host.login
    (do (vault/unlock)
      (if-let [entry (vault/find-entry host.login)]
        {user: (or entry.username host.user "anonymous") password: entry.password}
        (raise (error (str "The vault has no entry called " host.login)))))
    {user: (or host.user "anonymous") password: (if host.user "" "anonymous")}))

(def (ftp-url host remote)
  (str (if host.tls "ftps://" "ftp://") (or host.address host.name) (if host.port (str ":" host.port) "")
       (if (string/starts-with? remote "/") "" "/") remote))

(def (curl host remote arguments)
  (let [login (credentials host)
        config (str "user = " (pr-str (str login.user ":" login.password)) "\n")
        found (await (process/run "curl" (vec (concat ["-s" "-S" "-K" "-"] arguments [(ftp-url host remote)])) :input config))]
    (when (not= found.status 0)
      (raise (error (string/trim (str "curl: " found.stderr)))))
    found))

(def (sftp host commands)
  (let [found (await (process/run "sftp" (vec (concat ["-b" "-" "-o" "BatchMode=yes"] (if host.port ["-P" (str host.port)] [])
                                                      [(if host.user (str host.user "@" (or host.address host.name)) (or host.address host.name))]))
                                  :input (str (string/join "\n" commands) "\n")))]
    (when (not= found.status 0)
      (raise (error (string/trim (str "sftp: " found.stderr)))))
    found))

(def (transfer-file)
  "A file of its own in the oxmacs home for one sftp transfer to pass through."
  (path/join (oxmacs-home) "remote" (str ".transfer-" (crypto/random 8))))

(def unsafe-characters [":" "<" ">" "\"" "|" "?" "*" "/" "\\"])

(def (safe-name text)
  (reduce (fn [out bad] (string/replace out bad "_")) text unsafe-characters))

(def (local-copy host remote)
  "The local file a remote file on host is edited through, inside the oxmacs home's remote folder."
  (let [parts (remove (fn [part] (contains? #{"" "." ".."} part))
                      (string/split (string/replace remote "\\" "/") "/"))]
    (apply path/join (concat [(oxmacs-home) "remote" (safe-name host.name)] (map safe-name parts)))))

(def (sftp-word text)
  (str "\"" (string/replace (string/replace text "\\" "\\\\") "\"" "\\\"") "\""))

(def (home host)
  "The folder a host starts in."
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) "/")
      ((= protocol :sftp)
       (let [found (sftp host ["pwd"])]
         (or (some (fn [line] (when-let [match (regex/find "^Remote working directory: (.*)$" line)] (first match.groups)))
                   (string/lines found.stdout))
             "/")))
      (else (or (first (get (checked (shell/run! host "pwd") "pwd") :lines)) "/")))))

(def (list-dir host dir)
  "The entries of dir on host."
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp)
           (parse-listing (string/lines (get (curl host (if (string/ends-with? dir "/") dir (str dir "/")) []) :stdout))))
      ((= protocol :sftp)
       (parse-listing (string/lines (get (sftp host [(str "ls -ln " (sftp-word dir))]) :stdout))))
      (else
       (parse-listing (get (checked (shell/run! host (str "cd -- " (shell/quoted dir) " && (ls -lAn --time-style=long-iso 2>/dev/null || ls -lAn)"))
                                    (str "Couldn't list " dir))
                           :lines))))))

(def (base64-of host remote on-line)
  (get (checked (shell/run! host (str "base64 < " (shell/quoted remote)) :on-line on-line) (str "Couldn't read " remote)) :lines))

(def (read-text host remote)
  "The text of the file remote on host."
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) (get (curl host remote []) :stdout))
      ((= protocol :sftp)
       (let [local (transfer-file)]
         (await (file/make-dir (path/parent local)))
         (try (sftp host [(str "get " (sftp-word remote) " " (sftp-word local))])
           (await (file/read local))
           (finally (when (await (file/exists? local))
                      (await (file/remove local)))))))
      (else (crypto/from-base64 (string/join "" (base64-of host remote nil)))))))

(def (download! host remote local {size nil on-progress nil})
  "Copies the file remote on host to local, calling on-progress with the share copied so far."
  (let [protocol (hosts/protocol-of host)]
    (await (file/make-dir (path/parent local)))
    (cond ((= protocol :ftp) (curl host remote ["-o" local]))
      ((= protocol :sftp) (sftp host [(str "get " (sftp-word remote) " " (sftp-word local))]))
      (else
       (let [expected (max 1 (* 4 (/ (or size 0) 3)))
             seen (atom 0)
             lines (base64-of host remote
                              (fn [line]
                                (let [before @seen
                                      now (swap! seen + (inc (count line)))]
                                  (when (and on-progress size (> size 65536)
                                             (not= (// (* 10 before) expected) (// (* 10 now) expected)))
                                    (on-progress (min 1 (/ now expected)))))))]
         (await (file/write-base64 local (string/join "" lines))))))
    local))

(def (chunks text size)
  (if (<= (count text) size)
    [text]
    (vec (map (fn [at] (string/slice text at (min (count text) (+ at size)))) (range 0 (count text) size)))))

(def (upload! host local remote)
  "Copies the file local to remote on host."
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) (curl host remote ["-T" local]))
      ((= protocol :sftp) (sftp host [(str "put " (sftp-word local) " " (sftp-word remote))]))
      (else
       (let [encoded (await (file/read-base64 local))
             end (str "__OX_END_" (crypto/random 4) "__")]
         (checked (shell/run! host (str "if printf '' | base64 -d >/dev/null 2>&1; then D='base64 -d'; else D='base64 -D'; fi; $D > "
                                        (shell/quoted remote) " <<'" end "'\n" (string/join "\n" (chunks encoded 76)) "\n" end))
                  (str "Couldn't write " remote)))))
    remote))

(def (remove! host remote folder)
  "Deletes remote on host, and what is in it when it is a folder."
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) (curl host "/" ["-Q" (str (if folder "RMD " "DELE ") remote)]))
      ((= protocol :sftp) (sftp host [(str (if folder "rmdir " "rm ") (sftp-word remote))]))
      (else (checked (shell/run! host (str "rm -r -- " (shell/quoted remote))) (str "Couldn't delete " remote))))))

(def (rename! host from to)
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) (curl host "/" ["-Q" (str "RNFR " from) "-Q" (str "RNTO " to)]))
      ((= protocol :sftp) (sftp host [(str "rename " (sftp-word from) " " (sftp-word to))]))
      (else (checked (shell/run! host (str "mv -- " (shell/quoted from) " " (shell/quoted to))) (str "Couldn't rename " from))))))

(def (make-dir! host remote)
  (let [protocol (hosts/protocol-of host)]
    (cond ((= protocol :ftp) (curl host "/" ["-Q" (str "MKD " remote)]))
      ((= protocol :sftp) (sftp host [(str "mkdir " (sftp-word remote))]))
      (else (checked (shell/run! host (str "mkdir -p -- " (shell/quoted remote))) (str "Couldn't make " remote))))))

(def (run host dir command)
  "Runs command in dir on host, returning its :status and :lines."
  (unless (can-run? host)
    (raise (error (str host.name " has no shell to run commands in"))))
  (shell/run! host (str "cd -- " (shell/quoted dir) " && {{ " command "\n} </dev/null")))

(def (search host dir pattern)
  "The lines below dir on host that hold pattern, each a map of :path, :line and :text."
  (let [found (run host dir (str "grep -rnI -- " (shell/quoted pattern) " . | head -n 1000"))]
    (vec (keep (fn [line]
                 (when-let [match (regex/find #"^\./(.*?):(\d+):(.*)$" line)]
                   (let [[file number text] match.groups]
                     {path: (join-path dir file) line: (string/to-number number) text: text})))
               found.lines))))

(test "ls listings are read with folders first and links apart"
  (expect (= (map (fn [entry] [entry.name entry.kind entry.size entry.target])
                  (parse-listing ["total 12"
                                  "drwxr-xr-x 2 1000 1000 4096 2026-10-23 09:30 src"
                                  "-rw-r--r-- 1 1000 1000  120 2026-10-22 18:01 notes.txt"
                                  "lrwxrwxrwx 1 1000 1000    9 2026-10-23 09:31 latest -> notes.txt"
                                  "-rw-r--r--  1 501  20    64 Oct 23 09:30 bsd file.txt"
                                  "drwxr-xr-x 2 1000 1000 4096 2026-10-23 09:30 ."]))
             [["src" :dir 4096 nil] ["bsd file.txt" :file 64 nil] ["latest" :link 9 "notes.txt"] ["notes.txt" :file 120 nil]])))

(test "remote paths join and climb"
  (expect (= (parent-of "/a/b/") "/a"))
  (expect (= (parent-of "/a") "/"))
  (expect (= (parent-of "/") "/"))
  (expect (= (join-path "/a/" "b") "/a/b")))
