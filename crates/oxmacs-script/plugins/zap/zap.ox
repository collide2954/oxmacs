(def (zapped buffer start end failed)
  (when (buffer-live? buffer)
    (with-buffer buffer
      (annotate start end
                :face (setting (if failed :error-face :face))
                :lasts (setting :duration)
                :key :zap))))

(on-evaluate :zap zapped)
