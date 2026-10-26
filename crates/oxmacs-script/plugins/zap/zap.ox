(def (zapped buffer start end failed)
  (when (buffer-live? buffer)
    (with-buffer buffer
      (flash start end
             :face (setting (if failed :error-face :face))
             :duration (setting :duration)))))

(on-evaluate :zap zapped)
