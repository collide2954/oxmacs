(plugin irc
  :version "0.9.4"
  :requires {vault: "^0.2" customize: "^0.1"}
  :opt-in true
  :summary "An IRCv3 client in the style of ERC"
  :commands {irc: "Connect to your IRC networks"
             irc-setup: "Add an IRC network to your settings"
             irc-next-active: "Go to the next IRC buffer with new messages"
             irc-search: "Search the IRC logs"}
  :keys {"C-c C-SPC" irc-next-active}
  :settings {networks: {type: :list default: [] doc: "Networks to connect to, each a map of :name, :host, :port, :tls, :nick, :login (the vault entry holding the SASL account and password), :channels and :monitor (nicks to hear about when they come online or go offline)"}
             highlights: {type: :list default: [] doc: "Words besides your nick that count as mentions"}
             nick-width: {type: :int default: 12 doc: "Width of the nick column"}
             notify: {type: :bool default: true doc: "Desktop notifications for mentions and private messages while you are elsewhere"}
             logs: {type: :bool default: true doc: "Keep plain-text logs under irc/logs in the oxmacs home folder"}}
  :entry "irc.ox")
