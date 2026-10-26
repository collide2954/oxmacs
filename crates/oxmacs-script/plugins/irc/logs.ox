(module
  (import (ox file) (ox path) (ox string) (ox time))
  (export folder append! search entry-line))

(defonce made (atom (set [])))

(def (folder)
  "Where IRC logs are kept: irc/logs in the oxmacs home folder."
  (when-let [home (oxmacs-home)]
    (path/join home "irc" "logs")))

(def (safe text)
  (reduce (fn [out c] (string/replace out c "_")) text ["/" "\\" ":" "*" "?" "\"" "<" ">" "|"]))

(def (entry-line message)
  "message as one line of a log."
  (let [nick (or message.nick "")
        text (string/replace (or message.text "") "\n" " ")
        said (cond ((= message.kind :privmsg) (str "<" nick "> " text))
               ((= message.kind :action) (str "* " nick " " text))
               ((= message.kind :notice) (str "-" nick "- " text))
               (else (str "*** " text)))]
    (str (time/format message.time "%H:%M:%S") " " said)))

(def (append! network target message)
  "Adds message to the day's log of target on network."
  (when-let [root (folder)]
    (let [place (path/join root (safe network) (safe target))]
      (unless (contains? @made place)
        (await (file/make-dir place))
        (swap! made conj place))
      (await (file/append (path/join place (str (time/format message.time "%Y-%m-%d") ".log"))
                          (str (entry-line message) "\n"))))))

(def (dirs-in folder)
  (if (await (file/exists? folder))
    (vec (filter :dir (await (file/list folder))))
    []))

(def (matches-in file query network target)
  (let [lines (string/lines (await (file/read file.path)))
        wanted (string/lower query)]
    (vec (keep (fn [[number line]]
                 (when (string/includes? (string/lower line) wanted)
                   {network: network target: target day: (path/stem file.name) line: number text: line path: file.path}))
               (zip (range (count lines)) lines)))))

(def (search query)
  "Every logged line containing query, ignoring case, as maps of :network, :target, :day, :line, :text and :path, oldest first."
  (if-let [root (folder)]
    (vec (mapcat (fn [network]
                   (mapcat (fn [target]
                             (mapcat (fn [file] (matches-in file query network.name target.name))
                                     (sort-by :name (filter (fn [file] (string/ends-with? file.name ".log"))
                                                            (await (file/list target.path))))))
                           (dirs-in network.path)))
                 (dirs-in root)))
    []))
