(plugin remote
  :version "0.4.0"
  :requires {vault: "^0.3" customize: "^0.1"}
  :summary "Browse and edit files on other machines over ssh, sftp and ftp"
  :commands [remote remote-add-host remote-disconnect]
  :settings {hosts: {type: :list default: [] doc: "Hosts besides those of ~/.ssh/config, each a map of :name, :protocol (:ssh, :sftp or :ftp), :address, :user, :port, :login (a vault entry, for ftp) and :tls, or a :shell command such as [\"docker\" \"exec\" \"-i\" \"box\" \"sh\"]"}}
  :entry "remote.ox")
