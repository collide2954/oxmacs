(module
  (import (ox json) (ox file) (ox path) (ox string) (ox time))
  (export save! load chats-of remove! stored restored))

(def (folder)
  (when-let [home (plugin-folder 'claude)]
    (path/join home "chats")))

(def (file-of id)
  (path/join (folder) (str id ".json")))

(def (stored chat)
  "chat as it is written: its log's kinds and faces as names."
  (assoc (select-keys chat [:id :title :project :created :updated :messages])
         :log (vec (map (fn [entry]
                          (merge entry {kind: (name entry.kind)}
                                 (if entry.face {face: (name entry.face)} {})))
                        chat.log))))

(def (restored written)
  (assoc written :log (vec (map (fn [entry]
                                  (merge entry {kind: (keyword entry.kind)}
                                         (if entry.face {face: (keyword entry.face)} {})))
                                (or written.log [])))))

(def (save! chat)
  "Writes chat to the plugin's folder once it has a message."
  (when (and (folder) (not (empty? chat.messages)))
    (file/make-dir (folder))
    (file/write (file-of chat.id) (json/write (stored (assoc chat :updated (time/now)))))))

(def (load id)
  (restored (json/read (file/read (file-of id)))))

(def (chats-of project)
  "The chats saved for project, or every chat when project is nil, newest first."
  (if (and (folder) (file/exists? (folder)))
    (->> (file/list (folder))
         (filter (fn [entry] (string/ends-with? entry.name ".json")))
         (keep (fn [entry] (try (json/read (file/read entry.path)) (catch error _e nil))))
         (filter (fn [chat] (or (nil? project) (= chat.project project))))
         (sort-by (fn [chat] (- (or chat.updated 0))))
         (map (fn [chat] (select-keys chat [:id :title :project :updated])))
         vec)
    []))

(def (remove! id)
  (delete-path (file-of id)))

(test "a chat's log keeps its kinds and faces through JSON"
  (let [chat {id: "1" title: "t" project: "/p" created: 5 messages: [{role: "user" content: "hi"}]
              log: [{kind: :you text: "hi"} {kind: :note text: "Stopped" face: :comment}]}]
    (expect (= (restored (json/read (json/write (stored chat))))
               (assoc chat :log [{kind: :you text: "hi"} {kind: :note text: "Stopped" face: :comment}])))))
