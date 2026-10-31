(plugin claude
  :version "0.1.0"
  :opt-in true
  :summary "Claude beside your code: a chat that can read and drive the editor, questions about a region, and edits reviewed as diffs"
  :requires {vault: "^0.3" git: "^0.5"}
  :commands [claude claude-send claude-stop claude-new claude-ask claude-chats claude-open-chat claude-delete-chat
             claude-set-key claude-accept-hunk claude-reject-hunk claude-accept-all claude-reject-all
             claude-finish-review]
  :settings {model: {type: :text default: "claude-opus-5-5" doc: "The Claude model the chat talks to"}
             width: {type: :integer default: 72 doc: "How many columns wide the chat's side window is"}
             effort: {type: [:low :medium :high :xhigh :max] default: :high doc: "How hard Claude thinks before it answers in the chat; higher is slower and costs more"}
             max-tokens: {type: :integer default: 64000 doc: "The most tokens one reply may take"}
             suggestions: {type: :toggle default: true doc: "Show Claude's suggestions in grey as you type in code, taken with TAB"}
             suggestion-model: {type: :text default: "claude-haiku-5-5" doc: "The Claude model that suggests code as you type"}
             suggestion-delay: {type: :integer default: 800 doc: "Milliseconds of quiet typing before a suggestion is asked for"}
             quiet-modes: {type: :list default: [] doc: "Modes where suggestions stay off, by name, such as Markdown"}
             key-entry: {type: :text default: "Anthropic API key" doc: "The vault entry that holds your Anthropic API key"}
             api-url: {type: :text default: "https://api.anthropic.com" doc: "Where the Anthropic API is"}}
  :entry "claude.ox")
