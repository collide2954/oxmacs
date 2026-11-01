(module
  (import (ox string) (ox time)
          (claude api)
          (claude render)
          (claude tools)
          (claude review)
          (claude chats)
          (claude suggest)
          (claude where)))

(def chat-buffer "*claude*")
(def chats-buffer "*claude chats*")
(def chat-keys {"RET" 'claude-send "C-c C-k" 'claude-stop "C-c C-n" 'claude-new "C-c C-l" 'claude-chats})
(def chats-keys {"RET" 'claude-open-chat "d" 'claude-delete-chat "g" 'claude-chats "q" 'quit-window})
(def redraw-every 80)

(def system
  [{type: "text"
    text: "You are Claude, working inside oxmacs, an Emacs-like text editor written in Rust whose plugins are written in Ox, a Scheme-like language. You talk with the person using it in a chat beside their work. Each of their messages starts with an <editor> note saying where they are.

Look at their buffers and files with your tools rather than guessing, open files and run oxmacs commands for them when they ask, and propose changes with edit_file or write_file, which they review as a diff before anything changes. Reading is free; commands and Ox evaluation ask them first, and they may say no. Prefer run_command with a command list_commands finds over evaluating Ox.

Keep answers short and plain, show code in fenced blocks, and say briefly what you changed."}])

(def (new-chat)
  {id: (str (time/now)) title: nil project: nil created: (time/now)
   messages: [] log: [] busy: false stopped: false response: nil})

(defonce state (atom (new-chat)))

(def (show!)
  (when (buffer-live? chat-buffer)
    (set-lines chat-buffer (render/lines-of (get @state :log)))))

(def (open-chat!)
  (unless (buffer-live? chat-buffer)
    (open-buffer chat-buffer (render/lines-of (get @state :log))
                 :input "> " :keys chat-keys :label "Claude chat" :show false)
    (set-word-wrap chat-buffer true))
  (set-header chat-buffer [[["Claude" :function] "  " [(setting :model) :comment]]
                           [["RET" :keyword] " send  " ["C-c C-k" :keyword] " stop  "
                            ["C-c C-n" :keyword] " new  " ["C-c C-l" :keyword] " past chats"]])
  (show-side-window chat-buffer :edge :right :width (setting :width) :select true))

(on-buffer-close :claude-review review/closed!)

(def (note! text face)
  (swap! state update :log conj {kind: :note text: text face: face})
  (show!))

(def (log! entry)
  (swap! state update :log conj entry)
  (show!))

(def finished-stops #{"end_turn" "tool_use" "pause_turn" "stop_sequence"})

(def (answered reply stopped)
  "The reply's blocks to keep in the conversation: all of them when it finished, else only its text."
  (let [kept (api/echoed reply.content)]
    (if (and (not stopped) (contains? finished-stops reply.stop))
      kept
      (filter (fn [block] (= block.type "text")) kept))))

(def (reply!)
  "Asks Claude for the next reply, showing it as it streams in, and adds it to the chat."
  (let [chat @state
        base (count (render/lines-of chat.log))
        shown-at (atom 0)
        live (fn [message]
               (let [now (time/now)]
                 (when (> (- now @shown-at) redraw-every)
                   (reset! shown-at now)
                   (when (buffer-live? chat-buffer)
                     (set-lines chat-buffer (render/entry-lines {kind: :reply content: message.content live: true})
                                :from base)))))
        reply (api/stream! {model: (setting :model) effort: (setting :effort) max-tokens: (setting :max-tokens)
                            system: system tools: tools/definitions messages: chat.messages}
                           live
                           (fn [response] (swap! state assoc :response response)))
        stopped (get @state :stopped)
        kept (answered reply stopped)]
    (swap! state (fn [chat]
                   (-> chat
                       (update :log conj {kind: :reply content: reply.content})
                       (update :messages (fn [messages]
                                           (if (empty? kept)
                                             messages
                                             (conj messages {role: "assistant" content: kept})))))))
    (show!)
    reply))

(def (use-tools! reply)
  "Runs the tools reply asks for, one after another, and adds their results to the chat."
  (let [calls (filter (fn [block] (= block.type "tool_use")) reply.content)
        results (map (fn [call]
                       (if (get @state :stopped)
                         {block: {type: "tool_result" tool_use_id: call.id content: "The person stopped this" is_error: true}}
                         (let [done (tools/run! call)]
                           (log! {kind: :tool name: call.name result: done.text error: done.error})
                           done)))
                     calls)]
    (swap! state update :messages conj {role: "user" content: (map (fn [result] result.block) results)})))

(def (refusal reply)
  (let [category (get reply.details :category)]
    (if category
      $"Claude declined this ({category})."
      "Claude declined this.")))

(def (converse!)
  "Talks with Claude until it has answered, running the tools it asks for along the way."
  (swap! state assoc :busy true :stopped false)
  (try
    (loop []
      (let [reply (reply!)]
        (cond
          [(get @state :stopped) (note! "Stopped" :comment)]
          [reply.error (note! (str "Claude stopped with an error: " (get reply.error :message)) :error)]
          [(= reply.stop "tool_use")
           (use-tools! reply)
           (if (get @state :stopped) (note! "Stopped" :comment) (again))]
          [(= reply.stop "pause_turn") (again)]
          [(= reply.stop "refusal") (note! (refusal reply) :warning)]
          [(= reply.stop "max_tokens") (note! "The reply reached the max-tokens setting and was cut short" :warning)]
          [else nil])))
    (catch error e (note! e.message :error))
    (finally
      (swap! state assoc :busy false :response nil)
      (when (empty? (get @state :project))
        (swap! state assoc :project (where/root)))
      (chats/save! @state)
      (show!))))

(def (send! shown text where)
  "Sends text to Claude with where the person is, showing shown in the chat."
  (when (get @state :busy)
    (raise (error "Claude is still answering; C-c C-k stops it")))
  (let [content [{type: "text" text: (where/context-of where)} {type: "text" text: text}]]
    (swap! state (fn [chat]
                   (-> chat
                       (update :messages conj {role: "user" content: content})
                       (update :log conj {kind: :you text: shown})
                       (assoc :title (or chat.title (render/first-line shown))
                              :project (or chat.project where.project)))))
    (show!)
    (converse!)))

(defcommand claude "Talk with Claude in a chat beside your work"
  (open-chat!))

(defcommand claude-send "Send what you typed to Claude"
  (if (nil? (input-point))
    (run-command 'end-of-buffer)
    (let [text (take-input)]
      (unless (string/blank? text)
        (send! text text (where/whereabouts))))))

(defcommand claude-stop "Stop Claude's reply"
  (if (get @state :busy)
    (do (swap! state assoc :stopped true)
      (when-let [response (get @state :response)]
        (close response.lines))
      (message "Stopping Claude"))
    (message "Claude isn't answering")))

(defcommand claude-new "Start a new chat with Claude"
  (when (get @state :busy)
    (raise (error "Claude is still answering; C-c C-k stops it")))
  (chats/save! @state)
  (reset! state (new-chat))
  (show!)
  (message "A new chat with Claude"))

(def (lines-around start end)
  (let [first (inc (line-of start))
        last (inc (line-of (max start (dec end))))]
    [first last]))

(defcommand claude-ask "Ask Claude about the region, or the whole buffer when nothing is selected"
  (let [where (where/whereabouts)
        [start end] (or (region) [0 (buffer-size)])
        [first last] (lines-around start end)
        code (text-between start end)
        language (string/lower (or (buffer-language) ""))
        file (buffer-file)
        problems (if file
                   (filter (fn [found] (and (>= found.line first) (<= found.line last))) (diagnostics file))
                   [])
        question (string/trim (prompt "Ask Claude about this: "))
        named $"lines {first}-{last} of {where.buffer}"
        text (str question "\n\nHere are " named ":\n```" language "\n" code "\n```"
                  (if (empty? problems)
                    ""
                    (str "\n\nThe problems found there:\n"
                         (string/join "\n" (map (fn [found] $"line {found.line}: {(name found.severity)}: {found.message}") problems)))))]
    (when (string/blank? question)
      (raise (error "Ask Claude something")))
    (open-chat!)
    (send! (str question "\n(about " named ")") text where)))

(defcommand claude-set-key "Give Claude a new Anthropic API key, kept in the vault"
  (api/set-key!)
  (message "Claude will use the new key"))

(defcommand claude-accept-hunk "Accept Claude's hunk at point"
  (review/decide! :accepted))

(defcommand claude-reject-hunk "Reject Claude's hunk at point"
  (review/decide! :rejected))

(defcommand claude-accept-all "Accept all of Claude's hunks"
  (review/decide-all! :accepted))

(defcommand claude-reject-all "Reject all of Claude's hunks"
  (review/decide-all! :rejected))

(defcommand claude-finish-review "Make the accepted edits and tell Claude which were made"
  (review/finish!))

(def (chat-row chat)
  {line: [[(or chat.title "Untitled") :heading] "  "
          [(time/format (or chat.updated 0) "%e %b %H:%M") :comment]]
   id: chat.id})

(defcommand claude-chats "List your chats with Claude in this project"
  (let [project (where/root)
        found (chats/chats-of project)]
    (open-buffer chats-buffer
                 (if (empty? found)
                   [[["No saved chats in this project yet" :comment]]]
                   (map chat-row found))
                 :keys chats-keys :label "Claude chats" :cursor :line :line 0)))

(defcommand claude-open-chat "Carry on the chat at point"
  (when-let [item (line-item)]
    (when (get @state :busy)
      (raise (error "Claude is still answering; C-c C-k stops it")))
    (chats/save! @state)
    (reset! state (merge (new-chat) (chats/load item.id)))
    (close-buffer chats-buffer)
    (open-chat!)
    (show!)))

(defonce told (atom #{}))

(def (tell-once! text)
  (unless (contains? @told text)
    (swap! told conj text)
    (message text)))

(def (suggest-quietly!)
  (try (suggest/suggest!)
    (catch error e (tell-once! (str "Claude couldn't suggest: " e.message)))))

(def (wants-suggestion?)
  (and (setting :suggestions)
       (just-typed?)
       (some? (buffer-file))
       (some? (buffer-language))
       (not (contains? (set (setting :quiet-modes)) (buffer-language)))
       (nil? (region))
       (not (completion-shown?))
       (not (prompting?))))

(def (watch-typing!)
  (on-idle :claude-suggest (/ (setting :suggestion-delay) 1000.0)
           (fn []
             (when (wants-suggestion?)
               (if (api/ready-key)
                 (suggest-quietly!)
                 (tell-once! "Claude's suggestions wait for your API key; M-x claude-suggest asks for it"))))))

(watch-typing!)
(on-settings-change :claude watch-typing!)

(defcommand claude-suggest "Ask Claude for a suggestion at point"
  (api/key!)
  (suggest-quietly!))

(defcommand claude-delete-chat "Delete the chat at point"
  (when-let [item (line-item)]
    (when (yes-or-no "Delete this chat? ")
      (chats/remove! item.id)
      (run-command 'claude-chats))))
