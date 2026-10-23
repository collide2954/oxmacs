(module
  (import (ox file) (ox path) (ox string) (ox http) (ox regex)
          (calendar store :as store)
          (vault store :as vault))
  (export sync! responses))

(def query
  "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<C:calendar-query xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\">
  <D:prop><D:getetag/><C:calendar-data/></D:prop>
  <C:filter><C:comp-filter name=\"VCALENDAR\"><C:comp-filter name=\"VEVENT\"/></C:comp-filter></C:filter>
</C:calendar-query>")

(def (xml-text text)
  (let [inner (if-let [found (regex/find "(?s)<!\\[CDATA\\[(.*)\\]\\]>" text)] (first found.groups) text)]
    (-> inner
        (string/replace "&lt;" "<")
        (string/replace "&gt;" ">")
        (string/replace "&quot;" "\"")
        (string/replace "&apos;" "'")
        (string/replace "&#13;" "\r")
        (string/replace "&#xD;" "\r")
        (string/replace "&#xd;" "\r")
        (string/replace "&amp;" "&"))))

(def (tag-text block tag)
  (when-let [found (regex/find (str "(?s)<(?:[A-Za-z0-9_-]+:)?" tag "(?:\\s[^>]*)?>(.*?)</(?:[A-Za-z0-9_-]+:)?" tag ">") block)]
    (xml-text (string/trim (first found.groups)))))

(def (responses body)
  "The responses of a CalDAV multistatus body, each a map of :href, :etag and :data."
  (vec (keep (fn [match]
               (let [block match.text]
                 (when-let [href (tag-text block "href")]
                   {href: href etag: (tag-text block "getetag") data: (tag-text block "calendar-data")})))
             (regex/find-all "(?s)<(?:[A-Za-z0-9_-]+:)?response\\b.*?</(?:[A-Za-z0-9_-]+:)?response>" body))))

(def (origin url)
  (or (get (regex/find "^[a-zA-Z]+://[^/]+" url) :text) ""))

(def (absolute url href)
  (cond ((string/starts-with? href "http") href)
    ((string/starts-with? href "/") (str (origin url) href))
    (else (str url href))))

(def (folder-url url)
  (if (string/ends-with? url "/") url (str url "/")))

(def (file-name href)
  (let [name (store/safe (path/name (string/replace href "%40" "@")))]
    (if (string/ends-with? (string/lower name) ".ics") name (str name ".ics"))))

(def (header response name)
  (or (get response.headers name) (get response.headers (string/lower name))))

(def (login calendar)
  (if-let [title calendar.login]
    (do (vault/unlock)
      (if-let [entry (vault/find-entry title)]
        {user: entry.username password: entry.password}
        (raise (error (str "The vault has no entry called " title)))))
    {user: nil password: nil}))

(def (request credentials method url {headers {} body nil})
  (await (http/request method url :headers headers :body body :user credentials.user :password credentials.password)))

(def (resource sync href)
  (find (fn [known] (= known.href href)) sync.resources))

(def (href-of sync file)
  (some (fn [known] (when (= known.file file) known.href)) sync.resources))

(def (forget sync href)
  (assoc sync :resources (vec (remove (fn [known] (= known.href href)) sync.resources))))

(def (remember sync href etag file)
  (update (forget sync href) :resources conj {href: href etag: etag file: file}))

(def (push! calendar credentials sync)
  (let [base (folder-url calendar.url)
        place (store/calendar-folder calendar.name)
        step (fn [[sync problems] change]
               (let [href (or (href-of sync change.file) (str (string/replace base (origin base) "") change.file))
                     known (resource sync href)
                     url (absolute base href)]
                 (if change.removed
                   (let [response (request credentials :delete url :headers (if known.etag {"If-Match" known.etag} {}))]
                     (if (or (< response.status 300) (= response.status 404))
                       [(forget sync href) problems]
                       [sync (conj problems (str "couldn't delete " change.file " (" response.status ")"))]))
                   (let [body (await (file/read (path/join place change.file)))
                         response (request credentials :put url
                                           :headers (merge {"Content-Type" "text/calendar; charset=utf-8"}
                                                           (if known.etag {"If-Match" known.etag} {"If-None-Match" "*"}))
                                           :body body)]
                     (cond ((< response.status 300)
                            [(remember sync href (header response "ETag") change.file) problems])
                       ((= response.status 412)
                        [sync (conj problems (str change.file " changed on the server too; kept the server's copy"))])
                       (else [sync (conj problems (str "couldn't send " change.file " (" response.status ")"))]))))))]
    (reduce step [(assoc sync :pending []) []] sync.pending)))

(def (pull! calendar credentials sync)
  (let [response (request credentials :report (folder-url calendar.url)
                          :headers {"Depth" "1" "Content-Type" "application/xml; charset=utf-8"}
                          :body query)
        place (store/calendar-folder calendar.name)]
    (when (>= response.status 300)
      (raise (error (str calendar.name " answered " response.status " to the calendar query"))))
    (let [found (vec (filter :data (responses response.body)))
          seen (set (map :href found))
          changed (reduce (fn [sync item]
                            (let [known (resource sync item.href)]
                              (if (and known (= known.etag item.etag) item.etag)
                                sync
                                (let [name (or (get known :file) (file-name item.href))]
                                  (await (file/write (path/join place name) item.data))
                                  (remember sync item.href item.etag name)))))
                          sync found)
          gone (vec (remove (fn [known] (contains? seen known.href)) changed.resources))]
      (reduce (fn [sync known]
                (let [file (path/join place known.file)]
                  (when (await (file/exists? file))
                    (await (file/trash file)))
                  (forget sync known.href)))
              changed gone))))

(def (sync! calendar)
  "Sends this calendar's local changes to its CalDAV server and brings in the server's, returning the problems met."
  (let [credentials (login calendar)
        place (store/calendar-folder calendar.name)]
    (await (file/make-dir place))
    (let [[pushed problems] (push! calendar credentials (store/read-sync calendar.name))
          pulled (pull! calendar credentials pushed)]
      (store/write-sync! calendar.name pulled)
      problems)))
