(plugin vault
  :version "0.2.4"
  :opt-in true
  :summary "An encrypted password vault with one-time codes and imports"
  :commands {vault: "List the entries of the password vault"
             vault-copy-password: "Copy the password of a vault entry"
             vault-generate: "Make a new password or passphrase and copy it"
             vault-add: "Add an entry to the password vault"
             vault-import: "Import passwords from KeePass, Bitwarden, 1Password or pass"
             vault-lock: "Lock the password vault now"}
  :settings {lock-after: {type: :int default: 10 doc: "Minutes the vault stays unlocked after its last use"}
             clear-clipboard: {type: :int default: 30 doc: "Seconds before a copied password is wiped from the clipboard"}}
  :entry "vault.ox")
