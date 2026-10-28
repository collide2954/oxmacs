(plugin profiler
  :version "0.1.3"
  :summary "Find where the Ox code in oxmacs spends its time"
  :commands {ox-profiler-start: "Start profiling the Ox code oxmacs runs"
             ox-profiler-stop: "Stop profiling and show where the time went"
             ox-profiler-report: "Show where the time went so far"}
  :entry "profiler.ox")
