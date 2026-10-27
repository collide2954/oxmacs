(module
  (import (ox file) (ox path) (ox time)
          (dired listing :as l)))

(defonce listings (atom {}))

(def (current)
  (get @listings (buffer-name)))

(def (focused listing)
  (when-let [entry (l/entry-at listing (current-line))]
    entry.name))

(def (place listing focus)
  (let [line (current-line)
        target (or (and focus (l/line-of listing focus))
                   (if (and (l/has-parent listing) (= line 1))
                     line
                     (max (l/first-entry-line listing) (min line (l/last-entry-line listing)))))]
    (goto-line target (l/name-column listing))))

(def (change directory edit focus)
  (swap! listings update directory (fn [listing] (when listing (edit listing))))
  (when-let [listing (get @listings directory)]
    (set-lines directory (l/render listing (time/now)))
    (when (= (buffer-name) directory)
      (place listing focus))))

(def (update-here edit)
  (when-let [listing (current)]
    (change listing.directory edit (focused listing))))

(def (keep-marks marks entries)
  (let [names (set (map :name entries))]
    (reduce (fn [kept name] (if (contains? names name) (assoc kept name (get marks name)) kept))
            {} (keys marks))))

(def (refresh directory)
  (let [shown (get @listings directory)
        entries (await (file/list directory :sort (if shown shown.order :name)))]
    (change directory
            (fn [listing]
              (assoc listing :entries entries :marks (keep-marks listing.marks entries)))
            (when (and shown (= (buffer-name) directory)) (focused shown)))))

(def (refresh-all)
  (map (fn [directory]
         (try (refresh directory) (catch error _e (swap! listings dissoc directory))))
       (keys @listings))
  nil)

(def listing-keys
  {"n" 'dired-next-line "SPC" 'dired-next-line "C-n" 'dired-next-line "<down>" 'dired-next-line
   "p" 'dired-previous-line "C-p" 'dired-previous-line "<up>" 'dired-previous-line
   "RET" 'dired-find-file "f" 'dired-find-file "e" 'dired-find-file
   "^" 'dired-up-directory "g" 'dired-revert "s" 'dired-cycle-sort "(" 'dired-hide-details-mode
   "m" 'dired-mark "d" 'dired-flag-file-deletion "u" 'dired-unmark "DEL" 'dired-unmark-backward
   "U" 'dired-unmark-all-marks "t" 'dired-toggle-marks "x" 'dired-do-flagged-delete
   "D" 'dired-do-delete "R" 'dired-do-rename "C" 'dired-do-copy "+" 'dired-create-directory
   "W" 'dired-do-open "q" 'quit-window})

(def (show listing focus)
  (swap! listings assoc listing.directory listing)
  (open-buffer listing.directory (l/render listing (time/now))
               :keys listing-keys :directory listing.directory)
  (place listing focus))

(def (visit directory focus order details)
  (let [entries (await (file/list directory :sort order))]
    (show {directory: directory entries: entries order: order details: details marks: {}}
          (or focus (when-let [entry (first entries)] entry.name)))))

(def (move-to directory focus)
  (let [old (current)]
    (visit directory focus old.order old.details)
    (unless (= old.directory directory)
      (swap! listings dissoc old.directory)
      (close-buffer old.directory))))

(def (quietly work)
  (try (work) (catch error e (unless (= e.message "Quit") (message e.message)))))

(defcommand dired "List a directory to manage its files"
  (quietly (fn [] (visit (await (prompt-path "Dired (directory): " :directories true)) nil :name true))))

(def (up-directory)
  (when-let [listing (current)]
    (when-let [parent (path/parent listing.directory)]
      (move-to parent (path/name listing.directory)))))

(defcommand dired-jump "List the directory of the current file"
  (if (current)
    (up-directory)
    (let [file (buffer-file)]
      (quietly (fn [] (visit (if file (path/parent file) (default-directory))
                             (when file (path/name file)) :name true))))))

(def (step delta)
  (when-let [listing (current)]
    (let [line (+ (current-line) delta)
          line (max 1 (min line (l/last-entry-line listing)))]
      (goto-line line (l/name-column listing)))))

(defcommand dired-next-line "Move to the next file" (step 1))
(defcommand dired-previous-line "Move to the previous file" (step -1))

(defcommand dired-find-file "Open the file or directory on this line"
  (when-let [listing (current)]
    (let [line (current-line)]
      (if (and (l/has-parent listing) (= line 1))
        (up-directory)
        (if-let [entry (l/entry-at listing line)]
          (let [target (path/join listing.directory entry.name)]
            (if entry.dir (quietly (fn [] (move-to target nil))) (open-file target)))
          (message "No file on this line"))))))

(defcommand dired-up-directory "Go up to the parent directory" (up-directory))

(defcommand dired-revert "Read the directory again"
  (when-let [listing (current)] (quietly (fn [] (refresh listing.directory)))))

(defcommand dired-cycle-sort "Sort by name, newest or largest"
  (update-here (fn [listing]
                 (let [order (l/next-order listing.order)]
                   (assoc listing :order order :entries (file/sorted listing.entries :by order))))))

(defcommand dired-hide-details-mode "Show or hide sizes and dates"
  (update-here (fn [listing] (assoc listing :details (not listing.details)))))

(def (mark-here mark)
  (when-let [listing (current)]
    (when-let [entry (l/entry-at listing (current-line))]
      (change listing.directory
              (fn [listing]
                (assoc listing :marks (if mark
                                        (assoc listing.marks entry.name mark)
                                        (dissoc listing.marks entry.name))))
              entry.name))))

(defcommand dired-mark "Mark this file and move on" (mark-here :marked) (step 1))
(defcommand dired-flag-file-deletion "Flag this file for deletion and move on" (mark-here :flagged) (step 1))
(defcommand dired-unmark "Remove this file's mark and move on" (mark-here nil) (step 1))
(defcommand dired-unmark-backward "Move back and remove that file's mark" (step -1) (mark-here nil))

(defcommand dired-unmark-all-marks "Remove every mark and deletion flag"
  (update-here (fn [listing] (assoc listing :marks {}))))

(defcommand dired-toggle-marks "Mark the unmarked files and unmark the marked ones"
  (update-here (fn [listing]
                 (assoc listing :marks
                        (reduce (fn [marks entry]
                                  (let [mark (get listing.marks entry.name)]
                                    (cond
                                      [(= mark :marked) (dissoc marks entry.name)]
                                      [(nil? mark) (assoc marks entry.name :marked)]
                                      [else marks])))
                                listing.marks listing.entries)))))

(def (targets listing mark at-point)
  (let [marked (filter (fn [entry] (= (get listing.marks entry.name) mark)) listing.entries)]
    (if (and (empty? marked) at-point)
      (if-let [entry (l/entry-at listing (current-line))] [entry] [])
      marked)))

(def (describe names)
  (let [n (count names)]
    (cond
      [(= n 1) (first names)]
      [(= n 2) "{(nth names 0)} and {(nth names 1)}"]
      [(= n 3) "{(nth names 0)}, {(nth names 1)} and {(nth names 2)}"]
      [else "{(nth names 0)}, {(nth names 1)} and {(- n 2)} more"])))

(def (files n)
  (if (= n 1) "1 file" "{n} files"))

(def ^:shadow (delete flagged)
  (when-let [listing (current)]
    (let [found (targets listing (if flagged :flagged :marked) (not flagged))]
      (if (empty? found)
        (message (if flagged "(No deletions requested)" "No file on this line"))
        (let [trash (get (settings) :delete-to-trash)
              names (map :name found)]
          (quietly
           (fn []
             (when (await (yes-or-no "{(if trash "Trash" "Delete")} {(describe names)}?"))
               (map (fn [entry] (await (delete-path (path/join listing.directory entry.name)))) found)
               (refresh-all)
               (message "{(if trash "Trashed" "Deleted")} {(files (count found))}")))))))))

(defcommand dired-do-flagged-delete "Delete the files flagged for deletion" (delete true))
(defcommand dired-do-delete "Delete the marked files, or this one" (delete false))

(def (transfer moving)
  (when-let [listing (current)]
    (let [found (targets listing :marked true)
          names (map :name found)
          question (cond
                     [(empty? found) nil]
                     [(= (count found) 1) "{(if moving "Rename" "Copy")} {(first names)} to: "]
                     [else "{(if moving "Move" "Copy")} {(files (count found))} to: "])]
      (if (nil? question)
        (message "No file on this line")
        (quietly
         (fn []
           (let [target (await (prompt-path question))
                 into (await (file/exists? target))
                 into (and into (get (await (file/metadata target)) :dir))]
             (if (and (> (count found) 1) (not into))
               (message "Target must be a directory: {(path/abbreviate target)}")
               (do
                 (map (fn [entry]
                        (let [from (path/join listing.directory entry.name)
                              to (if into (path/join target entry.name) target)]
                          (if moving
                            (await (move-path from to))
                            (when (or (not (await (file/exists? to)))
                                      (await (yes-or-no "{(path/abbreviate to)} exists; replace it? ")))
                              (await (file/copy from to))))))
                      found)
                 (refresh-all)
                 (message "{(if moving "Moved" "Copied")} {(files (count found))}"))))))))))

(defcommand dired-do-rename "Rename or move the marked files, or this one" (transfer true))
(defcommand dired-do-copy "Copy the marked files, or this one" (transfer false))

(defcommand dired-create-directory "Create a directory"
  (when-let [listing (current)]
    (quietly
     (fn []
       (let [target (await (prompt-path "Create directory: " :directories true))]
         (if (await (file/exists? target))
           (message "{(path/abbreviate target)} already exists")
           (do
             (await (file/make-dir target))
             (refresh listing.directory)
             (when (= (path/parent target) listing.directory)
               (place (current) (path/name target))))))))))

(defcommand dired-do-open "Open the marked files, or this one, with their usual apps"
  (when-let [listing (current)]
    (let [found (targets listing :marked true)]
      (if (empty? found)
        (message "No file on this line")
        (map (fn [entry] (open-externally (path/join listing.directory entry.name))) found)))))

(on-open-directory (fn [directory] (quietly (fn [] (visit directory nil :name true)))))
(on-disk-change :dired (fn [] (refresh-all)))
