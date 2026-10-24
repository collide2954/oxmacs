(module
  (import (ox file) (ox path) (ox string))
  (export text open))

(def text
  #r#"The oxmacs tutorial

This is your own copy of the tutorial: edit it as you go. oxmacs keeps
your place, so C-h t brings you back here. Delete tutorial.txt in your
oxmacs folder to start over.

Keys are written C-x for Control+x and M-x for Alt+x (Meta).
C-x C-s means C-x, then C-s. RET is Enter, SPC is space, DEL is
Backspace. C-g cancels whatever is in progress.


1. MOVING

  C-n  next line            C-p  previous line
  C-f  forward a character  C-b  back a character
  M-f  forward a word       M-b  back a word
  C-a  start of the line    C-e  end of the line
  C-v  next screen          M-v  previous screen
  M-<  top of the buffer    M->  end of the buffer

>> Use C-n to come down to this line, then C-e to reach its end.

The arrow keys, Home, End and the mouse work too.


2. EDITING

Type to insert text. DEL deletes the character before point and C-d
the one after it.

  C-k  kill to the end of the line   C-y  yank (paste) it back
  C-/  undo                          M-y  after C-y, an older kill

C-SPC sets the mark; move, and the text between is the region.
C-w kills the region, M-w copies it.

>> Delete the repeated words in this this this sentence.
>> Move to the next line, kill it with C-k, then yank it back twice.
Copy me.


3. FILES AND BUFFERS

  C-x C-f  open a file      C-x C-s  save
  C-x b    switch buffers   C-x k    close a buffer

>> Save this file now with C-x C-s.


4. WINDOWS

  C-x 2  split below        C-x 3  split beside
  C-x o  other window       C-x 1  only this window
  C-x 0  close this window

>> Press C-x 3, then C-x o twice, then C-x 1.


5. SEARCHING

C-s searches forward as you type and C-r backward. Press C-s again for
the next match, RET to stop there, or C-g to go back. M-% replaces,
asking at each match.

>> Search for the word quince with C-s.

Here is a quince.


6. COMMANDS AND HELP

M-x runs any command by name, with completion: type a few letters and
TAB. C-h f explains a command. C-h i opens the Ox manual.

>> Run M-x calendar, then press q to leave it.


7. OX

oxmacs is extended in Ox, a small Lisp. Try it:

>> Press C-x C-f, type ~/hello.ox and press RET.
>> Type (+ 1 2) and press C-x C-e. The result appears beside it.
>> Now type the lines below and press C-x C-e after the last ).

  (defcommand hello "Say hello"
    (message "Hello from Ox"))

>> Run M-x hello.

C-c C-z opens the Ox REPL, and C-c C-d shows the documentation of the
name at point.


8. YOUR CONFIGURATION

M-x find-init-file opens ~/.oxmacs/init.ox, which runs every time
oxmacs starts. Saving it applies it at once.

>> Add these lines to init.ox and save it:

  (settings {tab-width: 4})
  (bind-key "C-c h" 'hello)

M-x customize changes settings in a form instead.


9. WHERE NEXT

The Ox manual (C-h i) teaches the language, extending oxmacs and
writing plugins, and lists every function. It has examples you can
run with x.

You have finished the tutorial.
"#)

(def (folder)
  (or (oxmacs-home) (path/home)))

(def (place-file)
  (path/join (folder) "tutorial.place"))

(def (saved-line)
  (try
    (or (string/to-number (string/trim (await (file/read (place-file))))) 0)
    (catch error _e 0)))

(def (reading?)
  (string/ends-with? (or (buffer-file) "") "tutorial.txt"))

(def (open)
  "Opens the reader's own copy of the tutorial where they left it, making it the first time."
  (let [copy (path/join (folder) "tutorial.txt")
        fresh (not (await (file/exists? copy)))]
    (when fresh
      (await (file/make-dir (folder)))
      (await (file/write copy text)))
    (open-file copy)
    (goto-line (if fresh 0 (saved-line)) 0)
    (on-idle :tutorial 2
             (fn []
               (when (reading?)
                 (await (file/write (place-file) (str (current-line)))))))))
