(module
  (import (ox string) (ox time) (ox net) (ox crypto) (ox regex)
          (irc protocol :as protocol)
          (irc format :as format)
          (irc logs :as logs)
          (vault store :as vault))
  (export state network buffer-entry server-buffer buffer-of ensure-buffer! render! connect! quit! send! send-command!
          say! say-lines! act! join! part! has-cap? my-nick channel-nicks info! error! note-seen! toggle-noise!
          message-at reply-to react! clear-buffer! irc-buffers next-active connected?))

(def wanted-caps ["server-time" "message-tags" "echo-message" "batch" "draft/chathistory" "chathistory" "sasl"
                  "away-notify" "account-notify" "multi-prefix" "extended-join" "draft/multiline"
                  "draft/read-marker" "invite-notify" "chghost" "setname"])
(def talk (set [:privmsg :action :notice]))
(def noisy (set [:join :part :quit :nick]))
(def max-messages 2000)
(def typing-for (time/seconds 6))

(defonce state (atom {networks: {} buffers: {} activity: [] showing: (set []) noise: false watching: false}))

(def chat-keys
  {"RET" 'irc-return "TAB" 'irc-complete "C-c C-n" 'irc-nicks "C-c C-r" 'irc-reply "C-c C-e" 'irc-react
   "C-c C-h" 'irc-toggle-noise "C-c C-j" 'irc-join "C-c C-p" 'irc-part "C-c C-v" 'irc-preview
   "C-c C-b" 'irc-switch "C-c C-SPC" 'irc-next-active})

(def (each f coll)
  (reduce (fn [_ item] (f item) nil) nil coll))

(def (fold text)
  (protocol/casefold (or text "")))

(def (network name)
  (get-in @state [:networks name]))

(def (buffer-entry buffer)
  (get-in @state [:buffers buffer]))

(def (my-nick name)
  (get (network name) :nick))

(def (has-cap? name cap)
  (contains? (or (get (network name) :caps) (set [])) cap))

(def (connected? name)
  (some? (get (network name) :conn)))

(def (nick-width)
  (or (plugin-setting 'irc :nick-width) 12))

(def (server-buffer name)
  (str "*irc " name "*"))

(def (irc-buffers)
  "The names of every IRC buffer."
  (vec (keys (get @state :buffers))))

(def (buffer-of name target)
  "The buffer already showing target on the network called name, or nil."
  (some (fn [[buffer entry]]
          (when (and (= entry.network name) (not= entry.kind :server) (= (fold entry.target) (fold target)))
            buffer))
        (entries (get @state :buffers))))

(def (width-of buffer)
  (or (buffer-width buffer) 100))

(def (prompt-of entry)
  (str (if entry.replying (str "↪ " entry.replying.nick " ") "") entry.target "> "))

(def (visible? entry message)
  (or (get @state :noise)
      (not (contains? noisy message.kind))
      (let [spoke (get entry.spoke (fold message.nick))]
        (and spoke (< (- message.time spoke) (time/minutes 30))))))

(def (rendered entry)
  (let [width (width-of entry.buffer)
        wide (nick-width)
        step (fn [[lines rows context] [index message]]
               (if (visible? entry message)
                 (let [made (format/message-lines message context width wide)
                       marked (= index entry.marker)
                       lines (if marked (conj lines (format/marker-line)) lines)
                       rows (if marked (conj rows nil) rows)]
                   [(into lines made.lines) (into rows (repeat (count made.lines) index)) made.context])
                 [lines rows context]))]
    (reduce step [[] [] (format/fresh-context)] (zip (range (count entry.messages)) entry.messages))))

(def (render! buffer)
  "Draws the buffer again from its messages."
  (when-let [entry (buffer-entry buffer)]
    (let [[lines rows context] (rendered entry)]
      (swap! state update-in [:buffers buffer] merge {context: context rows: rows})
      (when (buffer-exists? buffer)
        (set-lines buffer lines)))))

(def (ensure-buffer! name target kind {show false})
  "The buffer for target on the network called name, made when it is missing, and shown when show is true."
  (let [buffer (or (if (= kind :server) (when (buffer-entry (server-buffer name)) (server-buffer name)) (buffer-of name target))
                   (cond ((= kind :server) (server-buffer name))
                     ((buffer-entry target) (str target "@" name))
                     (else target)))]
    (unless (buffer-entry buffer)
      (swap! state assoc-in [:buffers buffer]
             {buffer: buffer network: name target: target kind: kind messages: [] rows: []
              context: (format/fresh-context) marker: nil shown: false spoke: {} typing: {} replying: nil}))
    (if (buffer-exists? buffer)
      (when show (switch-to-buffer buffer))
      (let [[lines rows context] (rendered (buffer-entry buffer))]
        (swap! state update-in [:buffers buffer] merge {context: context rows: rows})
        (open-buffer buffer lines :input (prompt-of (buffer-entry buffer)) :keys chat-keys :show show
                     :on-resize (fn [_] (render! buffer)))))
    buffer))

(def (keep-last messages)
  (if (> (count messages) max-messages)
    (vec (drop (- (count messages) max-messages) messages))
    messages))

(def (show-activity!)
  (let [items (get @state :activity)]
    (set-mode-line "irc" (when (not (empty? items))
                           (str "[" (string/join "," (map (fn [item] (str item.buffer (if item.mention "!" ""))) items)) "]")))))

(def (note-activity! buffer mention)
  (swap! state update :activity
         (fn [items]
           (let [old (find (fn [item] (= item.buffer buffer)) items)]
             (conj (vec (remove (fn [item] (= item.buffer buffer)) items))
                   {buffer: buffer mention: (or mention (and old old.mention))}))))
  (show-activity!))

(def (note-seen! buffer)
  "Forgets the activity in buffer, now that it has been seen."
  (when (some (fn [item] (= item.buffer buffer)) (get @state :activity))
    (swap! state update :activity (fn [items] (vec (remove (fn [item] (= item.buffer buffer)) items))))
    (show-activity!)))

(def (next-active)
  "The buffer to visit for new messages: the oldest with a mention, else the oldest with activity."
  (let [items (get @state :activity)]
    (when-let [item (or (find :mention items) (first items))]
      item.buffer)))

(def (add! buffer message)
  (when-let [entry (buffer-entry buffer)]
    (let [index (count entry.messages)
          hidden (not (buffer-shown? buffer))
          chat (contains? talk message.kind)
          mark (and hidden chat (not message.self) (nil? entry.marker))
          spoke (if chat (assoc entry.spoke (fold message.nick) message.time) entry.spoke)
          updated (merge entry {spoke: spoke marker: (if mark index entry.marker)})
          shown (visible? updated message)]
      (swap! state update-in [:buffers buffer]
             (fn [now]
               (let [messages (conj now.messages message)
                     cut (- (count messages) (count (keep-last messages)))]
                 (merge now {messages: (keep-last messages)
                             spoke: spoke
                             marker: (when-let [at (if mark index now.marker)] (when (>= (- at cut) 0) (- at cut)))}))))
      (when shown
        (let [made (format/message-lines message entry.context (width-of buffer) (nick-width))
              lines (if mark (vec (cons (format/marker-line) made.lines)) made.lines)
              rows (concat (if mark [nil] []) (repeat (count made.lines) index))]
          (swap! state update-in [:buffers buffer] merge {context: made.context
                                                          rows: (vec (concat (get (buffer-entry buffer) :rows) rows))})
          (when (buffer-exists? buffer)
            (append-lines buffer lines))))
      (when (and hidden chat (not message.self))
        (note-activity! buffer message.mention)))))

(def (here-on name)
  (let [entry (buffer-entry (buffer-name))]
    (if (and entry (= entry.network name)) entry.buffer (server-buffer name))))

(def (info! buffer text)
  (add! buffer {kind: :info time: (time/now) text: text}))

(def (error! buffer text)
  (add! buffer {kind: :error time: (time/now) text: text}))

(def (stamp line)
  (if-let [given (get line.tags "time")]
    (try (time/instant given) (catch error _ (time/now)))
    (time/now)))

(def (event! buffer line kind text)
  (add! buffer {kind: kind time: (stamp line) nick: line.nick text: text}))

(def (log! name target message)
  (when (plugin-setting 'irc :logs)
    (spawn (logs/append! name target message))))

(def (send! name line)
  (when-let [conn (get (network name) :conn)]
    (net/send conn line)))

(def (send-command! name command params {tags nil})
  (send! name (protocol/format-line command params :tags tags)))

(def (change-network! name f & args)
  (swap! state (fn [all] (apply update-in all [:networks name] f args))))

(def (channel-entry name channel)
  (get-in (network name) [:channels (fold channel)]))

(def (channel-nicks name channel)
  "The people in channel on the network called name, as maps of :nick, :prefix and :away."
  (vec (vals (or (get (channel-entry name channel) :names) {}))))

(def (prefixes name)
  (or (get-in (network name) [:isupport :prefixes]) "~&@%+"))

(def (split-prefix name given)
  (let [marks (prefixes name)
        chars (protocol/letters given)
        taken (vec (take-while (fn [c] (string/includes? marks c)) chars))]
    [(apply str taken) (string/slice given (count taken))]))

(def (add-name! name channel nick prefix)
  (when (channel-entry name channel)
    (swap! state update-in [:networks name :channels (fold channel) :names]
           assoc (fold nick) {nick: nick prefix: prefix away: false})))

(def (remove-name! name channel nick)
  (when (channel-entry name channel)
    (swap! state update-in [:networks name :channels (fold channel) :names] dissoc (fold nick))))

(def (channels-with name nick)
  (vec (keep (fn [[_ channel]] (when (get channel.names (fold nick)) channel.name))
             (entries (or (get (network name) :channels) {})))))

(def (mentions? name text)
  (let [words (map (fn [match] (fold match.text)) (regex/find-all "[^\\s,.:;!?()<>\"']+" text))
        wanted (set (map fold (cons (my-nick name) (or (plugin-setting 'irc :highlights) []))))]
    (some (fn [word] (contains? wanted word)) words)))

(def (alert! name buffer message private)
  (when (and (not message.self)
             (or message.mention private)
             (plugin-setting 'irc :notify)
             (or (not (window-focused?)) (not (buffer-shown? buffer))))
    (notify (if private (str message.nick " on " name) (str message.nick " in " buffer))
            (protocol/plain-text (or message.text "")))))

(def (message-by-id buffer id)
  (when id
    (find (fn [message] (= message.id id)) (get (buffer-entry buffer) :messages))))

(def (message-at buffer line)
  "The message drawn on line of buffer, or nil."
  (when-let [index (get (get (buffer-entry buffer) :rows) line)]
    (get (get (buffer-entry buffer) :messages) index)))

(def (cap-end! name)
  (unless (get (network name) :capped)
    (change-network! name assoc :capped true)
    (send! name "CAP END")))

(def (on-cap! name line)
  (let [[_ sub & rest] line.params
        sub (string/upper (or sub ""))]
    (cond ((= sub "LS")
           (let [more (and (> (count rest) 1) (= (first rest) "*"))
                 offered (map (fn [cap] (first (string/split cap "=" 2))) (string/words (or (last rest) "")))]
             (change-network! name update :offered into offered)
             (unless more
               (let [known (get (network name) :offered)
                     wanted (filter (fn [cap] (and (contains? known cap) (or (not= cap "sasl") (get (network name) :sasl))))
                                    wanted-caps)]
                 (if (empty? wanted)
                   (cap-end! name)
                   (send-command! name "CAP" ["REQ" (string/join " " wanted)]))))))
      ((= sub "ACK")
       (let [acked (remove (fn [cap] (string/starts-with? cap "-")) (string/words (or (last rest) "")))]
         (change-network! name update :caps into acked)
         (if (and (some (fn [cap] (= cap "sasl")) acked) (get (network name) :sasl))
           (send-command! name "AUTHENTICATE" ["PLAIN"])
           (cap-end! name))))
      ((= sub "NAK") (cap-end! name))
      (else nil))))

(def (on-authenticate! name line)
  (when (= (first line.params) "+")
    (let [login (get (network name) :sasl)]
      (send-command! name "AUTHENTICATE" [(protocol/sasl-plain login.user login.password)]))))

(def (on-welcome! name line)
  (let [nick (first line.params)
        config (get (network name) :config)]
    (change-network! name merge {nick: nick registered: true attempts: 0})
    (info! (server-buffer name) (or (last line.params) "Connected"))
    (unless (empty? (or config.channels []))
      (send-command! name "JOIN" [(string/join "," config.channels)]))))

(def (on-isupport! name line)
  (let [tokens (drop 1 (take (dec (count line.params)) line.params))
        found (reduce (fn [found token]
                        (let [[key value] (string/split token "=" 2)]
                          (assoc found key (or value true))))
                      {} tokens)
        prefix (get found "PREFIX")]
    (change-network! name update :isupport merge
                     (merge found (if (string? prefix)
                                    {prefixes: (second (string/split prefix ")" 2))}
                                    {})))))

(def (on-nick-taken! name line)
  (if (get (network name) :registered)
    (on-numeric! name line)
    (let [nick (str (my-nick name) "_")]
      (change-network! name assoc :nick nick)
      (send-command! name "NICK" [nick]))))

(def (history? name)
  (or (has-cap? name "draft/chathistory") (has-cap? name "chathistory")))

(def (on-join! name line)
  (let [channel (first line.params)
        mine (protocol/same-nick? line.nick (my-nick name))
        show (and mine (contains? (get @state :showing) (fold channel)))
        buffer (ensure-buffer! name channel :channel :show show)]
    (if mine
      (do (swap! state update :showing disj (fold channel))
        (swap! state assoc-in [:networks name :channels (fold channel)] {name: channel topic: nil names: {}})
        (when (history? name)
          (send-command! name "CHATHISTORY" ["LATEST" channel "*" "50"]))
        (event! buffer line :info (str "You joined " channel)))
      (do (add-name! name channel line.nick "")
        (event! buffer line :join (str line.nick " joined"))))))

(def (on-part! name line)
  (let [[channel reason] line.params]
    (when-let [buffer (buffer-of name channel)]
      (if (protocol/same-nick? line.nick (my-nick name))
        (do (swap! state update-in [:networks name :channels] dissoc (fold channel))
          (event! buffer line :info (str "You left " channel)))
        (do (remove-name! name channel line.nick)
          (event! buffer line :part (str line.nick " left" (if reason (str " (" reason ")") ""))))))))

(def (on-kick! name line)
  (let [[channel victim reason] line.params]
    (when-let [buffer (buffer-of name channel)]
      (remove-name! name channel victim)
      (event! buffer line :kick (str line.nick " kicked " victim (if reason (str " (" reason ")") "")))
      (when (protocol/same-nick? victim (my-nick name))
        (swap! state update-in [:networks name :channels] dissoc (fold channel))))))

(def (on-quit! name line)
  (let [reason (first line.params)]
    (each (fn [channel]
            (remove-name! name channel line.nick)
            (when-let [buffer (buffer-of name channel)]
              (event! buffer line :quit (str line.nick " quit" (if reason (str " (" reason ")") "")))))
          (channels-with name line.nick))))

(def (on-nick! name line)
  (let [fresh (first line.params)
        old line.nick]
    (when (protocol/same-nick? old (my-nick name))
      (change-network! name assoc :nick fresh)
      (info! (server-buffer name) (str "You are now " fresh)))
    (each (fn [channel]
            (let [known (get-in (channel-entry name channel) [:names (fold old)])]
              (remove-name! name channel old)
              (add-name! name channel fresh (or (get known :prefix) ""))
              (when-let [buffer (buffer-of name channel)]
                (event! buffer line :nick (str old " is now " fresh)))))
          (channels-with name old))))

(def (reply-ctcp! name nick [command argument])
  (cond ((= command "VERSION") (send-command! name "NOTICE" [nick "\u{1}VERSION oxmacs\u{1}"]))
    ((= command "PING") (send-command! name "NOTICE" [nick (str "\u{1}PING " argument "\u{1}")]))
    ((= command "TIME") (send-command! name "NOTICE" [nick (str "\u{1}TIME " (time/format (time/now) "%c") "\u{1}")]))
    (else nil)))

(def (message-of name line kind)
  (let [[_ text] line.params
        action (protocol/action (or text ""))
        mine (protocol/same-nick? line.nick (my-nick name))]
    {kind: (if action :action kind)
     time: (stamp line)
     nick: line.nick
     text: (or action text "")
     id: (get line.tags "msgid")
     self: mine
     mention: (and (not mine) (mentions? name (or text "")))
     reply-id: (get line.tags "+draft/reply")}))

(def (on-message! name line kind)
  (let [[target text] line.params
        text (or text "")
        me (my-nick name)
        mine (protocol/same-nick? line.nick me)
        private (not (protocol/channel? target))
        partner (cond ((not private) target)
                  (mine target)
                  (else line.nick))
        ctcp (protocol/ctcp text)]
    (cond ((and ctcp (not (protocol/action text)))
           (when (and (= kind :privmsg) (not mine))
             (reply-ctcp! name line.nick ctcp)))
      ((or (nil? line.user) (and (= kind :notice) private (not (buffer-of name partner))))
       (add! (server-buffer name) (message-of name line kind)))
      (else
       (let [buffer (ensure-buffer! name partner (if private :query :channel))
             message (message-of name line kind)
             quoted (message-by-id buffer message.reply-id)
             message (if quoted (assoc message :reply {nick: quoted.nick text: quoted.text}) message)]
         (swap! state update-in [:buffers buffer :typing] dissoc (fold line.nick))
         (add! buffer message)
         (alert! name buffer message (and private (not mine)))
         (log! name partner message))))))

(def (react! buffer id nick emoji)
  "Adds nick's emoji reaction to the message with id in buffer and draws it again."
  (when (message-by-id buffer id)
    (swap! state update-in [:buffers buffer :messages]
           (fn [messages]
             (vec (map (fn [message]
                         (if (= message.id id)
                           (update message :reactions
                                   (fn [found] (update (or found {}) emoji (fn [nicks] (vec (distinct (conj (or nicks []) nick)))))))
                           message))
                       messages))))
    (render! buffer)))

(def (on-tagmsg! name line)
  (let [target (first line.params)
        private (not (protocol/channel? target))
        partner (if private line.nick target)
        buffer (if private (buffer-of name partner) (buffer-of name target))
        typing (get line.tags "+typing")
        reaction (get line.tags "+draft/react")]
    (when buffer
      (when typing
        (swap! state update-in [:buffers buffer :typing]
               (fn [found] (if (= typing "active")
                             (assoc (or found {}) (fold line.nick) {nick: line.nick time: (time/now)})
                             (dissoc (or found {}) (fold line.nick))))))
      (when reaction
        (react! buffer (get line.tags "+draft/reply") line.nick reaction)))))

(def (on-topic! name line)
  (let [[channel topic] line.params]
    (when-let [buffer (buffer-of name channel)]
      (swap! state assoc-in [:networks name :channels (fold channel) :topic] topic)
      (event! buffer line :topic (str line.nick " set the topic: " topic)))))

(def (on-topic-reply! name line)
  (let [[_ channel topic] line.params]
    (when-let [buffer (buffer-of name channel)]
      (swap! state assoc-in [:networks name :channels (fold channel) :topic] topic)
      (event! buffer line :topic (str "Topic: " topic)))))

(def (on-names! name line)
  (let [channel (get line.params 2)
        names (string/words (or (last line.params) ""))]
    (each (fn [given]
            (let [[prefix nick] (split-prefix name given)
                  nick (first (string/split nick "!" 2))]
              (add-name! name channel nick prefix)))
          names)))

(def (on-away! name line)
  (let [away (not (empty? line.params))]
    (each (fn [channel]
            (swap! state update-in [:networks name :channels (fold channel) :names (fold line.nick)] assoc :away away))
          (channels-with name line.nick))))

(def (on-mode! name line)
  (let [[target & changes] line.params]
    (when-let [buffer (when (protocol/channel? target) (buffer-of name target))]
      (event! buffer line :mode (str (or line.nick "The server") " sets " (string/join " " changes)))
      (send-command! name "NAMES" [target]))))

(def (history-message name line)
  (when (or (= line.command "PRIVMSG") (= line.command "NOTICE"))
    (merge (message-of name line (if (= line.command "PRIVMSG") :privmsg :notice)) {mention: false})))

(def (merge-history! name target lines)
  (when-let [buffer (buffer-of name target)]
    (let [known (set (keep :id (get (buffer-entry buffer) :messages)))
          older (vec (filter (fn [message] (or (nil? message.id) (not (contains? known message.id))))
                             (keep (fn [line] (history-message name line)) lines)))]
      (swap! state update-in [:buffers buffer]
             (fn [entry]
               (merge entry {messages: (keep-last (vec (sort-by :time (concat older entry.messages))))
                             marker: nil
                             spoke: (reduce (fn [spoke message] (assoc spoke (fold message.nick) message.time))
                                            entry.spoke older)})))
      (render! buffer))))

(def (on-batch! name line)
  (let [[reference kind & arguments] line.params
        opening (string/starts-with? reference "+")
        id (string/slice reference 1)]
    (if opening
      (change-network! name assoc-in [:batches id] {kind: kind arguments: arguments tags: line.tags lines: []})
      (when-let [batch (get-in (network name) [:batches id])]
        (change-network! name update :batches dissoc id)
        (cond ((or (= batch.kind "chathistory") (= batch.kind "draft/chathistory"))
               (merge-history! name (first batch.arguments) batch.lines))
          ((= batch.kind "draft/multiline")
           (when-let [first-line (first batch.lines)]
             (let [text (reduce (fn [text part]
                                  (str text (if (contains? part.tags "draft/multiline-concat") "" "\n") (or (last part.params) "")))
                                (or (last first-line.params) "") (rest batch.lines))]
               (on-message! name (merge first-line {params: [(first batch.arguments) text]
                                                    tags: (merge first-line.tags batch.tags)})
                            :privmsg))))
          (else (each (fn [inner] (dispatch! name inner)) batch.lines)))))))

(def (on-numeric! name line)
  (let [code (string/to-number line.command)
        params line.params
        about (when (> (count params) 2) (get params 1))
        target (when about (buffer-of name about))
        text (string/join " " (drop 1 params))]
    (cond ((and code (>= code 400) (< code 600))
           (cond (target (error! target (last params)))
             (about (error! (here-on name) (str (string/join " " (drop 1 (take (dec (count params)) params))) ": " (last params))))
             (else (error! (here-on name) (last params)))))
      ((contains? (set ["002" "003" "004" "251" "252" "253" "254" "255" "265" "266" "375" "372" "376"]) line.command)
       (info! (server-buffer name) (last line.params)))
      (else (info! (server-buffer name) text)))))

(def (on-standard! name line)
  (let [kind line.command
        about (first line.params)
        text (or (last line.params) "")
        said (cond ((or (nil? about) (= about "*")) text)
               ((= kind "FAIL") (str about " failed: " text))
               (else (str about ": " text)))]
    (if (= kind "FAIL")
      (error! (here-on name) said)
      (info! (here-on name) said))))

(def (on-invite! name line)
  (let [[invited channel] line.params]
    (if (protocol/same-nick? invited (my-nick name))
      (do (change-network! name assoc :invited channel)
        (info! (here-on name) (str line.nick " invites you to " channel "; C-c C-j joins"))
        (when (and (plugin-setting 'irc :notify) (not (window-focused?)))
          (notify (str line.nick " on " name) (str "Invites you to " channel))))
      (when-let [buffer (buffer-of name channel)]
        (event! buffer line :info (str line.nick " invited " invited))))))

(def (dispatch! name line)
  (let [command line.command]
    (cond ((= command "PING") (send-command! name "PONG" line.params))
      ((= command "CAP") (on-cap! name line))
      ((= command "AUTHENTICATE") (on-authenticate! name line))
      ((= command "001") (on-welcome! name line))
      ((= command "005") (on-isupport! name line))
      ((= command "433") (on-nick-taken! name line))
      ((= command "903") (do (info! (server-buffer name) "Logged in") (cap-end! name)))
      ((contains? (set ["902" "904" "905" "906" "907"]) command)
       (do (error! (server-buffer name) (str "SASL login failed: " (last line.params))) (cap-end! name)))
      ((= command "JOIN") (on-join! name line))
      ((= command "PART") (on-part! name line))
      ((= command "KICK") (on-kick! name line))
      ((= command "QUIT") (on-quit! name line))
      ((= command "NICK") (on-nick! name line))
      ((= command "PRIVMSG") (on-message! name line :privmsg))
      ((= command "NOTICE") (on-message! name line :notice))
      ((= command "TAGMSG") (on-tagmsg! name line))
      ((= command "INVITE") (on-invite! name line))
      ((contains? (set ["FAIL" "WARN" "NOTE"]) command) (on-standard! name line))
      ((= command "TOPIC") (on-topic! name line))
      ((= command "332") (on-topic-reply! name line))
      ((= command "353") (on-names! name line))
      ((contains? (set ["333" "366" "MARKREAD" "PONG"]) command) nil)
      ((= command "AWAY") (on-away! name line))
      ((= command "BATCH") (on-batch! name line))
      ((= command "MODE") (on-mode! name line))
      ((= command "ERROR") (error! (server-buffer name) (or (last line.params) "The server closed the link")))
      ((string/to-number command) (on-numeric! name line))
      (else nil))))

(def (handle! name line)
  (let [batch (get line.tags "batch")]
    (if (and batch (get-in (network name) [:batches batch]))
      (change-network! name update-in [:batches batch :lines] conj line)
      (dispatch! name line))))

(def (typing-text)
  (when-let [entry (buffer-entry (buffer-name))]
    (let [now (time/now)
          active (filter (fn [found] (< (- now found.time) typing-for)) (vals (or entry.typing {})))]
      (when (not (empty? active))
        (str (string/join ", " (map :nick active)) (if (= (count active) 1) " is" " are") " typing…")))))

(def (mark-read! entry)
  (when-let [last-one (last entry.messages)]
    (when (and (has-cap? entry.network "draft/read-marker") (not= entry.kind :server))
      (send-command! entry.network "MARKREAD"
                     [entry.target (str "timestamp=" (time/format last-one.time "%Y-%m-%dT%H:%M:%S%.3fZ" :zone "UTC"))]))))

(def (tick!)
  (each (fn [[buffer entry]]
          (let [shown (buffer-shown? buffer)]
            (cond ((and shown (not entry.shown))
                   (swap! state assoc-in [:buffers buffer :shown] true)
                   (note-seen! buffer)
                   (mark-read! entry))
              ((and (not shown) entry.shown)
               (swap! state update-in [:buffers buffer] merge {shown: false})
               (when entry.marker
                 (swap! state assoc-in [:buffers buffer :marker] nil)
                 (render! buffer)))
              (else nil))))
        (entries (get @state :buffers)))
  (set-mode-line "irc-typing" (typing-text)))

(def (watch!)
  (unless (get @state :watching)
    (swap! state assoc :watching true)
    (every 1000 (fn []
                  (if (some connected? (keys (get @state :networks)))
                    (do (tick!) nil)
                    (do (swap! state assoc :watching false) :stop))))))

(def (credentials config)
  (when config.login
    (vault/unlock)
    (if-let [entry (vault/find-entry config.login)]
      {user: (if (string/blank? (or entry.username "")) config.nick entry.username) password: entry.password}
      (raise (error (str "The vault has no entry called " config.login))))))

(def (read-loop! name conn)
  (if-let [line (receive conn.lines)]
    (do (try (handle! name (protocol/parse-line line))
          (catch error e
            (error! (server-buffer name) (str "Couldn't handle " line ": " e.message))))
      (read-loop! name conn))
    (closed! name conn)))

(def (connect! config {show false})
  "Connects to the network config describes, logging in with SASL from the vault when it names a :login."
  (let [name config.name
        sasl (credentials config)
        buffer (ensure-buffer! name name :server :show show)]
    (change-network! name (fn [old]
                            {config: config conn: nil nick: config.nick registered: false capped: false
                             caps: (set []) offered: (set []) isupport: {} batches: {} channels: {}
                             sasl: sasl quitting: false attempts: (or (get old :attempts) 0)}))
    (info! buffer (str "Connecting to " config.host " port " (or config.port 6697) "…"))
    (let [conn (try (await (net/connect config.host (or config.port 6697) :tls (not= config.tls false)))
                 (catch error e
                   (error! buffer (str "Couldn't connect: " e.message))
                   nil))]
      (when conn
        (change-network! name assoc :conn conn)
        (send! name "CAP LS 302")
        (send-command! name "NICK" [config.nick])
        (send-command! name "USER" [(or config.user config.nick) "0" "*" (or config.realname config.nick)])
        (spawn (read-loop! name conn))
        (watch!))
      conn)))

(def (closed! name conn)
  (when (= (get (network name) :conn) conn)
    (let [old (network name)
          attempts (inc (or old.attempts 0))
          wait (min 60 (* 5 attempts))]
      (change-network! name merge {conn: nil registered: false attempts: attempts})
      (if old.quitting
        (info! (server-buffer name) "Disconnected")
        (do (error! (server-buffer name) (str "The connection ended; trying again in " wait " seconds"))
          (sleep (time/seconds wait))
          (unless (or (connected? name) (get (network name) :quitting))
            (connect! old.config)))))))

(def (quit! name reason)
  (change-network! name assoc :quitting true)
  (send-command! name "QUIT" [(or reason "oxmacs")])
  (when-let [conn (get (network name) :conn)]
    (net/close conn)))

(def (join! name channel {show true})
  (when show
    (swap! state update :showing conj (fold channel)))
  (if-let [buffer (when (channel-entry name channel) (buffer-of name channel))]
    (switch-to-buffer buffer)
    (send-command! name "JOIN" [channel])))

(def (part! name channel reason)
  (send-command! name "PART" (if reason [channel reason] [channel])))

(def (reply-to buffer message)
  "Makes the next message sent from buffer a reply to message, or stops replying with nil."
  (swap! state assoc-in [:buffers buffer :replying] (when message {id: message.id nick: message.nick text: message.text}))
  (open-buffer buffer [] :input (prompt-of (buffer-entry buffer)) :show false)
  (render! buffer))

(def (local! buffer kind text tags)
  (let [entry (buffer-entry buffer)
        name entry.network]
    (unless (has-cap? name "echo-message")
      (let [replying entry.replying
            message {kind: kind time: (time/now) nick: (my-nick name) text: text self: true
                     reply: (when (and replying (get tags "+draft/reply")) {nick: replying.nick text: replying.text})}]
        (add! buffer message)
        (log! name entry.target message)))))

(def (reply-tags entry)
  (if (and entry.replying entry.replying.id (has-cap? entry.network "message-tags"))
    {"+draft/reply" entry.replying.id}
    {}))

(def (finish-reply! buffer)
  (when (get (buffer-entry buffer) :replying)
    (reply-to buffer nil)))

(def (say! buffer text)
  "Sends text from buffer to its channel or person."
  (let [entry (buffer-entry buffer)
        tags (reply-tags entry)]
    (each (fn [piece] (send-command! entry.network "PRIVMSG" [entry.target piece] :tags tags))
          (protocol/split-message text 400))
    (local! buffer :privmsg text tags)
    (finish-reply! buffer)))

(def (say-lines! buffer lines)
  "Sends several lines from buffer: one multiline message where the server allows it, else a line every half second."
  (let [entry (buffer-entry buffer)
        name entry.network
        lines (vec (remove string/blank? lines))]
    (if (has-cap? name "draft/multiline")
      (let [reference (crypto/random 4)]
        (send-command! name "BATCH" [(str "+" reference) "draft/multiline" entry.target] :tags (reply-tags entry))
        (each (fn [line] (send-command! name "PRIVMSG" [entry.target line] :tags {"batch" reference})) lines)
        (send-command! name "BATCH" [(str "-" reference)])
        (local! buffer :privmsg (string/join "\n" lines) (reply-tags entry))
        (finish-reply! buffer))
      (each (fn [line] (say! buffer line) (sleep 500)) lines))))

(def (act! buffer text)
  (let [entry (buffer-entry buffer)]
    (send-command! entry.network "PRIVMSG" [entry.target (str "\u{1}ACTION " text "\u{1}")])
    (local! buffer :action text {})))

(def (toggle-noise!)
  (swap! state update :noise not)
  (each render! (irc-buffers))
  (get @state :noise))

(def (clear-buffer! buffer)
  (swap! state update-in [:buffers buffer] merge {messages: [] marker: nil})
  (render! buffer))
