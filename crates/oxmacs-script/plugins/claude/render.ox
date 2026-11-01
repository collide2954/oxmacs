(module
  (import (ox string) (ox json))
  (export text-lines entry-lines lines-of summary first-line))

(def (inline line)
  "line with `code` spans set apart."
  (let [parts (string/split line "`")]
    (if (< (count parts) 3)
      line
      (keep (fn [i]
              (let [part (nth parts i)]
                (cond
                  [(= part "") nil]
                  [(odd? i) [part :string]]
                  [else part])))
            (range (count parts))))))

(def (text-lines text)
  "The lines of Claude's text, with code blocks, headings and inline code set apart."
  (let [rows (string/split (or text "") "\n")]
    (loop [i 0 fenced false out []]
      (if (>= i (count rows))
        out
        (let [line (nth rows i)
              fence (string/starts-with? (string/trim-start line) "```")]
          (cond
            [fence (again (inc i) (not fenced) (conj out [[line :comment]]))]
            [fenced (again (inc i) fenced (conj out [[line :string]]))]
            [(string/starts-with? line "#") (again (inc i) fenced (conj out [[line :heading]]))]
            [else (again (inc i) fenced (conj out (inline line)))]))))))

(def (first-line text)
  (let [shown (string/trim (or text ""))
        at (string/index-of shown "\n")]
    (string/truncate (if at (string/slice shown 0 at) shown) 100)))

(def (summary input)
  "A short note of what a tool was asked to do."
  (let [named (some (fn [key] (get input key)) [:path :buffer :target :name :pattern :filter :code])]
    (if named
      (first-line (str named))
      (string/truncate (json/write input) 80))))

(def (block-lines block)
  (cond
    [(= block.type "thinking")
     (if (string/blank? (or block.thinking ""))
       []
       (map (fn [line] [[(str "· " line) :comment]]) (string/lines block.thinking)))]
    [(= block.type "text") (text-lines block.text)]
    [(= block.type "tool_use") [[[(str "⚙ " block.name " " (summary block.input)) :function]]]]
    [(= block.type "fallback") [[[(str "(" (get block.to :model) " carried on)") :comment]]]]
    [else []]))

(def (entry-lines entry)
  "The lines one entry of a chat shows."
  (cond
    [(= entry.kind :you)
     (concat [[["You" :keyword]]] (string/split entry.text "\n") [""])]
    [(= entry.kind :reply)
     (concat [[["Claude" :function]]]
             (mapcat block-lines entry.content)
             (if entry.live [[["…" :comment]]] [])
             [""])]
    [(= entry.kind :tool)
     [[[(str "  ↳ " (first-line entry.result)) (if entry.error :error :comment)]] ""]]
    [(= entry.kind :note)
     [[[entry.text (or entry.face :comment)]] ""]]
    [else []]))

(def (lines-of log)
  (mapcat entry-lines log))

(test "replies set code blocks, headings and inline code apart"
  (expect (= (text-lines "# Plan\nUse `map` here:\n```ox\n(map f xs)\n```")
             [[["# Plan" :heading]]
              ["Use " ["map" :string] " here:"]
              [["```ox" :comment]]
              [["(map f xs)" :string]]
              [["```" :comment]]])))

(test "a chat shows who said what and what tools did"
  (expect (= (lines-of [{kind: :you text: "Open main.rs"}
                        {kind: :reply content: [{type: "thinking" thinking: "Easy"}
                                                {type: "tool_use" name: "open_file" input: {path: "main.rs"}}]}
                        {kind: :tool result: "Opened main.rs\nat line 1" error: false}])
             [[["You" :keyword]] "Open main.rs" ""
              [["Claude" :function]] [["· Easy" :comment]] [["⚙ open_file main.rs" :function]] ""
              [["  ↳ Opened main.rs" :comment]] ""])))
