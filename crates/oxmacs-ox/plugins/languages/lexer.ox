(module
  (import (ox string) (ox regex))
  (export lex))

(def (word-face spec word)
  (cond ((contains? spec.keywords word) :keyword)
    ((contains? spec.constants word) :constant)
    ((contains? spec.types word) :type)
    (else nil)))

(def (span-of spec found)
  (let [named found.named
        from found.start]
    (cond ((some? named.comment) [from found.end :comment])
      ((some? named.key) [from (+ from (string/length named.key)) :attribute])
      ((some? named.string) [from found.end :string])
      ((some? named.attribute) [from found.end :attribute])
      ((some? named.macro) [from found.end :macro])
      ((some? named.number) [from found.end :constant])
      ((some? named.call) [from (+ from (string/length named.call)) (or (word-face spec named.call) :function)])
      ((some? named.word) (when-let [face (word-face spec named.word)] [from found.end face]))
      (else nil))))

(def (shifted span offset)
  [(+ offset (nth span 0)) (+ offset (nth span 1)) (nth span 2)])

(def (scan spec text offset)
  (let [tokens (regex/find-all spec.token text)
        width (string/length text)]
    (loop [at 0 spans []]
      (if (= at (count tokens))
        {spans: spans state: nil}
        (let [found (nth tokens at)
              opener found.named.block]
          (if (some? opener)
            (let [block (get spec.blocks opener)
                  closed (regex/find block.close (string/slice text found.end))]
              (if closed
                (let [end (+ found.end closed.end)
                      rest (scan spec (string/slice text end) (+ offset end))]
                  {spans: (into (conj spans [(+ offset found.start) (+ offset end) block.face]) rest.spans)
                   state: rest.state})
                {spans: (conj spans [(+ offset found.start) (+ offset width) block.face]) state: opener}))
            (again (+ at 1)
                   (if-let [span (span-of spec found)] (conj spans (shifted span offset)) spans))))))))

(def (lex spec text state)
  "The spans of a line of code in the language spec describes, and as its state the opener of a comment or string that runs on past the line, or nil.
The spec is a map of :token, a regex whose named groups comment, block, key, string, attribute, macro, number, call and word find the pieces of a line; :blocks, from each opener the block group finds to the :close regex that ends it and its :face; and the sets of :keywords, :constants and :types."
  (if (some? state)
    (let [block (get spec.blocks state)
          width (string/length text)]
      (if-let [closed (regex/find block.close text)]
        (let [rest (scan spec (string/slice text closed.end) closed.end)]
          {spans: (into [[0 closed.end block.face]] rest.spans) state: rest.state})
        {spans: (if (= width 0) [] [[0 width block.face]]) state: state}))
    (scan spec text 0)))

(def toy
  {token: #"(?P<comment>//.*)|(?P<block>/\*|`)|(?P<key>\"[^\"]*\")\s*:|(?P<string>\"[^\"]*\")|(?P<attribute>@\w+)|(?P<macro>^#\w+)|(?P<number>\b\d+)|(?P<call>[a-z]\w*)\s*\(|(?P<word>[A-Za-z]\w*)"
   blocks: {"/*" {close: #"\*/" face: :comment}
            "`" {close: #"^(?:[^`\\]|\\.)*`" face: :string}}
   keywords: #{"if" "return"}
   constants: #{"nil"}
   types: #{"Int"}})

(def (lexed lines)
  (get (reduce (fn [acc text]
                 (let [done (lex toy text acc.state)]
                   {state: done.state spans: (conj acc.spans done.spans)}))
               {state: nil spans: []}
               lines)
       :spans))

(test "words, numbers, strings, calls and comments get their faces"
  (expect (= (lex toy "if f(1) return nil // done" nil)
             {spans: [[0 2 :keyword] [3 4 :function] [5 6 :constant] [8 14 :keyword] [15 18 :constant] [19 26 :comment]]
              state: nil}))
  (expect (= (get (lex toy "{{\"a\": \"b\", x: Int}}" nil) :spans)
             [[1 4 :attribute] [6 9 :string] [14 17 :type]]))
  (expect (= (get (lex toy "#define @wrap if(" nil) :spans)
             [[0 7 :macro] [8 13 :attribute] [14 16 :keyword]])))

(test "comments and strings that run past a line carry on into the next"
  (expect (= (lexed ["a /* one" "two" "" "three */ if" "`x" "y` 2"])
             [[[2 8 :comment]]
              [[0 3 :comment]]
              []
              [[0 8 :comment] [9 11 :keyword]]
              [[0 2 :string]]
              [[0 2 :string] [3 4 :constant]]])))

(test "a block closed on its own line lets the rest of the line be read afresh"
  (expect (= (get (lex toy "/* \"*/ if \"s\" `a\\`b` 1" nil) :spans)
             [[0 6 :comment] [7 9 :keyword] [10 13 :string] [14 20 :string] [21 22 :constant]])))
