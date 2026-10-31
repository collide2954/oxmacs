(module
  (import (ox string) (ox path))
  (export in-main whereabouts context-of root resolved))

(def (in-main f)
  "Calls f with the main window selected, going back to the side window after when it was selected."
  (let [side (side-window)
        from-side (and side side.selected)]
    (when from-side (select-main-window))
    (try (f)
      (finally (when from-side (select-side-window))))))

(def (whereabouts)
  "Where the person is in the main window: its buffer, file, mode, project and point."
  (in-main (fn []
             {buffer: (buffer-name) file: (buffer-file) mode: (buffer-language)
              project: (project-root) directory: (default-directory)
              line: (inc (current-line)) column: (inc (current-column))})))

(def (context-of where)
  (string/join "\n" (keep identity
                          ["<editor>"
                           $"The person is in the buffer {where.buffer} of oxmacs, an Emacs-like editor."
                           (when where.file $"It visits {where.file}.")
                           (when where.mode $"Its mode is {where.mode}.")
                           (when where.project $"The project is {where.project}.")
                           $"Point is on line {where.line}, column {where.column}."
                           "</editor>"])))

(def (root)
  (in-main (fn [] (or (project-root) (default-directory)))))

(def (resolved given)
  (let [expanded (path/expand given)]
    (if (path/absolute? expanded) expanded (path/join (root) expanded))))

(test "the editor context says where the person is"
  (expect (= (context-of {buffer: "main.rs" file: "/p/main.rs" mode: "Rust" project: "/p" line: 3 column: 1})
             "<editor>\nThe person is in the buffer main.rs of oxmacs, an Emacs-like editor.\nIt visits /p/main.rs.\nIts mode is Rust.\nThe project is /p.\nPoint is on line 3, column 1.\n</editor>"))
  (expect (= (context-of {buffer: "*scratch*" line: 1 column: 1})
             "<editor>\nThe person is in the buffer *scratch* of oxmacs, an Emacs-like editor.\nPoint is on line 1, column 1.\n</editor>")))
