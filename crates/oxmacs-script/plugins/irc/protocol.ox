(module
  (import (ox string) (ox crypto))
  (export letters parse-line format-line unescape escape casefold same-nick? channel? action ctcp
          styled plain-text nick-face sasl-plain split-message))

(def tag-escapes {":" ";" "s" " " "\\" "\\" "r" "\r" "n" "\n"})
(def tag-specials {";" "\\:" " " "\\s" "\\" "\\\\" "\r" "\\r" "\n" "\\n"})

(def (letters text)
  (vec (map str (string/chars text))))

(def (split-once text separator)
  (if-let [at (string/index-of text separator)]
    [(string/slice text 0 at) (string/slice text (+ at (count separator)))]
    [text ""]))

(def (unescape value)
  "An IRCv3 tag value with its escapes undone."
  (first (reduce (fn [[out pending] c]
                   (cond (pending [(str out (or (get tag-escapes c) c)) false])
                     ((= c "\\") [out true])
                     (else [(str out c) false])))
                 ["" false] (letters (or value "")))))

(def (escape value)
  "value written as an IRCv3 tag value."
  (apply str (map (fn [c] (or (get tag-specials c) c)) (letters value))))

(def (parse-tags text)
  (reduce (fn [tags part]
            (if (string/blank? part)
              tags
              (let [[key value] (split-once part "=")]
                (assoc tags key (unescape value)))))
          {} (string/split text ";")))

(def (parse-params text)
  (let [text (string/trim-start text)]
    (cond ((= text "") [])
      ((string/starts-with? text ":") [(string/slice text 1)])
      (else (let [[param rest] (split-once text " ")]
              (vec (cons param (parse-params rest))))))))

(def (source-parts source)
  (cond ((nil? source) [nil nil nil])
    ((string/includes? source "!")
     (let [[nick rest] (split-once source "!")
           [user host] (split-once rest "@")]
       [nick user host]))
    ((string/includes? source "@")
     (let [[nick host] (split-once source "@")]
       [nick nil host]))
    (else [source nil nil])))

(def (parse-line line)
  "A line from an IRC server as a map of :tags, :source, :nick, :user, :host, :command and :params."
  (let [[tags rest] (if (string/starts-with? line "@")
                      (split-once (string/slice line 1) " ")
                      [nil line])
        rest (string/trim-start rest)
        [source rest] (if (string/starts-with? rest ":")
                        (split-once (string/slice rest 1) " ")
                        [nil rest])
        [command rest] (split-once (string/trim-start rest) " ")
        [nick user host] (source-parts source)]
    {tags: (if tags (parse-tags tags) {})
     source: source
     nick: nick
     user: user
     host: host
     command: (string/upper command)
     params: (parse-params rest)}))

(def (needs-colon? param)
  (or (= param "") (string/starts-with? param ":") (string/includes? param " ")))

(def (format-line command params {tags nil})
  "An IRC line for command with params, the last one written after a colon when it needs one, and IRCv3 :tags."
  (let [tagged (if (empty? tags)
                 ""
                 (str "@" (string/join ";" (map (fn [[key value]]
                                                  (if (or (nil? value) (= value true))
                                                    (str key)
                                                    (str key "=" (escape (str value)))))
                                                (entries tags)))
                      " "))
        middle (vec (take (dec (count params)) params))
        final (last params)]
    (str tagged command
         (apply str (map (fn [param] (str " " param)) middle))
         (cond ((empty? params) "")
           ((needs-colon? final) (str " :" final))
           (else (str " " final))))))

(def (casefold text)
  "text in the rfc1459 case mapping IRC servers compare names with."
  (string/replace (string/replace (string/replace (string/replace (string/lower text) "[" "{{") "]" "}") "\\" "|") "~" "^"))

(def (same-nick? a b)
  (and a b (= (casefold a) (casefold b))))

(def (channel? target)
  (some? (and target (some (fn [prefix] (string/starts-with? target prefix)) ["#" "&" "!" "+"]))))

(def ctcp-mark "\u{1}")

(def (ctcp text)
  "The CTCP command and argument in a message, as [command argument], or nil."
  (when (and (string/starts-with? text ctcp-mark) (> (count text) 1))
    (let [inner (string/slice text 1 (if (string/ends-with? text ctcp-mark) (dec (count text)) (count text)))
          [command argument] (split-once inner " ")]
      [(string/upper command) argument])))

(def (action text)
  "The text of a /me message, or nil when text isn't one."
  (when-let [[command argument] (ctcp text)]
    (when (= command "ACTION")
      argument)))

(def mirc-faces [nil :comment :keyword :success :error :warning :macro :warning
                 :constant :success :type :type :function :macro :comment :comment])

(def codes {"\u{2}" :bold "\u{1d}" :italic "\u{1f}" :underline "\u{1e}" :strike
            "\u{11}" :mono "\u{16}" :reverse "\u{f}" :reset})

(def (digits-at chars at)
  (let [first (get chars at)
        second (get chars (inc at))]
    (cond ((not (and first (string/to-number first))) [nil at])
      ((and second (string/to-number second)) [(string/to-number (str first second)) (+ at 2)])
      (else [(string/to-number first) (inc at)]))))

(def (color-at chars at)
  (let [[fore after] (digits-at chars at)]
    (if (and fore (= (get chars after) ",") (get chars (inc after)) (string/to-number (get chars (inc after))))
      (let [[_ end] (digits-at chars (inc after))]
        [fore end])
      [fore after])))

(def (face-of state)
  (cond ((get state :color) (get mirc-faces (mod (get state :color) 16)))
    ((get state :bold) :heading)
    ((get state :italic) :string)
    (else nil)))

(def (styled text)
  "text with its mIRC formatting codes turned into [text face] segments."
  (let [chars (letters text)
        total (count chars)
        step (fn step [at state current out]
               (if (>= at total)
                 (conj out [current (face-of state)])
                 (let [c (get chars at)
                       code (get codes c)]
                   (cond ((= c "\u{3}")
                          (let [[color after] (color-at chars (inc at))]
                            (step after (assoc state :color color) "" (conj out [current (face-of state)]))))
                     ((= code :reset) (step (inc at) {} "" (conj out [current (face-of state)])))
                     (code (step (inc at) (update state code not) "" (conj out [current (face-of state)])))
                     ((= c "\u{4}") (step (+ at 7) state current out))
                     (else (step (inc at) state (str current c) out))))))]
    (vec (keep (fn [[shown face]]
                 (cond ((= shown "") nil)
                   ((nil? face) shown)
                   (else [shown face])))
               (step 0 {} "" [])))))

(def (plain-text text)
  "text without its mIRC formatting codes."
  (apply str (map (fn [part] (if (string? part) part (first part))) (styled text))))

(def (nick-face nick)
  "The face a nick is always drawn in, one of eight picked from its name."
  (keyword (str "nick-" (mod (abs (hash (casefold (or nick "")))) 8))))

(def (sasl-plain user password)
  "The SASL PLAIN response for user and password, base64 encoded."
  (crypto/to-base64 (str user "\u{0}" user "\u{0}" password)))

(def (split-message text width)
  "text cut into pieces of at most width characters, breaking at spaces when it can, for servers' line limits."
  (if (<= (count text) width)
    [text]
    (let [cut (string/slice text 0 width)
          back (string/index-of (string/reverse cut) " ")
          space (when back (- (count cut) 1 back))
          at (if (and space (> space (// width 2))) space width)]
      (vec (cons (string/slice text 0 at)
                 (split-message (string/trim-start (string/slice text at)) width))))))

(test "lines are parsed with their tags, source and params"
  (let [l (parse-line "@time=2026-10-23T05:43:15.656Z;msgid=a\\sb\\:c :bo!b@host PRIVMSG #oxmacs :hi there")]
    (expect (= [l.nick l.user l.host l.command l.params]
               ["bo" "b" "host" "PRIVMSG" ["#oxmacs" "hi there"]]))
    (expect (= (get l.tags "msgid") "a b;c"))
    (expect (= (get l.tags "time") "2026-10-23T05:43:15.656Z")))
  (expect (= (get (parse-line "PING :tok") :params) ["tok"]))
  (expect (= (get (parse-line ":srv 001 ana :Welcome") :command) "001")))

(test "lines are written with escaped tags and a colon where needed"
  (expect (= (format-line "PRIVMSG" ["#oxmacs" "hi there"] :tags {"+draft/reply" "x;y z"})
             "@+draft/reply=x\\:y\\sz PRIVMSG #oxmacs :hi there"))
  (expect (= (format-line "NICK" ["ana"]) "NICK ana"))
  (expect (= (format-line "PRIVMSG" ["#c" ":)"]) "PRIVMSG #c ::)")))

(test "ctcp, formatting and nicks are understood"
  (expect (= (action "\u{1}ACTION waves\u{1}") "waves"))
  (expect (= (ctcp "\u{1}VERSION\u{1}") ["VERSION" ""]))
  (expect (nil? (action "hi")))
  (expect (= (styled "\u{2}bold\u{2} \u{3}4,1red\u{f} plain") [["bold" :heading] " " ["red" :error] " plain"]))
  (expect (= (plain-text "\u{3}12blue\u{3} done") "blue done"))
  (expect (same-nick? "Ana[m]" "ana{{m}"))
  (expect (channel? "#x"))
  (expect (not (channel? "bo")))
  (expect (= (sasl-plain "ana" "pw") "YW5hAGFuYQBwdw=="))
  (expect (= (split-message "aaa bbb ccc" 7) ["aaa bbb" "ccc"]))
  (expect (= (nick-face "Bo") (nick-face "bo"))))
