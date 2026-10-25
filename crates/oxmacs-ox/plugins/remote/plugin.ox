(plugin remote
  :version "0.1.7"
  :requires {vault: "^0.2" customize: "^0.1"}
  :summary "Browse and edit files on other machines over ssh, sftp and ftp"
  :commands {remote: "Browse the files of a host over ssh, sftp or ftp"
             remote-add-host: "Save a host to browse"}
  :settings {hosts: {type: :list default: [] doc: "Hosts besides those of ~/.ssh/config, each a map of :name, :protocol (:ssh, :sftp or :ftp), :address, :user, :port, :login (a vault entry, for ftp) and :tls, or a :shell command such as [\"docker\" \"exec\" \"-i\" \"box\" \"sh\"]"}}
  :entry "remote.ox")
