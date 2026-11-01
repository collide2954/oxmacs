(module
  (import (ox string) (ox json) (ox http) (ox process) (ox path) (ox file)
          (vault store :as vault))
  (export version key! ready-key set-key! body headers fresh apply-event echoed failure stream!))

(def version "2026-10-21")

(defonce known-key (atom nil))

(def (key-note)
  (when-let [folder (plugin-folder 'claude)]
    (path/join folder "key-in-vault")))

(def (stored?)
  (if-let [file (key-note)]
    (file/exists? file)
    false))

(def (from-vault)
  (when (or (not (vault/locked?)) (stored?))
    (vault/unlock)
    (get (vault/find-entry (setting :key-entry)) :password)))

(def (set-key!)
  "Asks for an Anthropic API key, keeps it in the vault and uses it from now on."
  (let [typed (string/trim (prompt "Anthropic API key, kept in the vault: " :secret true))
        title (setting :key-entry)]
    (when (string/blank? typed)
      (raise (error "Claude needs an API key; console.anthropic.com makes one")))
    (vault/unlock)
    (if-let [old (vault/find-entry title)]
      (vault/change! old.id (fn [entry] (assoc entry :password typed)))
      (vault/add! {title: title username: "" password: typed url: "https://console.anthropic.com"
                   totp: "" notes: "Used by oxmacs' claude plugin"}))
    (when-let [file (key-note)]
      (file/make-dir (path/parent file))
      (file/write file "The Anthropic API key is in the vault\n"))
    (reset! known-key typed)))

(def (key!)
  "The API key: the vault's entry, else ANTHROPIC_API_KEY, else one asked for once and kept in the vault."
  (or @known-key
      (when-let [found (or (from-vault) (process/env "ANTHROPIC_API_KEY"))]
        (reset! known-key found))
      (set-key!)))

(def (ready-key)
  "The API key when it can be had without asking anything: known already, in the unlocked vault or in ANTHROPIC_API_KEY."
  (or @known-key
      (when-let [found (or (when (not (vault/locked?)) (get (vault/find-entry (setting :key-entry)) :password))
                           (process/env "ANTHROPIC_API_KEY"))]
        (reset! known-key found))))

(def (haiku? model)
  (string/includes? model "haiku"))

(def (body asked)
  "The JSON body of a streamed Messages API request for asked, a map of :model, :messages and
  optionally :system, :tools, :effort, :max-tokens and :quick, which turns thinking off where it can."
  (merge {model: asked.model
          max_tokens: (or asked.max-tokens 64000)
          stream: true
          cache_control: {type: "ephemeral"}
          output_config: {effort: (name (or asked.effort :high))}
          messages: asked.messages}
         (if (and asked.quick (haiku? asked.model))
           {thinking: {type: "disabled"}}
           {thinking: {type: "adaptive" display: "summarized"}})
         (if asked.system {system: asked.system} {})
         (if (empty? (or asked.tools [])) {} {tools: asked.tools})
         (if (haiku? asked.model) {} {fallbacks: "default"})))

(def (headers key model)
  (merge {x-api-key: key anthropic-version: version content-type: "application/json"}
         (if (haiku? model) {} {anthropic-beta: "server-side-fallback-2026-07-01"})))

(def (fresh)
  {content: [] stop: nil details: nil usage: nil model: nil error: nil})

(def (put items index item)
  (if (< index (count items))
    (assoc items index item)
    (conj items item)))

(def (grown block delta)
  (cond
    [(= delta.type "text_delta") (update block :text str delta.text)]
    [(= delta.type "thinking_delta") (update block :thinking str delta.thinking)]
    [(= delta.type "signature_delta") (assoc block :signature delta.signature)]
    [(= delta.type "input_json_delta") (update block :partial str delta.partial_json)]
    [else block]))

(def (closed block)
  (if (= block.type "tool_use")
    (let [text (or block.partial "")
          parsed (try (if (string/blank? text) {} (json/read text))
                   (catch error _e nil))]
      (dissoc (if (map? parsed)
                (assoc block :input parsed)
                (assoc block :input {} :invalid text))
              :partial))
    block))

(def (changed message index change)
  (let [content message.content]
    (if (< index (count content))
      (assoc message :content (assoc content index (change (nth content index))))
      message)))

(def (apply-event message event)
  "message with one streamed event of the Messages API applied."
  (let [kind event.type]
    (cond
      [(= kind "message_start")
       (assoc message :model event.message.model :usage event.message.usage)]
      [(= kind "content_block_start")
       (let [block event.content_block]
         (assoc message :content (put message.content event.index
                                      (if (= block.type "tool_use") (assoc block :partial "") block))))]
      [(= kind "content_block_delta")
       (changed message event.index (fn [block] (grown block event.delta)))]
      [(= kind "content_block_stop")
       (changed message event.index closed)]
      [(= kind "message_delta")
       (assoc message :stop event.delta.stop_reason :details event.delta.stop_details
              :usage (merge (or message.usage {}) (or event.usage {})))]
      [(= kind "error")
       (assoc message :error event.error)]
      [else message])))

(def (echoed content)
  "The blocks of a reply to send back in later turns: after a fallback, only the text from before it."
  (let [marks (filter (fn [i] (= (get (nth content i) :type) "fallback")) (range (count content)))
        cut (if (empty? marks) -1 (last marks))]
    (vec (keep (fn [i]
                 (let [block (nth content i)]
                   (cond
                     [(= block.type "fallback") nil]
                     [(and (< i cut) (not= block.type "text")) nil]
                     [(and (= block.type "text") (string/blank? (or block.text ""))) nil]
                     [else (dissoc block :invalid)])))
               (range (count content))))))

(def (failure status text)
  "What to tell someone when the API answered status with text."
  (let [said (try (get (get (json/read text) :error) :message) (catch error _e nil))
        reason (or said (string/trim text))]
    (cond
      [(= status 401) $"Anthropic refused the API key ({reason}); M-x claude-set-key gives another"]
      [(= status 429) $"Claude is rate limited right now: {reason}"]
      [(= status 529) "Claude is overloaded right now; try again in a moment"]
      [else $"Claude's API answered {status}: {reason}"])))

(def (rest-of lines)
  (loop [seen []]
    (let [line (receive lines)]
      (if (nil? line) seen (again (conj seen line))))))

(def (event-from data)
  (json/read (string/join "\n" data)))

(def (stream! asked on-change on-open)
  "Sends asked, as body takes it, and reads the reply as it streams in, calling on-change with the
  reply so far after each event, and on-open with the response first so that closing its :lines
  stops it. Returns the reply as fresh and apply-event build it."
  (let [url (str (string/trim-end (setting :api-url)) "/v1/messages")
        response (http/open :post url :headers (headers (or asked.key (key!)) asked.model)
                            :body (json/write (body (dissoc asked :key))))]
    (on-open response)
    (if (not= response.status 200)
      (raise (error (failure response.status (string/join "\n" (rest-of response.lines)))))
      (loop [message (fresh) data []]
        (let [line (receive response.lines)]
          (cond
            [(nil? line)
             (await response.done)
             (if (empty? data) message (apply-event message (event-from data)))]
            [(= line "")
             (if (empty? data)
               (again message [])
               (let [next (apply-event message (event-from data))]
                 (on-change next)
                 (again next [])))]
            [(string/starts-with? line "data:")
             (again message (conj data (string/trim-start (string/slice line 5))))]
            [else (again message data)]))))))

(test "streamed events build up the reply's blocks"
  (let [events [{type: "message_start" message: {model: "claude-opus-5-5" usage: {input_tokens: 9}}}
                {type: "content_block_start" index: 0 content_block: {type: "thinking" thinking: ""}}
                {type: "content_block_delta" index: 0 delta: {type: "thinking_delta" thinking: "Hm"}}
                {type: "content_block_delta" index: 0 delta: {type: "signature_delta" signature: "s1"}}
                {type: "content_block_stop" index: 0}
                {type: "content_block_start" index: 1 content_block: {type: "text" text: ""}}
                {type: "content_block_delta" index: 1 delta: {type: "text_delta" text: "Hel"}}
                {type: "content_block_delta" index: 1 delta: {type: "text_delta" text: "lo"}}
                {type: "content_block_start" index: 2 content_block: {type: "tool_use" id: "t1" name: "open_file" input: {}}}
                {type: "content_block_delta" index: 2 delta: {type: "input_json_delta" partial_json: "{\"path\": "}}
                {type: "content_block_delta" index: 2 delta: {type: "input_json_delta" partial_json: "\"a.rs\"}"}}
                {type: "content_block_stop" index: 2}
                {type: "message_delta" delta: {stop_reason: "tool_use"} usage: {output_tokens: 30}}]
        reply (reduce apply-event (fresh) events)]
    (expect (= reply.model "claude-opus-5-5"))
    (expect (= reply.stop "tool_use"))
    (expect (= reply.usage {input_tokens: 9 output_tokens: 30}))
    (expect (= reply.content [{type: "thinking" thinking: "Hm" signature: "s1"}
                              {type: "text" text: "Hello"}
                              {type: "tool_use" id: "t1" name: "open_file" input: {path: "a.rs"}}]))))

(test "tool input that isn't JSON is kept to say so"
  (let [reply (reduce apply-event (fresh)
                      [{type: "content_block_start" index: 0 content_block: {type: "tool_use" id: "t" name: "x" input: {}}}
                       {type: "content_block_delta" index: 0 delta: {type: "input_json_delta" partial_json: "{\"a\": "}}
                       {type: "content_block_stop" index: 0}])]
    (expect (= (first reply.content) {type: "tool_use" id: "t" name: "x" input: {} invalid: "{\"a\": "}))
    (expect (= (echoed reply.content) [{type: "tool_use" id: "t" name: "x" input: {}}]))))

(test "after a fallback only the text from before it is sent back"
  (expect (= (echoed [{type: "thinking" thinking: "a" signature: "s"}
                      {type: "text" text: "Part"}
                      {type: "tool_use" id: "t" name: "x" input: {}}
                      {type: "fallback" from: {model: "a"} to: {model: "b"}}
                      {type: "thinking" thinking: "b" signature: "z"}
                      {type: "text" text: ""}
                      {type: "text" text: "Rest"}])
             [{type: "text" text: "Part"}
              {type: "thinking" thinking: "b" signature: "z"}
              {type: "text" text: "Rest"}])))

(test "requests stream, think and fall back unless the model is a Haiku"
  (let [opus (body {model: "claude-opus-5-5" messages: [] effort: :xhigh})
        haiku (body {model: "claude-haiku-5-5" messages: [] quick: true})]
    (expect (= [opus.stream opus.output_config opus.thinking opus.fallbacks]
               [true {effort: "xhigh"} {type: "adaptive" display: "summarized"} "default"]))
    (expect (= [haiku.thinking (contains? haiku :fallbacks)] [{type: "disabled"} false]))
    (expect (= (get (headers "k" "claude-opus-5-5") :anthropic-beta) "server-side-fallback-2026-07-01"))
    (expect (nil? (get (headers "k" "claude-haiku-5-5") :anthropic-beta)))))

(test "API failures say what went wrong and what to do"
  (expect (= (failure 401 "{\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid x-api-key\"}}")
             "Anthropic refused the API key (invalid x-api-key); M-x claude-set-key gives another"))
  (expect (= (failure 500 "oops") "Claude's API answered 500: oops")))
