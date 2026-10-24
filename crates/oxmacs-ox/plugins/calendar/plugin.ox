(plugin calendar
  :version "0.2.0"
  :opt-in true
  :summary "Calendars in .ics files, with CalDAV sync, reminders and invitations"
  :commands {calendar: "Show the calendar's agenda"
             calendar-month: "Show a month of the calendar"
             calendar-week: "Show a week of the calendar as a grid of hours"
             calendar-add: "Add an event"
             calendar-import: "Add the events of an .ics file, such as an invitation"
             calendar-sync: "Sync the calendars that have a CalDAV address"}
  :settings {calendars: {type: :list default: [] doc: "Calendars, each a map of :name and, to sync with CalDAV, its :url and :login (a vault entry); one called Personal when empty"}
             days: {type: :int default: 14 doc: "Days the agenda shows"}
             reminder: {type: :int default: 10 doc: "Minutes before an event to remind you when it sets no reminder of its own"}
             reminders: {type: :bool default: true doc: "Remind you of events in the echo area and with desktop notifications, from the moment oxmacs starts"}
             email: {type: :string doc: "Your address, to answer invitations"}}
  :startup-when :reminders
  :entry "calendar.ox")
