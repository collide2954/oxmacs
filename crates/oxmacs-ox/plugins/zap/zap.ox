(def face :region)

(def error-face :removed)

(def (zapped buffer start end failed)
  (when (buffer-live? buffer)
    (with-buffer buffer
      (flash start end :face (if failed error-face face) :duration (setting :duration)))))

(on-evaluate :zap zapped)
