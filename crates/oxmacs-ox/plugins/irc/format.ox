(module
  (import (ox string) (ox time) (ox regex)
          (irc protocol :as protocol))
  (export message-lines marker-line fill-segments linkify segments-of clock day-of url-at fresh-context channel-lines))

(def time-width 6)
(def url-pattern "https?://[^\\s<>\"]*[^\\s<>\".,;:!?)'\\]]")

(def (clock millis)
  (time/format millis "%H:%M"))

(def (day-of millis)
  (time/format millis "%Y-%m-%d"))

(def (fresh-context)
  "The context before a buffer's first message."
  {clock: nil nick: nil kind: nil time: 0 day: nil})

(def (piece text face)
  (cond ((= text "") nil)
    (face [text face])
    (else text)))

(def (linkify text face)
  "text as segments, with web addresses as links and the rest in face."
  (let [found (regex/find-all url-pattern text)
        [out at] (reduce (fn [[out at] match]
                           [(conj out (piece (string/slice text at match.start) face) [match.text :link]) match.end])
                         [[] 0] found)]
    (vec (filter some? (conj out (piece (string/slice text at) face))))))

(def (url-at text column)
  "The web address in text that column falls on, or nil."
  (when-let [match (find (fn [match] (and (<= match.start column) (< column match.end)))
                         (regex/find-all url-pattern text))]
    match.text))

(def (segments-of text face)
  "The segments of a message's text: its mIRC formatting as faces, and links."
  (vec (mapcat (fn [part]
                 (if (string? part)
                   (linkify part face)
                   (linkify (first part) (second part))))
               (protocol/styled text))))

(def (words-of segments)
  (vec (mapcat (fn [segment]
                 (let [[text face] (if (string? segment) [segment nil] segment)]
                   (map (fn [match] [match.text face]) (regex/find-all "\\s+|\\S+" text))))
               segments)))

(def (chop word width)
  (let [[text face] word
        size (count text)]
    (if (<= size width)
      [word]
      (vec (map (fn [at] [(string/slice text at (min size (+ at width))) face])
                (range 0 size width))))))

(def (shown [text face])
  (if face [text face] text))

(def (fill-segments segments width)
  "The segments broken at spaces into lines of at most width characters, each a vector of segments."
  (let [words (mapcat (fn [word] (chop word width)) (words-of segments))
        step (fn [[lines line used pending] word]
               (let [size (count (first word))
                     gap (if pending (count (first pending)) 0)]
                 (cond ((string/blank? (first word)) [lines line used (when (> used 0) word)])
                   ((<= (+ used gap size) width)
                    [lines (conj (if pending (conj line (shown pending)) line) (shown word)) (+ used gap size) nil])
                   ((empty? line) [lines [(shown word)] size nil])
                   (else [(conj lines line) [(shown word)] size nil]))))
        [lines line] (reduce step [[] [] 0 nil] words)]
    (conj lines line)))

(def (gutter message)
  (let [kind message.kind
        nick (or message.nick "")]
    (cond ((= kind :privmsg) [nick (protocol/nick-face nick)])
      ((= kind :notice) [(str "-" nick "-") (protocol/nick-face nick)])
      ((= kind :join) ["→" :success])
      ((or (= kind :part) (= kind :quit) (= kind :kick)) ["←" :removed])
      ((= kind :error) ["!" :error])
      (else ["*" :comment]))))

(def (body message)
  (let [kind message.kind
        text (or message.text "")]
    (cond ((= kind :privmsg) (segments-of text nil))
      ((= kind :notice) (segments-of text :comment))
      ((= kind :action) (vec (concat [[message.nick (protocol/nick-face message.nick)] " "] (segments-of text :string))))
      ((= kind :error) (segments-of text :error))
      (else (segments-of text :comment)))))

(def (grouped? message context)
  (and (= message.kind :privmsg)
       (= context.kind :privmsg)
       (protocol/same-nick? message.nick context.nick)
       (< (- message.time context.time) (time/minutes 5))))

(def (day-line millis)
  {line: [[(str "─── " (time/format millis "%A %-d %B %Y") " ───") :heading]]})

(def (reply-line message indent room)
  (let [quoted message.reply]
    {line: [indent ["↪ " :comment] [quoted.nick (protocol/nick-face quoted.nick)]
            [(str " " (string/truncate (protocol/plain-text (or quoted.text "")) (max 10 (- room (count quoted.nick) 3)))) :comment]]}))

(def (reaction-line message indent)
  {line: (vec (cons indent
                    (mapcat (fn [[emoji nicks]] [[(str emoji " " (count nicks)) :comment :pill] " "])
                            (sort-by first (entries message.reactions)))))})

(def (channel-lines network channels total wanted)
  "The lines of the channel list of network: a heading, then each of the channels with its users and topic."
  (let [width (+ 2 (reduce max 0 (map (fn [channel] (count channel.name)) channels)))
        shown (if (= (count channels) total) "" (str (count channels) " of "))
        having (if (string/blank? (or wanted "")) "" (str " with " (pr-str wanted)))]
    (vec (concat [{line: [[(str "Channels on " network) :heading] [(str "  " shown total " channels" having) :comment]]}
                  {line: [["RET joins  / filters  g lists again  q quits" :comment]]}
                  {line: []}]
                 (map (fn [channel]
                        {line: (vec (concat [[(string/pad-end channel.name width) :keyword]
                                             [(string/pad-start (str channel.users) 5) :comment]
                                             "  "]
                                            (segments-of channel.topic nil)))})
                      channels)))))

(def (marker-line)
  "The line that marks where unread messages start."
  {line: [[(str (string/repeat "─" 12) " new " (string/repeat "─" 12)) :warning]]})

(def (message-lines message context width nick-width)
  "The lines a message takes in a buffer width wide after context, and the context for the next message, as :lines and :context."
  (let [stamp-text (clock message.time)
        day (day-of message.time)
        stamp (if (= stamp-text context.clock) (string/repeat " " time-width) (str stamp-text " "))
        [label face] (gutter message)
        label (if (grouped? message context) "" label)
        padded (string/pad-start (string/truncate label nick-width) nick-width)
        indent (string/repeat " " (+ time-width nick-width 1))
        room (max 20 (- width time-width nick-width 1))
        filled (fill-segments (body message) room)
        background (when message.mention :marked)
        first-line (vec (concat [[stamp :comment] [padded face] " "] (first filled)))
        rest-lines (map (fn [line] (vec (cons indent line))) (rest filled))
        text-lines (map (fn [line] {line: line background: background}) (cons first-line rest-lines))
        lines (concat (if (= day context.day) [] [(day-line message.time)])
                      (if message.reply [(reply-line message indent room)] [])
                      text-lines
                      (if (empty? message.reactions) [] [(reaction-line message indent)]))]
    {lines: (vec lines)
     context: {clock: stamp-text nick: message.nick kind: message.kind time: message.time day: day}}))
