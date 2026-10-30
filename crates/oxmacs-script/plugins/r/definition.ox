(module
  (import (ox string) (ox path) (ox file) (ox regex)
          (r session :as session)
          (r help :as help)
          (r language :as language))
  (export line find-definition answered))

(def skipped #{"renv" "node_modules" "packrat"})
(def most-files 2000)
(def deepest 8)

(defonce asked (atom nil))

(def (assigns? text name equals)
  (let [trimmed (string/trim-start text)
        after (cond ((string/starts-with? trimmed $"`{name}`")
                     (string/slice trimmed (+ (string/length name) 2)))
                ((and (string/starts-with? trimmed name)
                      (not (regex/matches? #"^[\p{L}\p{N}._]" (string/slice trimmed (string/length name)))))
                 (string/slice trimmed (string/length name)))
                (else nil))]
    (when after
      (let [after (string/trim-start after)]
        (or (string/starts-with? after "<-")
            (string/starts-with? after "<<-")
            (and equals (string/starts-with? after "=") (not (string/starts-with? after "=="))))))))

(def (line text name)
  "The line, counting from 0, where name is assigned in R text: a top-level assignment first, or any other."
  (let [lines (string/lines text)
        numbers (range (count lines))]
    (or (find (fn [at] (let [shown (nth lines at)]
                         (and (not (regex/matches? #"^\s" shown)) (assigns? shown name true))))
              numbers)
        (find (fn [at] (assigns? (nth lines at) name false)) numbers))))

(def (listing folder)
  (try (sort-by (fn [entry] entry.path) (await (file/list folder)))
    (catch error _e [])))

(def (r-file? name)
  (or (contains? #{"R" "r"} (path/extension name)) (= name ".Rprofile")))

(def (walk entries at depth found)
  (if (or (>= at (count entries)) (>= (count found) most-files))
    found
    (let [entry (nth entries at)
          found (cond (entry.dir
                       (if (and (< depth deepest)
                                (not (string/starts-with? entry.name "."))
                                (not (contains? skipped entry.name)))
                         (walk (listing entry.path) 0 (+ depth 1) found)
                         found))
                  ((r-file? entry.name) (conj found entry.path))
                  (else found))]
      (walk entries (+ at 1) depth found))))

(def (first-definition files name at)
  (when (< at (count files))
    (let [path (nth files at)
          text (try (await (file/read path)) (catch error _e nil))
          found (when text (line text name))]
      (if found [path found] (first-definition files name (+ at 1))))))

(def (visit path at origin)
  (when (open-file path)
    (apply remember-location origin)
    (goto-line at)
    true))

(def (in-buffers name origin)
  (when-let [[buffer at] (some (fn [buffer]
                                 (when (= (buffer-language buffer) "R")
                                   (when-let [found (line (or (buffer-contents buffer) "") name)]
                                     [buffer found])))
                               (buffer-list))]
    (apply remember-location origin)
    (switch-to-buffer buffer)
    (goto-line at)
    true))

(def (in-project name origin)
  (when-let [root (session/project-here)]
    (when-let [[path at] (first-definition (walk (listing root) 0 0 []) name 0)]
      (visit path at origin))))

(def (find-definition)
  (if-let [name (help/symbol-in (or (line-text (current-line)) "") (current-column))]
    (let [origin [(buffer-name) (point)]
          running (session/current-session)]
      (cond ((in-buffers name origin) true)
        ((in-project name origin) true)
        ((and (session/running? running) (console-waiting? running))
         (reset! asked {name: name origin: origin})
         (session/request running $"getOption(\"oxmacs\")$definition({(session/r-string name)})")
         true)
        ((language/ready?) nil)
        (else (message $"No definition of {name} in open R buffers or the project") true)))
    (cond ((language/ready?) nil)
      (else (message "No symbol at point") true))))

(def (answered _ payload)
  (when-let [question @asked]
    (reset! asked nil)
    (let [fields (string/split payload "\t" 3)
          name (first fields)
          kind (get fields 1)
          place (or (get fields 2) "")]
      (when (= name question.name)
        (cond ((= kind "file")
               (let [pieces (string/split place "\t")
                     path (if (> (count pieces) 1) (string/join "\t" (pop pieces)) place)
                     at (if (> (count pieces) 1) (or (string/to-number (last pieces)) 1) 1)]
                 (unless (visit path (- at 1) question.origin)
                   (message $"Cannot open {path}"))))
          ((= kind "source")
           (apply remember-location question.origin)
           (show-text $"*R source: {name}*" place :mode "R" :read-only true))
          (else (message $"No definition of {name} found")))))))
