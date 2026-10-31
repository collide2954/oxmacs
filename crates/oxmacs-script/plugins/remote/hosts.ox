(module
  (import (ox file) (ox path) (ox string))
  (export all-hosts find-host ssh-hosts parse-ssh-config protocol-of add-hosts!))

(def (protocol-of host)
  "How to reach host: :ssh, :sftp or :ftp, or another protocol a plugin's hosts name, such as :wsl."
  (let [given (or host.protocol :ssh)]
    (keyword (string/lower (name given)))))

(def (wildcard? name)
  (or (string/includes? name "*") (string/includes? name "?") (string/starts-with? name "!")))

(def (parse-ssh-config text)
  "The hosts an ssh config names, leaving out patterns, each a map of :name and, when given, :address, :user and :port."
  (let [step (fn [[hosts current] line]
               (let [words (string/words (string/trim (first (string/split line "#" 2))))
                     key (string/lower (or (first words) ""))
                     value (second words)
                     set-field (fn [field] (vec (map (fn [host] (if (contains? (set current) host.name) (assoc host field value) host)) hosts)))]
                 (cond ((= key "host")
                        (let [names (vec (remove wildcard? (rest words)))]
                          [(into hosts (map (fn [found] {name: found protocol: :ssh from: "~/.ssh/config"}) names)) names]))
                   ((= key "match") [hosts []])
                   ((and value (= key "hostname")) [(set-field :address) current])
                   ((and value (= key "user")) [(set-field :user) current])
                   ((and value (= key "port")) [(set-field :port) current])
                   (else [hosts current]))))]
    (first (reduce step [[] []] (string/lines text)))))

(def (ssh-hosts)
  "The hosts named in ~/.ssh/config."
  (let [file (path/join (path/home) ".ssh" "config")]
    (if (await (file/exists? file))
      (parse-ssh-config (await (file/read file)))
      [])))

(defonce sources (atom []))

(def (add-hosts! source)
  "Lists the hosts source, a function of no arguments, gives in every remote command, after the saved ones and those of ~/.ssh/config. A host with a :folder opens that folder in place of browsing."
  (swap! sources conj source))

(def (all-hosts)
  "The saved hosts, then those of ~/.ssh/config and those other plugins add, leaving out names already listed."
  (let [saved (vec (map (fn [host] (assoc host :protocol (protocol-of host))) (setting :hosts)))
        known (set (map :name saved))
        more (concat (ssh-hosts) (mapcat (fn [source] (source)) @sources))]
    (vec (concat saved (remove (fn [host] (contains? known host.name)) more)))))

(def (find-host name)
  (or (find (fn [host] (= host.name name)) (all-hosts))
      {name: name protocol: :ssh}))

(test "ssh config hosts are read without their patterns"
  (expect (= (map (fn [host] [host.name host.address host.user host.port])
                  (parse-ssh-config "Host web web2\n  HostName 10.0.0.5\n  User ana\n  Port 2222\nHost *.internal\n  User ops\nHost git\n  HostName github.com # mine\n"))
             [["web" "10.0.0.5" "ana" "2222"] ["web2" "10.0.0.5" "ana" "2222"] ["git" "github.com" nil nil]])))
