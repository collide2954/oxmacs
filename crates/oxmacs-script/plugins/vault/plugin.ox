(plugin vault
  :version "0.3.1"
  :opt-in true
  :summary "An encrypted password vault with one-time codes and imports"
  :commands [vault vault-copy-password vault-generate vault-add vault-import vault-lock]
  :settings {lock-after: {type: :integer default: 10 doc: "Minutes the vault stays unlocked after its last use"}
             clear-clipboard: {type: :integer default: 30 doc: "Seconds before a copied password is wiped from the clipboard"}}
  :entry "vault.ox")
