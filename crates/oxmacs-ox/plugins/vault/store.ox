(module
  (import (ox crypto) (ox json) (ox file) (ox path) (ox string) (ox time)
          (vault words :as words))
  (export vault-file locked? unlock lock entries find-entry save! add! add-all! change! remove!
          secret touch! generate passphrase))

(def file-version 1)
(def default-cost {memory: 65536 iterations: 3 parallelism: 1})
(def letters "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ")
(def digits "23456789")
(def symbols "!#$%&*+-=?@^_~")

(defonce state (atom {key: nil entries: nil header: nil used: 0 watching: false}))

(def (vault-file)
  "Where the vault lives: vault.json in the oxmacs home folder."
  (when-let [home (oxmacs-home)]
    (path/join home "vault.json")))

(def (minutes)
  (or (plugin-setting 'vault :lock-after) 10))

(def (locked?)
  "True until the vault's password is given in this session."
  (nil? (get @state :key)))

(def (touch!)
  "Counts now as the last time the vault was used, which keeps it unlocked."
  (swap! state assoc :used (time/now)))

(def (lock)
  "Forgets the vault's key and entries until the password is given again, and closes the vault's buffers."
  (swap! state assoc :key nil :entries nil :header nil)
  (close-buffer "*Vault*")
  (close-buffer "*Vault entry*")
  nil)

(def (expired?)
  (and (not (locked?))
       (> (- (time/now) (get @state :used)) (time/minutes (minutes)))))

(def (watch)
  (unless (get @state :watching)
    (swap! state assoc :watching true)
    (every (time/seconds 20)
           (fn []
             (when (expired?)
               (lock)
               (message "Vault locked after {(minutes)} minutes unused"))))))

(def (sealed-file key header entries)
  (json/write (assoc header :sealed (crypto/seal key (json/write {entries: entries})))))

(def (write! key header entries)
  (let [file (vault-file)
        fresh (str file ".new")]
    (await (file/make-dir (path/parent file)))
    (await (file/write fresh (sealed-file key header entries)))
    (when (await (file/exists? file))
      (await (file/copy file (str file ".bak"))))
    (await (file/rename fresh file))))

(def (opened key header)
  (swap! state assoc :key key :header header :used (time/now))
  (watch)
  (get @state :entries))

(def (create)
  (let [password (await (prompt "New vault password: " :secret true))
        again (await (prompt "Same password again: " :secret true))]
    (cond ((string/blank? password) (raise (error "The vault needs a password")))
      ((not= password again) (raise (error "The passwords differ; nothing was made")))
      (else
       (message "Making the vault key…")
       (let [header (merge {version: file-version salt: (crypto/random 16)} default-cost)
             key (await (crypto/derive-key password header.salt
                                           :memory header.memory :iterations header.iterations
                                           :parallelism header.parallelism))]
         (write! key header [])
         (swap! state assoc :entries [])
         (opened key header))))))

(def (open-existing file)
  (let [stored (json/read (await (file/read file)))
        password (await (prompt "Vault password: " :secret true))
        key (await (crypto/derive-key password stored.salt
                                      :memory stored.memory :iterations stored.iterations
                                      :parallelism stored.parallelism))
        inside (try (crypto/open key stored.sealed)
                 (catch error _ (raise (error "Wrong vault password"))))]
    (swap! state assoc :entries (vec (get (json/read inside) :entries)))
    (opened key (dissoc stored :sealed))))

(def (unlock)
  "The vault's entries, asking for its password first when it is locked, or making a new vault."
  (if (locked?)
    (let [file (or (vault-file) (raise (error "There is no oxmacs home folder for the vault")))]
      (if (await (file/exists? file))
        (open-existing file)
        (create)))
    (do (touch!)
      (get @state :entries))))

(def (entries)
  "The entries of the unlocked vault, or nil when it is locked."
  (get @state :entries))

(def (find-entry title)
  (find (fn [entry] (= entry.title title)) (entries)))

(def (save! changed)
  "Replaces the entries with changed and writes the vault."
  (let [now @state]
    (when (nil? now.key)
      (raise (error "The vault is locked")))
    (write! now.key now.header changed)
    (swap! state assoc :entries changed :used (time/now))
    changed))

(def (stamped entry)
  (assoc entry :id (crypto/random 8) :created (time/now) :modified (time/now)))

(def (add! entry)
  (let [made (stamped entry)]
    (save! (conj (vec (entries)) made))
    made))

(def (add-all! found)
  "Adds the entries that aren't in the vault yet, judged by title and user name, and says how many were new."
  (let [known (set (map (fn [entry] [entry.title entry.username]) (entries)))
        fresh (vec (filter (fn [entry] (not (contains? known [entry.title entry.username]))) found))]
    (save! (vec (concat (entries) (map stamped fresh))))
    (count fresh)))

(def (change! id edit)
  (save! (vec (map (fn [entry] (if (= entry.id id) (assoc (edit entry) :modified (time/now)) entry))
                   (entries)))))

(def (remove! id)
  (save! (vec (remove (fn [entry] (= entry.id id)) (entries)))))

(def (secret title)
  "The password of the entry titled title, asking for the vault's password first when it is locked; nil when there is no such entry. Other plugins use it for their logins."
  (unlock)
  (when-let [entry (find-entry title)]
    (touch!)
    entry.password))

(def (pick choices wanted)
  (let [size (count choices)]
    (vec (map (fn [_] (nth choices (crypto/random-below size))) (range wanted)))))

(def (generate {length 20 symbols? true})
  "A random password of length characters from letters, digits and, unless symbols? is false, symbols."
  (apply str (pick (str letters digits (if symbols? symbols "")) length)))

(def (passphrase {word-count 7 separator "-"})
  "A passphrase of word-count random words joined by separator."
  (string/join separator (pick words/all word-count)))
