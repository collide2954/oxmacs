(module
  (import (ox string) (ox file) (ox path) (ox http) (ox regex)
          (irc protocol :as protocol)
          (irc format :as format)
          (irc client :as client)
          (irc logs :as logs)
          (vault store :as vault)
          (customize form :as form)
          (customize written :as written)))

(def nicks-buffer "*irc nicks*")
(def search-buffer "*irc search*")
(def reactions ["👍" "❤️" "😂" "🎉" "👀" "🙏" "✅"])
(def slash-commands ["join" "part" "me" "msg" "query" "notice" "nick" "quit" "reconnect" "topic" "whois" "names"
                     "list" "away" "back" "invite" "kick" "quote" "clear" "noise" "help"])

(defonce completing (atom nil))
(defonce found (atom []))

(def (quietly work)
  (try (work)
    (catch error e
      (unless (= e.message "Quit")
        (message e.message)))))

(def (here)
  (client/buffer-entry (buffer-name)))

(def (here-or-complain)
  (or (here) (raise (error "This isn't an IRC buffer"))))

(def (networks)
  (or (plugin-setting 'irc :networks) []))

(def (connect-all!)
  (let [known (networks)]
    (if (empty? known)
      (run-command 'irc-setup)
      (do (client/connect! (first known) :show true)
        (reduce (fn [_ config]
                  (unless (client/connected? config.name)
                    (spawn (client/connect! config))))
                nil (rest known))))))

(defcommand irc "Connect to your IRC networks"
  (quietly (fn []
             (let [known (networks)
                   first-one (first known)]
               (if (and first-one (client/connected? first-one.name))
                 (switch-to-buffer (client/server-buffer first-one.name))
                 (connect-all!))))))

(def (networks-entry? entry)
  (and (= entry.plugin "irc") (= entry.key :networks)))

(def (save-networks! all)
  (configure-plugin 'irc {networks: all})
  (let [file (path/join (oxmacs-home) "custom.ox")
        found (form/entries (settings-schema) (settings) (saved-settings) (custom-settings) (plugin-settings))
        unsaved (some (fn [entry] (and entry.changed (not (networks-entry? entry)))) found)
        mine (find networks-entry? found)
        overridden (not= mine.saved mine.custom)
        entries (map (fn [entry] (assoc entry :changed (networks-entry? entry))) found)]
    (await (file/make-dir (path/parent file)))
    (await (file/write file (written/text entries)))
    (unless (or unsaved overridden)
      (reload-init))
    overridden))

(defcommand irc-setup "Add an IRC network to your settings"
  (quietly (fn []
             (let [name (string/trim (await (prompt "Network name: " :initial "Libera")))
                   host (string/trim (await (prompt "Server: " :initial "irc.libera.chat")))
                   port (or (string/to-number (await (prompt "Port: " :initial "6697"))) 6697)
                   tls (await (yes-or-no "Use TLS? "))
                   nick (string/trim (await (prompt "Nick: ")))
                   login (when (await (yes-or-no "Log in with a SASL account? "))
                           (let [account (string/trim (await (prompt "Account: " :initial nick)))
                                 password (await (prompt "Account password (kept in the vault): " :secret true))
                                 title (str "IRC " name)]
                             (vault/unlock)
                             (if-let [old (vault/find-entry title)]
                               (vault/change! old.id (fn [entry] (merge entry {username: account password: password})))
                               (vault/add! {title: title username: account password: password url: (str "ircs://" host) totp: "" notes: ""}))
                             title))
                   channels (vec (string/words (await (prompt "Channels to join: " :initial "#oxmacs"))))
                   config (merge {name: name host: host port: port tls: tls nick: nick}
                                 (if login {login: login} {})
                                 {channels: channels})
                   others (vec (remove (fn [known] (= known.name name)) (networks)))]
               (when (string/blank? nick)
                 (raise (error "A network needs a nick")))
               (when (nil? (oxmacs-home))
                 (raise (error "There is no oxmacs home folder to save the network in")))
               (message (if (save-networks! (conj others config))
                          (str "Saved " name " in custom.ox, but init.ox sets the IRC networks too and wins; move them out of init.ox")
                          (str "Saved " name " in custom.ox")))
               (when (await (yes-or-no (str "Connect to " name " now? ")))
                 (client/connect! config :show true))))))

(def (send-text! buffer typed)
  (let [text (string/replace (string/replace typed "\r\n" "\n") "\r" "\n")]
    (if (string/includes? text "\n")
      (let [lines (vec (remove string/blank? (string/lines text)))
            entry (client/buffer-entry buffer)]
        (when (await (yes-or-no (str "Send " (count lines) " lines to " entry.target "? ")))
          (client/say-lines! buffer lines)))
      (client/say! buffer text))))

(def (words-after text)
  (let [[command rest] (let [at (string/index-of text " ")]
                         (if at [(string/slice text 0 at) (string/trim (string/slice text (inc at)))] [text ""]))]
    [(string/lower command) rest]))

(def (first-word text)
  (let [at (string/index-of text " ")]
    (if at [(string/slice text 0 at) (string/trim (string/slice text (inc at)))] [text ""])))

(def (run-slash! buffer text)
  (let [entry (client/buffer-entry buffer)
        name entry.network
        [command rest] (words-after (string/slice text 1))
        [word more] (first-word rest)
        here-target (when (not= entry.kind :server) entry.target)]
    (cond ((= command "join") (client/join! name word))
      ((= command "part") (if (protocol/channel? word)
                            (client/part! name word (when (not= more "") more))
                            (client/part! name here-target (when (not= rest "") rest))))
      ((= command "me") (client/act! buffer rest))
      ((= command "msg") (let [target (client/ensure-buffer! name word (if (protocol/channel? word) :channel :query))]
                           (client/say! target more)))
      ((= command "query") (client/ensure-buffer! name word :query :show true))
      ((= command "notice") (client/send-command! name "NOTICE" [word more]))
      ((= command "nick") (client/send-command! name "NICK" [word]))
      ((= command "quit") (client/quit! name (when (not= rest "") rest)))
      ((= command "reconnect") (client/reconnect! name))
      ((= command "list") (client/list! name rest))
      ((= command "topic") (client/send-command! name "TOPIC" (if (= rest "") [here-target] [here-target rest])))
      ((= command "whois") (let [nick (if (and (= word "") (= entry.kind :query)) entry.target word)]
                             (if (= nick "")
                               (message "Whom? /whois takes a nick")
                               (client/whois! buffer nick))))
      ((= command "names") (client/info! buffer (str (count (client/channel-nicks name here-target)) " people: "
                                                     (string/join " " (sort (map (fn [person] (str person.prefix person.nick))
                                                                                 (client/channel-nicks name here-target)))))))
      ((= command "away") (client/send-command! name "AWAY" [(if (= rest "") "Away" rest)]))
      ((= command "back") (client/send-command! name "AWAY" []))
      ((= command "invite") (client/send-command! name "INVITE" [word (if (= more "") here-target more)]))
      ((= command "kick") (client/send-command! name "KICK" (if (= more "") [here-target word] [here-target word more])))
      ((or (= command "quote") (= command "raw")) (client/send! name rest))
      ((= command "clear") (client/clear-buffer! buffer))
      ((= command "noise") (run-command 'irc-toggle-noise))
      ((= command "help") (client/info! buffer (string/join " " (map (fn [known] (str "/" known)) slash-commands))))
      (else (client/send! name (string/trim (str (string/upper command) " " rest)))))))

(defcommand irc-return "Send the input, or open the link at point"
  (quietly (fn []
             (let [buffer (buffer-name)]
               (if (nil? (input-point))
                 (if-let [url (format/url-at (or (line-text (current-line)) "") (current-column))]
                   (open-externally url)
                   (run-command 'end-of-buffer))
                 (let [entry (here-or-complain)
                       text (take-input)]
                   (cond ((string/blank? text) nil)
                     ((contains? (set ["/quit" "/reconnect"]) (string/lower (first (string/words text))))
                      (run-slash! buffer text))
                     ((not (client/connected? entry.network))
                      (message (str "Not connected to " entry.network "; M-x irc connects")))
                     ((and (string/starts-with? text "/") (not (string/starts-with? text "//")))
                      (run-slash! buffer text))
                     ((= entry.kind :server)
                      (message "Messages go to channels and people: /join a channel or /msg someone"))
                     (else (send-text! buffer (if (string/starts-with? text "//") (string/slice text 1) text))))))))))

(def (word-start text at)
  (let [before (string/slice text 0 at)
        back (string/index-of (string/reverse before) " ")]
    (if back (- at back) 0)))

(def (candidates entry prefix)
  (let [wanted (protocol/casefold prefix)
        people (map :nick (client/channel-nicks entry.network entry.target))
        recent (reverse (distinct (keep :nick (reverse entry.messages))))
        ordered (distinct (concat (reverse recent) people))]
    (vec (filter (fn [nick] (and (string/starts-with? (protocol/casefold nick) wanted)
                                 (not (protocol/same-nick? nick (client/my-nick entry.network)))))
                 ordered))))

(def (completions entry start prefix)
  (cond ((and (= start 0) (string/starts-with? prefix "/"))
         {found: (vec (filter (fn [command] (string/starts-with? command (string/lower prefix)))
                              (map (fn [command] (str "/" command)) slash-commands)))
          suffix: " " kind: "command"})
    ((string/starts-with? prefix "#")
     {found: (vec (filter (fn [channel] (string/starts-with? (protocol/casefold channel) (protocol/casefold prefix)))
                          (client/joined-channels entry.network)))
      suffix: " " kind: "channel"})
    (else {found: (candidates entry prefix) suffix: (if (= start 0) ": " " ") kind: "nick"})))

(defcommand irc-complete "Complete the nick, channel or command before point; again for the next one"
  (let [entry (here)
        text (input-text)
        at (input-point)]
    (when (and entry text at)
      (let [last-one @completing
            again (and last-one (= last-one.text text) (= last-one.at at) (= last-one.buffer (buffer-name)))
            start (if again last-one.start (word-start text at))
            prefix (if again last-one.prefix (string/slice text start at))
            offered (if again last-one.offered (completions entry start prefix))
            found offered.found
            index (if again (mod (inc last-one.index) (max 1 (count found))) 0)]
        (if (empty? found)
          (message (str "No " offered.kind " starts with " prefix))
          (let [chosen (nth found index)
                suffix offered.suffix
                fresh (str (string/slice text 0 start) chosen suffix (string/slice text at))
                point (+ start (count chosen) (count suffix))]
            (set-input (buffer-name) fresh :point point)
            (reset! completing {buffer: (buffer-name) text: fresh at: point start: start prefix: prefix offered: offered index: index})))))))

(def (nick-lines entry)
  (let [people (client/channel-nicks entry.network entry.target)
        group (fn [title wanted]
                (let [chosen (sort-by (fn [person] (protocol/casefold person.nick)) (filter wanted people))]
                  (if (empty? chosen)
                    []
                    (vec (concat [{line: [[(str title " " (count chosen)) :heading]]}]
                                 (map (fn [person]
                                        {line: [[(str person.prefix person.nick) (if person.away :comment (protocol/nick-face person.nick))]]})
                                      chosen)
                                 [{line: []}])))))
        operator (fn [person] (some (fn [mark] (string/includes? person.prefix mark)) ["~" "&" "@" "%"]))
        voiced (fn [person] (and (not (operator person)) (string/includes? person.prefix "+")))
        plain (fn [person] (and (not (operator person)) (not (voiced person))))]
    (vec (concat [{line: [[entry.target :heading] [(str "  " (count people) " people") :comment]]} {line: []}]
                 (group "Operators" operator)
                 (group "Voiced" voiced)
                 (group "Everyone" plain)))))

(defcommand irc-nicks "Show or hide the people in this channel beside it"
  (quietly (fn []
             (if (buffer-shown? nicks-buffer)
               (close-buffer nicks-buffer)
               (let [entry (here-or-complain)]
                 (when (= entry.kind :server)
                   (raise (error "Nick lists are for channels")))
                 (open-buffer nicks-buffer (nick-lines entry) :beside true :keys {"q" 'quit-window}))))))

(def (target-message entry)
  (let [line (current-line)]
    (if (nil? (input-point))
      (client/message-at entry.buffer line)
      (find (fn [message] (and (not message.self) (or (= message.kind :privmsg) (= message.kind :action))))
            (reverse entry.messages)))))

(defcommand irc-reply "Reply to the message at point, or the last one; again to stop replying"
  (quietly (fn []
             (let [entry (here-or-complain)]
               (if entry.replying
                 (client/reply-to entry.buffer nil)
                 (if-let [chosen (target-message entry)]
                   (client/reply-to entry.buffer chosen)
                   (message "No message to reply to here")))))))

(defcommand irc-react "React to the message at point, or the last one, with an emoji"
  (quietly (fn []
             (let [entry (here-or-complain)
                   chosen (or (target-message entry) (raise (error "No message to react to here")))]
               (when (nil? chosen.id)
                 (raise (error "The server didn't give this message an id, so it can't take reactions")))
               (unless (client/has-cap? entry.network "message-tags")
                 (raise (error (str entry.network " doesn't pass on reactions"))))
               (let [emoji (await (prompt "React with: " :choices reactions))]
                 (client/send-command! entry.network "TAGMSG" [entry.target] :tags {"+draft/react" emoji "+draft/reply" chosen.id})
                 (unless (client/has-cap? entry.network "echo-message")
                   (client/react! entry.buffer chosen.id (client/my-nick entry.network) emoji)))))))

(defcommand irc-toggle-noise "Show every join, part and quit, or only those of people who spoke lately"
  (message (if (client/toggle-noise!) "Showing every join, part and quit" "Showing joins, parts and quits of people who spoke lately")))

(defcommand irc-join "Join a channel on this network"
  (quietly (fn []
             (let [entry (here-or-complain)
                   invited (get (client/network entry.network) :invited)
                   channel (string/trim (await (prompt "Join: " :initial (or invited "#"))))]
               (client/join! entry.network channel)))))

(defcommand irc-part "Leave this channel"
  (quietly (fn []
             (let [entry (here-or-complain)]
               (if (= entry.kind :channel)
                 (client/part! entry.network entry.target nil)
                 (message "This isn't a channel"))))))

(def (listed-or-complain)
  (or (client/listing (buffer-name)) (raise (error "This isn't a channel list"))))

(defcommand irc-list "List the channels on this network, or again in a channel list"
  (quietly (fn []
             (let [listed (client/listing (buffer-name))]
               (client/list! (if listed listed.network (get (here-or-complain) :network)) "")))))

(defcommand irc-list-join "Join the channel on this line of the channel list"
  (quietly (fn []
             (let [listed (listed-or-complain)]
               (if-let [channel (get listed.shown (- (current-line) 3))]
                 (client/join! listed.network channel.name)
                 (message "No channel on this line"))))))

(defcommand irc-list-filter "Show only the channels whose name or topic has some text; empty shows them all"
  (quietly (fn []
             (let [buffer (buffer-name)
                   listed (listed-or-complain)
                   wanted (string/trim (await (prompt "Channels with: " :initial (or listed.filter ""))))]
               (client/filter-list! buffer wanted)))))

(defcommand irc-switch "Go to an IRC buffer"
  (quietly (fn []
             (let [all (client/irc-buffers)]
               (when (empty? all)
                 (raise (error "There are no IRC buffers yet; M-x irc connects")))
               (switch-to-buffer (await (prompt "IRC buffer: " :choices (sort all))))))))

(defonce came-from (atom nil))

(defcommand irc-next-active "Go to the next IRC buffer with new messages"
  (if-let [buffer (client/next-active)]
    (do (unless (client/buffer-entry (buffer-name))
          (reset! came-from (buffer-name)))
      (switch-to-buffer buffer)
      (client/note-seen! buffer))
    (if-let [back @came-from]
      (do (reset! came-from nil)
        (when (buffer-exists? back)
          (switch-to-buffer back)))
      (message "No new IRC messages"))))

(def (title-of html)
  (when-let [match (regex/find "(?is)<title[^>]*>(.*?)</title>" html)]
    (string/trim (string/replace (first match.groups) "\n" " "))))

(defcommand irc-preview "Fetch the page or image a link at point leads to and say what it is"
  (quietly (fn []
             (let [url (or (format/url-at (or (line-text (current-line)) "") (current-column))
                           (raise (error "Put point on a link to preview it")))]
               (message (str "Fetching " url "…"))
               (let [response (await (http/get url))
                     kind (or (get response.headers "content-type") "")]
                 (cond ((>= response.status 400) (message (str url " answered " response.status)))
                   ((string/starts-with? kind "image/")
                    (message (str "An image, " kind ", " (// (count response.body) 1024) " KB: RET opens it")))
                   (else (message (or (title-of response.body) (str "A page of " kind))))))))))

(def (search-lines query results)
  (vec (concat [{line: [["IRC logs" :heading] [(str "  " (count results) " lines with " (pr-str query)) :comment]]}
                {line: [["RET visits the log  q quits" :comment]]}
                {line: []}]
               (map (fn [result]
                      {line: [[(str result.network " " result.target " " result.day) :comment] " " result.text]})
                    results))))

(defcommand irc-search "Search the IRC logs"
  (quietly (fn []
             (let [query (await (prompt "Search IRC logs for: "))
                   results (logs/search query)]
               (reset! found results)
               (open-buffer search-buffer (search-lines query results) :keys {"RET" 'irc-search-visit "q" 'quit-window} :line 3)))))

(defcommand irc-search-visit "Visit the log line on this line"
  (if-let [result (get @found (- (current-line) 3))]
    (do (open-file result.path)
      (goto-line result.line))
    (message "No log line here")))
