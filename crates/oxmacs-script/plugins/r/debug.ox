(module
  (import (ox string) (ox path) (ox regex)
          (r session :as session)
          (r help :as help))
  (export prompts watches heard))

(def prompts ["""^Browse\[\d+\]> $"""])
(def watches {debug: "^(debug at |debug: |Called from: |debugging in: )"})
(def guide "Debugging: <f10> next  S-<f4> step in  S-<f6> finish  S-<f5> continue  S-<f8> stop")

(def (show-source session path line)
  (let [here (buffer-name)]
    (when (session/known? here)
      (when-let [script (find (fn [name] (= (session/linked name) here)) (filter session/script? (buffer-list)))]
        (display-buffer script :select true)))
    (when (open-file path)
      (goto-line line)
      (session/show-console session false))))

(def (heard session payload)
  (when-let [hit (regex/find #"^debug at (.+)#(\d+): " payload)]
    (let [[file number] hit.groups
          path (if (path/absolute? file) file (path/join (or (session/directory-of session) (default-directory)) file))]
      (show-source session path (- (string/to-number number) 1))))
  (message guide))

(def (step command)
  (let [running (session/current-session)
        prompt (when (session/running? running) (console-prompt running))]
    (if (and prompt (string/starts-with? prompt "Browse["))
      (session/evaluate running command)
      (message "R is not debugging"))))

(defcommand r-debug-next "Run the next line of the code R is debugging" (step "n"))
(defcommand r-debug-step "Step into the function called on the line R is debugging" (step "s"))
(defcommand r-debug-finish "Finish the function or loop R is debugging" (step "f"))
(defcommand r-debug-continue "Continue running the code R is debugging" (step "c"))
(defcommand r-debug-stop "Stop debugging and go back to R's prompt" (step "Q"))

(def (flag call)
  (if-let [name (help/symbol-at-point)]
    (session/send $"{call}({name})")
    (message "No function name at point")))

(defcommand r-debug-function "Debug the function at point the next time it runs" (flag "debugonce"))
(defcommand r-undebug-function "Stop debugging the function at point" (flag "undebug"))
