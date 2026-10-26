(module
  (import (ox csv) (ox file) (ox keepass) (ox path) (ox process) (ox string))
  (export bitwarden one-password pass keepass))

(def (filled value)
  (when (and value (not (string/blank? value)))
    value))

(def (lowered row)
  (reduce (fn [all [key value]] (assoc all (string/lower key) (filled value)))
          {} (entries row)))

(def (bitwarden text)
  "The logins of a Bitwarden CSV export."
  (vec (keep (fn [row]
               (let [row (lowered row)]
                 (when (= (or (get row "type") "login") "login")
                   {title: (get row "name")
                    username: (get row "login_username")
                    password: (get row "login_password")
                    url: (get row "login_uri")
                    totp: (get row "login_totp")
                    notes: (get row "notes")})))
             (csv/read text :header true))))

(def (one-password text)
  "The items of a 1Password CSV export."
  (vec (map (fn [row]
              (let [row (lowered row)]
                {title: (get row "title")
                 username: (get row "username")
                 password: (get row "password")
                 url: (or (get row "url") (get row "website") (get row "login url"))
                 totp: (or (get row "otpauth") (get row "one-time password"))
                 notes: (or (get row "notes") (get row "notesplain"))}))
            (csv/read text :header true))))

(def (store-folder)
  (or (process/env "PASSWORD_STORE_DIR")
      (path/join (path/home) ".password-store")))

(def (gpg-files folder)
  (mapcat (fn [entry]
            (cond (entry.dir (if (string/starts-with? entry.name ".") [] (gpg-files entry.path)))
              ((string/ends-with? entry.name ".gpg") [entry.path])
              (else [])))
          (await (file/list folder))))

(def (labeled lines prefixes)
  (some (fn [line]
          (some (fn [prefix]
                  (when (string/starts-with? (string/lower line) prefix)
                    (filled (string/trim (string/slice line (count prefix))))))
                prefixes))
        lines))

(def (pass-entry root file)
  (let [done (await (process/run "gpg" ["--quiet" "--batch" "--decrypt" file]))]
    (when (= done.status 0)
      (let [lines (string/lines done.stdout)
            others (vec (rest lines))
            name (string/slice file (inc (count root)) (- (count file) 4))]
        {title: (string/replace name "\\" "/")
         username: (labeled others ["login:" "username:" "user:" "email:"])
         password: (first lines)
         url: (labeled others ["url:" "website:"])
         totp: (some (fn [line] (when (string/starts-with? line "otpauth://") line)) others)
         notes: (filled (string/join "\n" others))}))))

(def (pass)
  "The entries of the pass password store, decrypted with gpg."
  (let [root (store-folder)]
    (vec (keep (fn [file] (pass-entry root file)) (gpg-files root)))))

(def (keepass file password)
  "The entries of a KeePass database."
  (vec (map (fn [entry] (dissoc entry :group))
            (await (keepass/read file password)))))
