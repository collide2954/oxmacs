(def (zapped buffer start end failed)
  (when (and (setting :flash) (buffer-live? buffer))
    (with-buffer buffer
      (annotate start end
                :face (setting (if failed :error-face :face))
                :lasts (setting :duration)
                :key :zap))))

(on-evaluate :zap zapped)
