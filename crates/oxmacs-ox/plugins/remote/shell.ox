(module
  (import (ox process) (ox string) (ox crypto))
  (export run! quote command-of close! interactive-command))

(defonce sessions (atom {}))
(defonce locks (atom {}))

(def (quote text)
  "text as one word for a POSIX shell."
  (str "'" (string/replace text "'" "'\\''") "'"))

(def (target host)
  (let [address (or host.address host.name)]
    (if host.user (str host.user "@" address) address)))

(def (ssh-words host)
  (vec (concat ["ssh" "-T" "-o" "BatchMode=yes"]
               (if host.port ["-p" (str host.port)] [])
               [(target host)])))

(def (command-of host)
  "The program and arguments that start a shell on host."
  (if host.shell
    (vec host.shell)
    (conj (ssh-words host) "sh")))

(def (interactive-command host)
  "The program and arguments for a shell you type into on host."
  (if host.shell
    (vec host.shell)
    (ssh-words host)))

(def (live host)
  (when-let [session (get @sessions host.name)]
    (when (not (done? session.process.exit))
      session.process)))

(def (start! host)
  (let [[program & arguments] (command-of host)
        process (process/start program (vec arguments))]
    (swap! sessions assoc host.name {process: process})
    process))

(def (drain channel found)
  (if-let [line (receive channel)]
    (drain channel (conj found line))
    found))

(def (close! host)
  "Ends the shell kept open on host."
  (when-let [process (live host)]
    (process/close-input process)
    (process/kill process))
  (swap! sessions dissoc host.name))

(def (exclusive name work)
  (let [mine (promise)
        before (atomically
                 (let [old (get @locks name)]
                   (swap! locks assoc name mine)
                   old))]
    (when before
      (try (await before) (catch error _ nil)))
    (try (work)
      (finally (deliver mine true)))))

(def (read-until lines marker found on-line)
  (when-let [line (receive lines)]
    (if-let [at (string/index-of line marker)]
      (let [before (string/slice line 0 at)]
        {status: (or (string/to-number (string/trim (string/slice line (+ at (count marker))))) 1)
         lines: (if (= before "") found (conj found before))})
      (do (when on-line (on-line line))
        (read-until lines marker (conj found line) on-line)))))

(def (ended host)
  (let [process (get-in @sessions [host.name :process])
        errors (if process
                 (do (await process.exit)
                   (drain process.errors []))
                 [])]
    (swap! sessions dissoc host.name)
    (error (str "The connection to " host.name " ended"
                (if (empty? errors) "" (str ": " (string/join " " (take 5 errors))))))))

(def (run! host command {on-line nil})
  "Runs command in the shell kept open on host, starting it when needed, and returns its :status and :lines, errors included. on-line is called with each line as it comes."
  (exclusive host.name
             (fn []
               (let [process (or (live host) (start! host))
                     marker (str "__ox_" (crypto/random 6) "__")]
                 (unless (process/write process (str "{{ " command "\n} 2>&1; printf '%s %s\\n' '" marker "' \"$?\"\n"))
                   (raise (ended host)))
                 (or (read-until process.lines marker [] on-line)
                     (raise (ended host)))))))
