(module
  (import (ox crypto) (ox file) (ox string) (ox time)
          (vault store :as store)
          (vault import :as import)))

(def list-buffer "*Vault*")
(def entry-buffer "*Vault entry*")

(defonce view (atom {shown: nil reveal: false copies: 0 ticks: 0}))

(def list-keys
  {"RET" 'vault-show "c" 'vault-copy-password "u" 'vault-copy-username "t" 'vault-copy-code
   "a" 'vault-add "e" 'vault-edit "d" 'vault-delete "g" 'vault-refresh
   "n" 'vault-next "p" 'vault-previous "L" 'vault-lock "q" 'quit-window})

(def entry-keys
  {"c" 'vault-copy-password "u" 'vault-copy-username "t" 'vault-copy-code "s" 'vault-reveal
   "e" 'vault-edit "d" 'vault-delete "L" 'vault-lock "q" 'quit-window})

(def (quietly work)
  (try (work)
    (catch error e
      (unless (= e.message "Quit")
        (message e.message)))))

(def (text value)
  (or value ""))

(def (blank? value)
  (or (nil? value) (string/blank? value)))

(def (sorted found)
  (sort-by (fn [entry] (string/lower (text entry.title))) found))

(def (by-id id)
  (find (fn [entry] (= entry.id id)) (store/entries)))

(def (row entry)
  [(string/pad-end (text entry.title) 28)
   " "
   [(string/pad-end (text entry.username) 24) :string]
   " "
   [(text entry.url) :comment]])

(def (list-page)
  (let [found (sorted (store/entries))]
    {header: [[["Vault" :heading] "  " [(str (count found) " entries") :comment]]
              [["RET shows  c copies the password  u the user  t the code  a adds  e edits  d deletes  L locks  q quits" :comment]]]
     lines: (if (empty? found)
              [[["No entries yet: a adds one and M-x vault-import brings in others" :comment]]]
              (vec (map (fn [entry] {line: (row entry) id: entry.id}) found)))}))

(def (show-list)
  (let [page (list-page)]
    (open-buffer list-buffer page.lines :keys list-keys :line 0)
    (set-header list-buffer page.header)))

(def (redraw-list)
  (when (and (buffer-exists? list-buffer) (not (store/locked?)))
    (let [page (list-page)]
      (set-lines list-buffer page.lines)
      (set-header list-buffer page.header))))

(def (otp entry)
  (let [given (string/trim (text entry.totp))]
    (if (string/starts-with? given "otpauth://")
      (let [query (or (second (string/split given "?" 2)) "")
            found (reduce (fn [all part]
                            (let [[key value] (string/split part "=" 2)]
                              (assoc all key value)))
                          {} (string/split query "&"))]
        {secret: (get found "secret")
         step: (or (string/to-number (text (get found "period"))) 30)
         digits: (or (string/to-number (text (get found "digits"))) 6)})
      {secret: given step: 30 digits: 6})))

(def (code entry)
  (let [settings (otp entry)]
    (crypto/totp settings.secret :step settings.step :digits settings.digits)))

(def (seconds-left entry)
  (let [step (get (otp entry) :step)]
    (- step (mod (// (time/now) 1000) step))))

(def (field label value)
  [[(string/pad-end label 11) :comment] value])

(def (entry-header entry)
  [[[(text entry.title) :heading]]
   [["c copies the password  u the user  t the code  s shows the password  e edits  q quits" :comment]]])

(def (entry-lines entry)
  (let [reveal (get @view :reveal)
        password (text entry.password)]
    (vec (filter some?
                 (concat [(field "User" [(text entry.username) :string])
                          (field "Password" (if reveal password (string/repeat "•" (min 16 (count password)))))
                          (when (not (blank? entry.url))
                            (field "URL" [entry.url :link]))
                          (when (not (blank? entry.totp))
                            (field "Code" [(str (code entry) "   " (seconds-left entry) " s left") :keyword]))]
                         (if (blank? entry.notes)
                           []
                           (concat [""] (string/lines entry.notes))))))))

(def (redraw-entry)
  (when-let [entry (by-id (get @view :shown))]
    (when (buffer-exists? entry-buffer)
      (set-lines entry-buffer (entry-lines entry))
      (set-header entry-buffer (entry-header entry)))))

(def (show-entry entry)
  (let [ticks (inc (get @view :ticks))]
    (swap! view assoc :shown entry.id :reveal false :ticks ticks)
    (store/touch!)
    (open-buffer entry-buffer (entry-lines entry) :keys entry-keys)
    (set-header entry-buffer (entry-header entry))
    (when (not (blank? entry.totp))
      (every 1000
             (fn []
               (if (and (= (get @view :ticks) ticks) (buffer-exists? entry-buffer) (not (store/locked?)))
                 (do (redraw-entry) nil)
                 :stop))))))

(def (entry-here)
  (cond ((store/locked?) nil)
    ((= (buffer-name) list-buffer)
     (when-let [id (get (line-item) :id)]
       (by-id id)))
    ((= (buffer-name) entry-buffer) (by-id (get @view :shown)))
    (else nil)))

(def (choose question)
  (let [found (store/unlock)]
    (when (empty? found)
      (raise (error "The vault has no entries yet")))
    (let [title (await (prompt question :choices (vec (map :title (sorted found)))))]
      (or (store/find-entry title)
          (raise (error "No entry is called {title}"))))))

(def (target question)
  (or (entry-here) (choose question)))

(def (clear-after)
  (or (setting :clear-clipboard) 30))

(def (copy! value what secret?)
  (when (blank? value)
    (raise (error "This entry has no {what}")))
  (clipboard-set value)
  (store/touch!)
  (if secret?
    (let [mine (inc (get @view :copies))]
      (swap! view assoc :copies mine)
      (message "Copied the {what}; the clipboard clears in {(clear-after)} seconds")
      (spawn (sleep (time/seconds (clear-after)))
        (when (= (get @view :copies) mine)
          (clipboard-set "")
          (message "Cleared the clipboard"))))
    (message "Copied the {what}")))

(def (ask question {initial ""})
  (await (prompt question :initial initial)))

(def (ask-password question)
  (let [typed (await (prompt question :secret true))]
    (if (string/blank? typed) (store/generate) typed)))

(defcommand vault "List the entries of the password vault"
  (quietly (fn []
             (store/unlock)
             (show-list))))

(defcommand vault-show "Show the vault entry on this line"
  (if-let [entry (entry-here)]
    (show-entry entry)
    (message "No entry on this line")))

(defcommand vault-copy-password "Copy the password of a vault entry"
  (quietly (fn []
             (let [entry (target "Copy the password of: ")]
               (copy! entry.password "password" true)))))

(defcommand vault-copy-username "Copy the user name of a vault entry"
  (quietly (fn []
             (let [entry (target "Copy the user name of: ")]
               (copy! entry.username "user name" false)))))

(defcommand vault-copy-code "Copy the one-time code of a vault entry"
  (quietly (fn []
             (let [entry (target "Copy the code of: ")]
               (when (blank? entry.totp)
                 (raise (error "{entry.title} has no one-time code")))
               (copy! (code entry) "code" true)))))

(defcommand vault-reveal "Show or hide the password of the entry"
  (swap! view update :reveal not)
  (redraw-entry))

(defcommand vault-refresh "Draw the vault list again" (redraw-list))

(defcommand vault-next "Move to the next entry" (next-item))
(defcommand vault-previous "Move to the previous entry" (next-item -1))

(defcommand vault-add "Add an entry to the password vault"
  (quietly (fn []
             (store/unlock)
             (let [title (string/trim (ask "Title: "))]
               (when (string/blank? title)
                 (raise (error "An entry needs a title")))
               (store/add! {title: title
                            username: (ask "User name: ")
                            password: (ask-password "Password (empty makes one): ")
                            url: (ask "URL: ")
                            totp: (ask "One-time code secret or otpauth link (optional): ")
                            notes: ""})
               (redraw-list)
               (message "Added {title}")))))

(defcommand vault-edit "Change a field of a vault entry"
  (quietly (fn []
             (let [entry (target "Edit: ")
                   field (await (prompt "Change: " :choices ["title" "username" "password" "url" "totp" "notes"]))
                   key (keyword field)
                   value (if (= field "password")
                           (ask-password "New password (empty makes one): ")
                           (ask "{field}: " :initial (text (get entry key))))]
               (store/change! entry.id (fn [old] (assoc old key value)))
               (redraw-list)
               (redraw-entry)
               (message "Changed the {field} of {entry.title}")))))

(defcommand vault-delete "Delete a vault entry"
  (quietly (fn []
             (let [entry (target "Delete: ")]
               (when (await (yes-or-no "Delete {entry.title} from the vault? "))
                 (store/remove! entry.id)
                 (when (= (get @view :shown) entry.id)
                   (close-buffer entry-buffer))
                 (redraw-list)
                 (message "Deleted {entry.title}"))))))

(defcommand vault-lock "Lock the password vault now"
  (store/lock)
  (message "Vault locked"))

(defcommand vault-generate "Make a new password or passphrase and copy it"
  (quietly (fn []
             (let [kind (await (prompt "Make: " :choices {"password" "20 letters, digits and symbols"
                                                          "passphrase" "7 random words"
                                                          "pin" "6 digits"}))
                   made (cond ((= kind "passphrase") (store/passphrase))
                          ((= kind "pin") (apply str (map (fn [_] (crypto/random-below 10)) (range 6))))
                          (else (store/generate)))]
               (copy! made "new {kind}" true)))))

(def (imported kind)
  (cond ((= kind "keepass")
         (let [file (await (prompt-path "KeePass database: "))]
           (import/keepass file (await (prompt "Its password: " :secret true)))))
    ((= kind "bitwarden") (import/bitwarden (await (file/read (await (prompt-path "Bitwarden CSV: "))))))
    ((= kind "1password") (import/one-password (await (file/read (await (prompt-path "1Password CSV: "))))))
    ((= kind "pass") (import/pass))
    (else (raise (error "oxmacs can't import from {kind}")))))

(defcommand vault-import "Import passwords from KeePass, Bitwarden, 1Password or pass"
  (quietly (fn []
             (store/unlock)
             (let [kind (await (prompt "Import from: " :choices {"keepass" "a KeePass .kdbx file"
                                                                 "bitwarden" "a Bitwarden CSV export"
                                                                 "1password" "a 1Password CSV export"
                                                                 "pass" "the pass password store"}))
                   found (imported kind)
                   added (store/add-all! found)]
               (redraw-list)
               (message "Imported {added} of {(count found)} entries; the others were already there")))))
