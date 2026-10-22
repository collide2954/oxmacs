use crate::editor::{ConnectionId, Editor, Effect};
use crate::external::net::{Endpoint, Event};

#[derive(Debug, Clone)]
pub struct ConnectionSpec {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub events: fn(&mut Editor, ConnectionId, Event),
    pub visit: fn(&mut Editor, ConnectionId),
    pub stop: fn(&mut Editor, ConnectionId),
}

impl Editor {
    pub fn connect(&mut self, endpoint: Endpoint, spec: ConnectionSpec) -> ConnectionId {
        let id = ConnectionId(self.next_connection);
        self.next_connection += 1;
        self.track_connection(id, &spec);
        self.connections.push((id, spec));
        self.effects.push(Effect::Connect(id, endpoint));
        id
    }

    pub fn transmit(&mut self, id: ConnectionId, lines: Vec<String>) {
        if !lines.is_empty() {
            self.effects.push(Effect::Transmit(id, lines));
        }
    }

    pub fn disconnect(&mut self, id: ConnectionId) {
        self.effects.push(Effect::Disconnect(id));
    }

    pub fn connection_event(&mut self, id: ConnectionId, event: Event) {
        let Some(events) = self.spec(id).map(|spec| spec.events) else {
            return;
        };
        match &event {
            Event::Connected => self.connection_changed(id, true),
            Event::Closed(_) => self.connection_changed(id, false),
            Event::Line(_) => {}
        }
        events(self, id, event);
    }

    pub(in crate::editor) fn visit_connection(&mut self, id: ConnectionId) {
        if let Some(visit) = self.spec(id).map(|spec| spec.visit) {
            visit(self, id);
        }
    }

    pub(in crate::editor) fn stop_connection(&mut self, id: ConnectionId) {
        if let Some(stop) = self.spec(id).map(|spec| spec.stop) {
            stop(self, id);
        }
    }

    fn spec(&self, id: ConnectionId) -> Option<&ConnectionSpec> {
        self.connections
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, spec)| spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_support::{lines, press, type_text};

    #[derive(Debug, Default)]
    struct Log(Vec<String>);

    fn note(editor: &mut Editor, text: String) {
        if let Some(log) = editor.plugin_state_mut::<Log>() {
            log.0.push(text);
        }
    }

    fn spec() -> ConnectionSpec {
        ConnectionSpec {
            kind: "chat",
            name: "example".to_owned(),
            detail: "irc.example:6697".to_owned(),
            events: |editor, id, event| note(editor, format!("{} {event:?}", id.0)),
            visit: |editor, id| note(editor, format!("visit {}", id.0)),
            stop: |editor, id| note(editor, format!("stop {}", id.0)),
        }
    }

    fn endpoint() -> Endpoint {
        Endpoint {
            host: "irc.example".to_owned(),
            port: 6697,
            tls: true,
        }
    }

    fn logged(editor: &Editor) -> Vec<String> {
        editor
            .plugin_state::<Log>()
            .map(|log| log.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn connections_carry_lines_and_report_events_to_their_owner() {
        let mut editor = Editor::new();
        editor.set_plugin_state(Log::default());

        let id = editor.connect(endpoint(), spec());
        editor.transmit(id, vec!["NICK me".to_owned()]);
        editor.transmit(id, Vec::new());
        editor.disconnect(id);
        assert_eq!(
            editor.take_effects(),
            [
                Effect::Connect(id, endpoint()),
                Effect::Transmit(id, vec!["NICK me".to_owned()]),
                Effect::Disconnect(id),
            ]
        );

        editor.connection_event(id, Event::Connected);
        editor.connection_event(id, Event::Line("PING :x".to_owned()));
        editor.connection_event(ConnectionId(99), Event::Connected);

        assert_eq!(logged(&editor), ["0 Connected", "0 Line(\"PING :x\")"]);
    }

    #[test]
    fn connections_show_in_processes_where_they_can_be_visited_and_stopped() {
        let mut editor = Editor::new();
        editor.set_plugin_state(Log::default());
        let id = editor.connect(endpoint(), spec());
        editor.connection_event(id, Event::Connected);
        press(&mut editor, "M-x");
        type_text(&mut editor, "list-processes");
        press(&mut editor, "RET");

        let row = lines(&editor)[4].clone();
        for part in ["example", "chat", "connected", "irc.example:6697"] {
            assert!(row.contains(part), "{part} in {row}");
        }

        press(&mut editor, "M-< C-n C-n C-n C-n RET");
        press(&mut editor, "k y");
        editor.connection_event(id, Event::Closed(None));

        assert_eq!(
            logged(&editor)[1..],
            ["visit 0", "stop 0", "0 Closed(None)"]
        );
        assert!(lines(&editor)[4].contains("closed"), "{:?}", lines(&editor));
    }
}
