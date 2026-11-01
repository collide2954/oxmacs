(module
  (import (ox string)
          (claude api))
  (export request-of tidy text-of suggest!))

(def most-before 4000)
(def most-after 1500)

(def instructions
  "You complete code in an editor. Reply with only the text that goes at <cursor/>: no explanation, no code fence, and nothing that is already before or after it. Finish the current line, or write the next few lines when the code clearly goes on. Reply with nothing when no completion fits.")

(def (request-of model before after language file)
  "The request asking model what goes between before and after in file."
  {model: model effort: :low max-tokens: 300 quick: true
   system: [{type: "text" text: instructions}]
   messages: [{role: "user"
               content: (str "File: " (or file "untitled") (if language (str " (" language ")") "")
                             "\n\n<code>\n" before "<cursor/>" after "\n</code>")}]})

(def (text-of content)
  (string/join "" (keep (fn [block] (when (= block.type "text") block.text)) content)))

(def (unfenced lines)
  (let [lines (if (and (not (empty? lines)) (string/starts-with? (first lines) "```")) (vec (rest lines)) lines)]
    (if (and (not (empty? lines)) (string/starts-with? (string/trim (last lines)) "```"))
      (take (dec (count lines)) lines)
      lines)))

(def (tidy text after)
  "Claude's suggestion without code fences, trailing blanks or the text already after point; nil when nothing is left."
  (let [shown (string/trim-end (string/join "\n" (unfenced (string/split (or text "") "\n"))))
        rest-of-line (string/trim (first (string/split after "\n")))
        shown (if (and (not (string/blank? rest-of-line)) (string/ends-with? shown rest-of-line))
                (string/trim-end (string/slice shown 0 (- (count shown) (count rest-of-line))))
                shown)]
    (when (not (string/blank? shown)) shown)))

(defonce latest (atom 0))
(defonce asking (atom nil))

(def (suggest!)
  "Asks Claude what comes at point and shows it in grey, unless the person moved on meanwhile."
  (when-let [key (api/ready-key)]
    (when-let [old @asking]
      (close old.lines))
    (let [ticket (swap! latest inc)
          at (point)
          text (buffer-text)
          before (string/slice text (max 0 (- at most-before)) at)
          after (string/slice text at (min (count text) (+ at most-after)))
          reply (api/stream! (assoc (request-of (setting :suggestion-model) before after (buffer-language) (buffer-file))
                                    :key key)
                             (fn [_] nil)
                             (fn [response] (reset! asking response)))
          shown (tidy (text-of reply.content) after)]
      (when (and shown (= ticket @latest) (= (point) at) (= (buffer-size) (count text)))
        (suggest shown)))))

(test "a suggestion is asked for with the code around point"
  (let [asked (request-of "claude-haiku-5-5" "fn main() {\n    let x = " ";\n}" "Rust" "/p/main.rs")]
    (expect (= [asked.model asked.effort asked.quick] ["claude-haiku-5-5" :low true]))
    (expect (= (get (first asked.messages) :content)
               "File: /p/main.rs (Rust)\n\n<code>\nfn main() {\n    let x = <cursor/>;\n}\n</code>"))))

(test "suggestions lose fences and what already follows point"
  (expect (= (tidy "```rust\n42\n```" ";\n}") "42"))
  (expect (= (tidy "42;" ";\n}") "42"))
  (expect (= (tidy "x + 1\n  " "") "x + 1"))
  (expect (nil? (tidy "  \n" "")))
  (expect (= (text-of [{type: "thinking" thinking: "x"} {type: "text" text: "a"} {type: "text" text: "b"}]) "ab")))
