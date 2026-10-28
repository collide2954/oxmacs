(plugin calendar
  :version "0.2.9"
  :requires {vault: "^0.3"}
  :opt-in true
  :summary "Calendars in .ics files, with CalDAV sync, reminders and invitations"
  :commands [calendar calendar-month calendar-week calendar-add calendar-import calendar-sync]
  :settings {calendars: {type: :list default: [] doc: "Calendars, each a map of :name and, to sync with CalDAV, its :url and :login (a vault entry); one called Personal when empty"}
             days: {type: :integer default: 14 doc: "Days the agenda shows"}
             reminder: {type: :integer default: 10 doc: "Minutes before an event to remind you when it sets no reminder of its own"}
             reminders: {type: :toggle default: true doc: "Remind you of events in the echo area and with desktop notifications, from the moment oxmacs starts"}
             email: {type: :text doc: "Your address, to answer invitations"}}
  :startup-when :reminders
  :entry "calendar.ox")
