(plugin irc
  :version "0.10.7"
  :requires {vault: "^0.3" customize: "^0.1"}
  :opt-in true
  :summary "An IRCv3 client in the style of ERC"
  :commands [irc irc-setup irc-next-active irc-search]
  :keys {"C-c C-SPC" irc-next-active}
  :settings {networks: {type: :list default: [] doc: "Networks to connect to, each a map of :name, :host, :port, :tls, :nick, :login (the vault entry holding the SASL account and password), :channels and :monitor (nicks to hear about when they come online or go offline)"}
             highlights: {type: :list default: [] doc: "Words besides your nick that count as mentions"}
             nick-width: {type: :integer default: 12 doc: "Width of the nick column"}
             notify: {type: :toggle default: true doc: "Desktop notifications for mentions and private messages while you are elsewhere"}
             logs: {type: :toggle default: true doc: "Keep plain-text logs under irc/logs in the oxmacs home folder"}}
  :entry "irc.ox")
