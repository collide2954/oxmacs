(module
  (import (ox string) (ox file) (ox process)
          (claude where :only [in-main resolved])
          (claude review :as review))
  (export definitions run! numbered))

(def most-lines 2000)
(def most-text 100000)

(def (numbered text from to)
  "The lines of text from line from to line to, counting from 1, each after its number."
  (let [lines (string/split text "\n")
        total (count lines)
        first (max 1 (or from 1))
        last (min total (or to (+ first (dec most-lines))) (+ first (dec most-lines)))]
    (if (> first total)
      $"There are only {total} lines."
      (str $"Lines {first}-{last} of {total}:\n"
           (string/join "\n" (map (fn [n] (str n "\t" (nth lines (dec n)))) (range first (inc last))))))))

(def (cut text)
  (if (> (string/length text) most-text)
    (str (string/slice text 0 most-text) "\n[cut short]")
    text))

(def (asked-for input key)
  (let [given (get input key)]
    (if (and (string? given) (not (string/blank? given)))
      given
      (raise (error $"The tool needs {(name key)}, a string")))))

(def (list-buffers _input)
  (let [here (in-main buffer-name)]
    (string/join "\n" (map (fn [buffer]
                             (let [[file mode] (with-buffer buffer [(buffer-file) (buffer-language)])]
                               (str buffer
                                    (if file $"  file {file}" "")
                                    (if mode $"  mode {mode}" "")
                                    (if (= buffer here) "  (in front of the person)" ""))))
                           (buffer-list)))))

(def (read-buffer input)
  (let [buffer (asked-for input :buffer)
        text (buffer-contents buffer)]
    (if (nil? text)
      (raise (error $"There is no buffer called {buffer}"))
      (numbered text input.from_line input.to_line))))

(def (read-file input)
  (numbered (file/read (resolved (asked-for input :path))) input.from_line input.to_line))

(def (list-directory input)
  (let [folder (resolved (or input.path "."))]
    (string/join "\n" (map (fn [entry] (if entry.dir (str entry.name "/") entry.name))
                           (file/list folder)))))

(def (search input)
  (let [pattern (asked-for input :pattern)
        folder (resolved (or input.path "."))
        rg (process/which "rg")
        ran (if rg
              (process/run rg ["--line-number" "--no-heading" "--color" "never" "--max-count" "20" "--" pattern folder])
              (process/run "git" ["grep" "--line-number" "-e" pattern "--" folder] :cwd folder))]
    (cond
      [(and (= ran.status 1) (string/blank? ran.stdout)) "No matches"]
      [(not= ran.status 0) (raise (error (str "The search failed: " (string/trim ran.stderr))))]
      [else (string/join "\n" (take 300 (string/lines ran.stdout)))])))

(def (open-in-main input)
  (let [path (resolved (asked-for input :path))]
    (in-main (fn []
               (unless (open-file path)
                 (raise (error $"Couldn't open {path}")))
               (when input.line (goto-line (dec input.line)))))
    $"Opened {path}"))

(def (show-buffer input)
  (let [buffer (asked-for input :buffer)]
    (unless (buffer-exists? buffer)
      (raise (error $"There is no buffer called {buffer}")))
    (in-main (fn [] (switch-to-buffer buffer)))
    $"Showing {buffer}"))

(def (list-commands input)
  (let [wanted (string/lower (or input.filter ""))
        found (filter (fn [command]
                        (or (string/includes? command.name wanted)
                            (string/includes? (string/lower command.summary) wanted)))
                      (in-main commands))]
    (if (empty? found)
      "No command matches"
      (string/join "\n" (map (fn [command]
                               (str command.name (if command.keys $" ({command.keys})" "") " - " command.summary))
                             (take 400 found))))))

(def (run-a-command input)
  (let [command (asked-for input :name)]
    (if (yes-or-no $"Let Claude run M-x {command}? ")
      (do (in-main (fn [] (run-command command)))
        $"Ran M-x {command}")
      (raise (error "The person said no")))))

(def (evaluate-ox input)
  (let [code (asked-for input :code)]
    (show-text "*claude ox*" code :read-only true)
    (if (yes-or-no "Let Claude evaluate the Ox in *claude ox*? ")
      (pr-str (in-main (fn [] (ox-evaluate code))))
      (raise (error "The person said no")))))

(def (problems input)
  (let [found (diagnostics (when input.path (resolved input.path)))]
    (if (empty? found)
      "No errors or warnings"
      (string/join "\n" (map (fn [d] $"{d.file}:{d.line}:{d.column}: {(name d.severity)}: {d.message}")
                             found)))))

(def (edit input)
  (let [target (asked-for input :target)
        edits (get input :edits)]
    (unless (and (vector? edits) (not (empty? edits)))
      (raise (error "edit_file needs edits, a list of {old_text, new_text}")))
    (review/edits! (if (buffer-exists? target) target (resolved target)) edits)))

(def (write input)
  (review/write! (resolved (asked-for input :path)) (or (get input :content) "")))

(def tools
  {list_buffers: list-buffers read_buffer: read-buffer read_file: read-file list_directory: list-directory
   search: search open_file: open-in-main show_buffer: show-buffer list_commands: list-commands
   run_command: run-a-command evaluate_ox: evaluate-ox diagnostics: problems edit_file: edit write_file: write})

(def (schema properties required)
  {type: "object" properties: properties required: required})

(def lines-wanted
  {from_line: {type: "integer" description: "The first line to read, counting from 1"}
   to_line: {type: "integer" description: "The last line to read"}})

(def definitions
  [{name: "list_buffers"
    description: "Lists the buffers open in oxmacs with the file each visits and its mode, marking the one in front of the person."
    input_schema: (schema {} [])}
   {name: "read_buffer"
    description: "Reads a buffer's text as it is now, saved or not, with line numbers. Prefer it over read_file for open files."
    input_schema: (schema (merge {buffer: {type: "string" description: "The buffer's name, as list_buffers gives it"}} lines-wanted) ["buffer"])}
   {name: "read_file"
    description: "Reads a file from disk with line numbers. Relative paths start at the project's folder."
    input_schema: (schema (merge {path: {type: "string"}} lines-wanted) ["path"])}
   {name: "list_directory"
    description: "Lists the files and folders in a folder, folders ending in /. Relative paths start at the project's folder."
    input_schema: (schema {path: {type: "string" description: "The folder; the project's folder when left out"}} [])}
   {name: "search"
    description: "Searches the files under a folder for a regular expression, giving file:line:text for each match."
    input_schema: (schema {pattern: {type: "string"} path: {type: "string" description: "Where to search; the project's folder when left out"}} ["pattern"])}
   {name: "open_file"
    description: "Opens a file in the person's main window, at a line when given."
    input_schema: (schema {path: {type: "string"} line: {type: "integer"}} ["path"])}
   {name: "show_buffer"
    description: "Shows an open buffer in the person's main window."
    input_schema: (schema {buffer: {type: "string"}} ["buffer"])}
   {name: "list_commands"
    description: "Lists oxmacs' commands, the ones M-x runs, with their keys and what they do, filtered by a word in their name or summary. Use it to find the command for something the person asks, such as git-status for the git buffer."
    input_schema: (schema {filter: {type: "string" description: "A word to look for, such as git or window"}} [])}
   {name: "run_command"
    description: "Runs an oxmacs command by name in the person's main window, as M-x would, after the person agrees."
    input_schema: (schema {name: {type: "string" description: "The command's name, as list_commands gives it"}} ["name"])}
   {name: "evaluate_ox"
    description: "Evaluates Ox, oxmacs' Scheme-like plugin language, in the person's user module after they agree, and gives the value of the last form. Ox can do anything a plugin can: open views, change settings, define commands. Prefer run_command when a command does the job."
    input_schema: (schema {code: {type: "string" description: "Ox forms, such as (settings {theme: :adwaita-dark})"}} ["code"])}
   {name: "diagnostics"
    description: "Lists the errors and warnings language servers and linters found, in one file or all."
    input_schema: (schema {path: {type: "string" description: "A file; every file when left out"}} [])}
   {name: "edit_file"
    description: "Proposes edits to a buffer or file, which the person reviews as a diff and accepts or rejects hunk by hunk. Each old_text must appear exactly once in the current text; give enough around it to make it unique. The result says which edits were made. Edits go into the buffer unsaved."
    input_schema: (schema {target: {type: "string" description: "A buffer's name or a file's path"}
                           edits: {type: "array"
                                   items: (schema {old_text: {type: "string"} new_text: {type: "string"}} ["old_text" "new_text"])}}
                          ["target" "edits"])}
   {name: "write_file"
    description: "Proposes a whole new file, or new contents for a file that isn't open, which the person reviews before it is written."
    input_schema: (schema {path: {type: "string"} content: {type: "string"}} ["path" "content"])}])

(def (run! block)
  "Runs the tool a tool_use block asks for and gives the tool_result block to send back."
  (let [result (try
                 (cond
                   [block.invalid {text: "The input wasn't valid JSON; send the call again" error: true}]
                   [(nil? (get tools (keyword block.name))) {text: $"There is no tool called {block.name}" error: true}]
                   [else {text: (cut (str ((get tools (keyword block.name)) (or block.input {})))) error: false}])
                 (catch error e {text: e.message error: true})
                 (catch quit _e {text: "The person cancelled this" error: true}))]
    {block: {type: "tool_result" tool_use_id: block.id content: result.text is_error: result.error}
     text: result.text error: result.error}))

(test "numbered lines count from one and stop at the end"
  (expect (= (numbered "a\nb\nc" 2 9) "Lines 2-3 of 3:\n2\tb\n3\tc"))
  (expect (= (numbered "a" 4 nil) "There are only 1 lines.")))

(test "every tool has a definition and a runner"
  (expect (= (set (map (fn [tool] (keyword tool.name)) definitions)) (set (keys tools)))))

(test "unknown tools and broken input are reported back to Claude"
  (expect (= (get (run! {id: "1" name: "fly" input: {}}) :text) "There is no tool called fly"))
  (expect (get (run! {id: "2" name: "search" input: {} invalid: "{"}) :error)))
