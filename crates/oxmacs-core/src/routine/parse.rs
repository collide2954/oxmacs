use std::ops::Range;

use super::tokens::{Found, Spanned, Token, tokenize, valid_name, word_template};
use super::{Action, Answer, Binding, Event, Piece, Problem, Routine, Step, Template, Trigger};

const HEADERS: &[&str] = &["summary", "key", "favorite", "on", "every"];

struct Block {
    owner: Option<usize>,
    line: usize,
    times: Option<Template>,
    body: Option<usize>,
    steps: Vec<Step>,
}

struct Reader<'a> {
    number: usize,
    tokens: &'a [Spanned],
    at: usize,
    problems: &'a mut Vec<Problem>,
}

impl Reader<'_> {
    fn problem(&mut self, columns: Range<usize>, message: impl Into<String>) {
        self.problems.push(Problem {
            line: self.number,
            columns,
            message: message.into(),
        });
    }

    fn missing(&mut self, message: &str) {
        let end = self.tokens.last().map_or(0, |last| last.columns.end);
        self.problem(end..end, message);
    }

    fn report(&mut self, found: Found) {
        for (columns, message) in found {
            self.problem(columns, message);
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|spanned| &spanned.token)
    }

    fn word(&mut self) -> Option<(String, Range<usize>)> {
        let tokens = self.tokens;
        let Some(Spanned {
            token: Token::Word(word),
            columns,
        }) = tokens.get(self.at)
        else {
            return None;
        };
        self.at += 1;
        Some((word.clone(), columns.clone()))
    }

    fn eat(&mut self, expected: &str) -> bool {
        let found = matches!(self.peek(), Some(Token::Word(word)) if word == expected);
        if found {
            self.at += 1;
        }
        found
    }

    fn text(&mut self, what: &str) -> Option<Template> {
        let tokens = self.tokens;
        let Some(spanned) = tokens.get(self.at) else {
            self.missing(&format!("Add {what} in quotes"));
            return None;
        };
        self.at += 1;
        match &spanned.token {
            Token::Quoted(template) => Some(template.clone()),
            Token::Word(_) => {
                self.problem(spanned.columns.clone(), format!("Put {what} in quotes"));
                None
            }
        }
    }

    fn finish(&mut self) {
        if let Some(extra) = self.tokens.get(self.at) {
            let columns = extra.columns.clone();
            self.problem(columns, "Nothing more belongs on this line");
            self.at = self.tokens.len();
        }
    }
}

fn run(reader: &mut Reader<'_>) -> Option<Action> {
    let Some((command, command_columns)) = reader.word() else {
        reader.missing("Name the command to run");
        return None;
    };
    let input = if reader.eat("input") {
        reader.text("the input")
    } else {
        None
    };
    Some(Action::Run {
        command,
        command_columns,
        input,
    })
}

fn keys(reader: &mut Reader<'_>) -> Option<Action> {
    if matches!(reader.peek(), Some(Token::Quoted(_))) {
        return reader.text("the keys").map(Action::Keys);
    }
    let rest = &reader.tokens[reader.at..];
    let (Some(first), Some(last)) = (rest.first(), rest.last()) else {
        reader.missing("Add the keys to press");
        return None;
    };
    let columns = first.columns.start..last.columns.end;
    let mut words = Vec::new();
    for spanned in rest {
        match &spanned.token {
            Token::Word(word) => words.push(word.clone()),
            Token::Quoted(_) => reader.problem(
                spanned.columns.clone(),
                "Write the keys either bare or as one text in quotes",
            ),
        }
    }
    reader.at = reader.tokens.len();
    Some(Action::Keys(Template {
        pieces: vec![Piece::Text(words.join(" "))],
        columns,
    }))
}

fn choices(reader: &mut Reader<'_>) -> Vec<Template> {
    let mut choices = Vec::new();
    while let Some(Token::Quoted(choice)) = reader.peek() {
        choices.push(choice.clone());
        reader.at += 1;
    }
    if choices.is_empty() {
        reader.missing("List the choices in quotes after from");
    }
    choices
}

fn ask(reader: &mut Reader<'_>) -> Option<Action> {
    let Some((name, columns)) = reader.word() else {
        reader.missing("Name the value to ask for");
        return None;
    };
    if !valid_name(&name) {
        reader.problem(columns, format!("{name} is not a value name"));
    }
    let prompt = reader.text("the question")?;
    let answer = if reader.eat("default") {
        Answer::Text {
            default: reader.text("the default"),
        }
    } else if reader.eat("from") {
        Answer::Pick(choices(reader))
    } else if reader.eat("yes-no") {
        Answer::YesNo
    } else {
        Answer::Text { default: None }
    };
    Some(Action::Ask {
        name,
        prompt,
        answer,
    })
}

fn search(reader: &mut Reader<'_>) -> Option<Action> {
    let pattern = reader.text("what to search for")?;
    let (mut backward, mut regexp) = (false, false);
    loop {
        if reader.eat("backward") {
            backward = true;
        } else if reader.eat("regexp") {
            regexp = true;
        } else {
            break;
        }
    }
    Some(Action::Search {
        pattern,
        backward,
        regexp,
    })
}

fn place(reader: &mut Reader<'_>) -> bool {
    const PLACES: &str = "Say in region or in buffer";
    match reader.word() {
        Some((place, _)) if place == "region" => true,
        Some((place, _)) if place == "buffer" => false,
        Some((_, columns)) => {
            reader.problem(columns, PLACES);
            false
        }
        None => {
            reader.missing(PLACES);
            false
        }
    }
}

fn replace(reader: &mut Reader<'_>) -> Option<Action> {
    let pattern = reader.text("what to replace")?;
    if !reader.eat("with") {
        reader.missing("Add with \"…\" after what to replace");
        return None;
    }
    let with = reader.text("the replacement")?;
    let (mut regexp, mut region) = (false, false);
    loop {
        if reader.eat("regexp") {
            regexp = true;
        } else if reader.eat("in") {
            region = place(reader);
        } else {
            break;
        }
    }
    Some(Action::Replace {
        pattern,
        with,
        regexp,
        region,
    })
}

fn step(reader: &mut Reader<'_>, keyword: &str, keyword_columns: Range<usize>) -> Option<Action> {
    let action = match keyword {
        "run" => run(reader)?,
        "insert" => Action::Insert(reader.text("the text to insert")?),
        "message" => Action::Message(reader.text("the message")?),
        "confirm" => Action::Confirm(reader.text("the question")?),
        "keys" => keys(reader)?,
        "ask" => ask(reader)?,
        "search" => search(reader)?,
        "replace" => replace(reader)?,
        "shell" => Action::Shell {
            command: reader.text("the command")?,
            insert: reader.eat("insert"),
        },
        "filter" => Action::Filter(reader.text("the command")?),
        _ => {
            reader.problem(keyword_columns, format!("{keyword} is not a step"));
            return None;
        }
    };
    reader.finish();
    Some(action)
}

fn summary(reader: &mut Reader<'_>, routine: &mut Routine, rest: &[char]) {
    let summary: String = rest.iter().collect();
    let summary = summary.trim();
    if summary.is_empty() {
        reader.missing("Write the summary after summary");
    } else {
        routine.summary = Some(summary.to_owned());
    }
    reader.at = reader.tokens.len();
}

fn language(rest: &mut Vec<Spanned>) -> Option<(String, Range<usize>)> {
    let named = rest.len() >= 3
        && matches!(&rest[rest.len() - 2].token, Token::Word(word) if word == "in")
        && matches!(&rest[rest.len() - 1].token, Token::Word(_));
    if !named {
        return None;
    }
    let Spanned {
        token: Token::Word(name),
        columns,
    } = rest.pop()?
    else {
        return None;
    };
    rest.pop();
    Some((name, columns))
}

fn key(reader: &mut Reader<'_>, routine: &mut Routine) {
    let mut rest = reader.tokens[reader.at..].to_vec();
    reader.at = reader.tokens.len();
    let language = language(&mut rest);
    let (Some(first), Some(last)) = (rest.first(), rest.last()) else {
        reader.missing("Write the keys after key");
        return;
    };
    let mut keys = Vec::new();
    for spanned in &rest {
        match &spanned.token {
            Token::Word(word) => keys.push(word.clone()),
            Token::Quoted(template) => match template.literal() {
                Some(literal) => keys.push(literal),
                None => reader.problem(spanned.columns.clone(), "Keys can't hold {values}"),
            },
        }
    }
    routine.bindings.push(Binding {
        line: reader.number,
        keys: keys.join(" "),
        keys_columns: first.columns.start..last.columns.end,
        language,
    });
}

fn pattern(reader: &mut Reader<'_>) -> Option<String> {
    let tokens = reader.tokens;
    let spanned = tokens.get(reader.at)?;
    reader.at += 1;
    match &spanned.token {
        Token::Quoted(template) => template.literal(),
        Token::Word(word) => Some(word.clone()),
    }
}

fn event(reader: &mut Reader<'_>) -> Option<Event> {
    const EVENTS: &str = "Use on save, on saved, on open or on start";
    let Some((kind, columns)) = reader.word() else {
        reader.missing(EVENTS);
        return None;
    };
    let make: fn(String) -> Event = match kind.as_str() {
        "start" => return Some(Event::Start),
        "save" => Event::BeforeSave,
        "saved" => Event::AfterSave,
        "open" => Event::Open,
        _ => {
            reader.problem(columns, EVENTS);
            return None;
        }
    };
    let Some(pattern) = pattern(reader) else {
        reader.missing("Add the files it is for, like \"*.md\"");
        return None;
    };
    Some(make(pattern))
}

fn on(reader: &mut Reader<'_>, routine: &mut Routine) {
    let event = event(reader);
    reader.finish();
    if let Some(event) = event {
        routine.triggers.push(Trigger {
            line: reader.number,
            event,
        });
    }
}

fn every(reader: &mut Reader<'_>, routine: &mut Routine) {
    let minutes = reader
        .word()
        .and_then(|(count, columns)| match count.parse::<u64>() {
            Ok(minutes) if minutes > 0 => Some(minutes),
            _ => {
                reader.problem(
                    columns,
                    "Give a whole number of minutes, like every 5 minutes",
                );
                None
            }
        });
    if !(reader.eat("minutes") || reader.eat("minute")) {
        reader.missing("End with minutes, like every 5 minutes");
    }
    reader.finish();
    if let Some(minutes) = minutes {
        routine.triggers.push(Trigger {
            line: reader.number,
            event: Event::Every(minutes),
        });
    }
}

fn header(reader: &mut Reader<'_>, routine: &mut Routine, keyword: &str, rest: &[char]) {
    match keyword {
        "summary" => summary(reader, routine, rest),
        "favorite" => {
            routine.favorite = true;
            reader.finish();
        }
        "key" => key(reader, routine),
        "on" => on(reader, routine),
        _ => every(reader, routine),
    }
}

fn close_blocks(stack: &mut Vec<Block>, problems: &mut Vec<Problem>, indent: usize) {
    while let Some(owner) = stack.last().and_then(|block| block.owner)
        && indent <= owner
        && let Some(block) = stack.pop()
    {
        if block.steps.is_empty() {
            problems.push(Problem {
                line: block.line,
                columns: owner..owner + "repeat".len(),
                message: "Indent the steps to repeat below this line".to_owned(),
            });
        }
        if let Some(parent) = stack.last_mut() {
            parent.steps.push(Step {
                line: block.line,
                action: Action::Repeat {
                    times: block.times,
                    steps: block.steps,
                },
            });
        }
    }
}

fn align(reader: &mut Reader<'_>, stack: &mut [Block], indent: usize) {
    let Some(top) = stack.last_mut() else {
        return;
    };
    match top.body {
        None => top.body = Some(indent),
        Some(body) if body == indent => {}
        Some(_) if top.owner.is_some() => {
            reader.problem(0..indent, "Line this step up with the steps above it");
        }
        Some(_) => reader.problem(0..indent, "Only steps inside repeat are indented"),
    }
}

fn repeat(reader: &mut Reader<'_>, stack: &mut Vec<Block>, indent: usize) {
    let times = reader.word().map(|(count, columns)| {
        let mut found = Vec::new();
        let times = word_template(&count, &columns, &mut found);
        reader.report(found);
        times
    });
    reader.finish();
    stack.push(Block {
        owner: Some(indent),
        line: reader.number,
        times,
        body: None,
        steps: Vec::new(),
    });
}

fn line(
    reader: &mut Reader<'_>,
    stack: &mut Vec<Block>,
    routine: &mut Routine,
    chars: &[char],
    indent: usize,
    found: Found,
) {
    let Some(Spanned {
        token: Token::Word(keyword),
        columns,
    }) = reader.tokens.first().cloned()
    else {
        reader.report(found);
        reader.problem(
            indent..chars.len(),
            "Start the line with a step, like run or insert",
        );
        return;
    };
    if keyword != "summary" {
        reader.report(found);
    }
    align(reader, stack, indent);
    if HEADERS.contains(&keyword.as_str()) {
        if stack.len() > 1 || indent > 0 {
            reader.problem(
                columns.clone(),
                format!("Put {keyword} at the top level, not inside repeat"),
            );
        }
        header(reader, routine, &keyword, &chars[columns.end..]);
    } else if keyword == "repeat" {
        repeat(reader, stack, indent);
    } else if let Some(action) = step(reader, &keyword, columns)
        && let Some(top) = stack.last_mut()
    {
        top.steps.push(Step {
            line: reader.number,
            action,
        });
    }
}

pub fn parse(source: &str) -> (Routine, Vec<Problem>) {
    let mut routine = Routine::default();
    let mut problems = Vec::new();
    let mut stack = vec![Block {
        owner: None,
        line: 0,
        times: None,
        body: Some(0),
        steps: Vec::new(),
    }];
    for (number, text) in source.lines().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        let indent = chars
            .iter()
            .take_while(|character| **character == ' ')
            .count();
        match chars.get(indent) {
            None | Some('#') => {}
            Some('\t') => problems.push(Problem {
                line: number,
                columns: 0..indent + 1,
                message: "Indent with spaces, not tabs".to_owned(),
            }),
            Some(_) => {
                close_blocks(&mut stack, &mut problems, indent);
                let mut found = Vec::new();
                let tokens = tokenize(&chars, indent, &mut found);
                let mut reader = Reader {
                    number,
                    tokens: &tokens,
                    at: 1,
                    problems: &mut problems,
                };
                line(&mut reader, &mut stack, &mut routine, &chars, indent, found);
            }
        }
    }
    close_blocks(&mut stack, &mut problems, 0);
    routine.steps = stack.pop().map(|root| root.steps).unwrap_or_default();
    (routine, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(literal: &str, columns: Range<usize>) -> Template {
        Template {
            pieces: vec![Piece::Text(literal.to_owned())],
            columns,
        }
    }

    fn clean(source: &str) -> Routine {
        let (routine, problems) = parse(source);
        assert_eq!(problems, [], "{source}");
        routine
    }

    fn messages(source: &str) -> Vec<(usize, String)> {
        parse(source)
            .1
            .into_iter()
            .map(|problem| (problem.line, problem.message))
            .collect()
    }

    const EXAMPLE: &str = "# Comment out the next lines
summary Comment out lines, asking how many
key C-c /

ask count \"How many lines? \" default \"3\"
repeat {count}
  run beginning-of-line
  insert \"// \"
  run next-line
message \"Commented {count} lines\"
";

    #[test]
    fn the_example_reads_as_headers_and_steps() {
        let routine = clean(EXAMPLE);

        assert_eq!(
            routine.summary.as_deref(),
            Some("Comment out lines, asking how many")
        );
        assert_eq!(routine.bindings.len(), 1);
        assert_eq!(routine.bindings[0].keys, "C-c /");
        assert_eq!(routine.bindings[0].keys_columns, 4..9);
        assert_eq!(routine.steps.len(), 3);
        assert!(matches!(
            &routine.steps[0].action,
            Action::Ask { name, answer: Answer::Text { default: Some(_) }, .. } if name == "count"
        ));
        let Action::Repeat { times, steps } = &routine.steps[1].action else {
            panic!("expected a repeat");
        };
        assert_eq!(times.as_ref().map(|times| times.values().count()), Some(1));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].line, 6);
        assert_eq!(steps[1].action, Action::Insert(text("// ", 9..14)));
        let Action::Message(message) = &routine.steps[2].action else {
            panic!("expected a message");
        };
        assert_eq!(
            message.pieces,
            [
                Piece::Text("Commented ".to_owned()),
                Piece::Value {
                    name: "count".to_owned(),
                    columns: 19..26,
                },
                Piece::Text(" lines".to_owned()),
            ]
        );
    }

    #[test]
    fn headers_set_keys_languages_triggers_and_favorites() {
        let routine = clean(
            "key C-c t in markdown\nfavorite\non save \"*.md\"\non saved *.txt\non open \"notes/*\"\non start\nevery 5 minutes\nevery 1 minute\nmessage \"hi\"",
        );

        assert_eq!(
            routine.bindings[0].language,
            Some(("markdown".to_owned(), 13..21))
        );
        assert!(routine.favorite);
        let events: Vec<Event> = routine
            .triggers
            .into_iter()
            .map(|trigger| trigger.event)
            .collect();
        assert_eq!(
            events,
            [
                Event::BeforeSave("*.md".to_owned()),
                Event::AfterSave("*.txt".to_owned()),
                Event::Open("notes/*".to_owned()),
                Event::Start,
                Event::Every(5),
                Event::Every(1),
            ]
        );
    }

    #[test]
    fn summaries_are_free_text() {
        let routine = clean("summary Say \"hi {there}\nmessage \"x\"");

        assert_eq!(routine.summary.as_deref(), Some("Say \"hi {there}"));
    }

    #[test]
    fn every_step_form_reads() {
        let routine = clean(
            "run find-file input \"~/notes.md\"
keys C-a C-k
keys \"C-e\"
ask kind \"Kind? \" from \"bug\" \"idea\"
ask sure \"Sure? \" yes-no
ask name \"Name? \"
confirm \"Go on?\"
search \"TODO\" backward regexp
replace \"a\" with \"b\" regexp in region
shell \"date\" insert
shell \"ls\"
filter \"sort -u\"
repeat
  search \"x\"",
        );

        let actions: Vec<&Action> = routine.steps.iter().map(|step| &step.action).collect();
        assert!(
            matches!(actions[0], Action::Run { command, input: Some(_), .. } if command == "find-file")
        );
        assert_eq!(*actions[1], Action::Keys(text("C-a C-k", 5..12)));
        assert_eq!(*actions[2], Action::Keys(text("C-e", 5..10)));
        assert!(
            matches!(actions[3], Action::Ask { answer: Answer::Pick(choices), .. } if choices.len() == 2)
        );
        assert!(matches!(
            actions[4],
            Action::Ask {
                answer: Answer::YesNo,
                ..
            }
        ));
        assert!(matches!(
            actions[5],
            Action::Ask {
                answer: Answer::Text { default: None },
                ..
            }
        ));
        assert!(matches!(actions[6], Action::Confirm(_)));
        assert!(matches!(
            actions[7],
            Action::Search {
                backward: true,
                regexp: true,
                ..
            }
        ));
        assert!(matches!(
            actions[8],
            Action::Replace {
                regexp: true,
                region: true,
                ..
            }
        ));
        assert!(matches!(actions[9], Action::Shell { insert: true, .. }));
        assert!(matches!(actions[10], Action::Shell { insert: false, .. }));
        assert!(matches!(actions[11], Action::Filter(_)));
        assert!(matches!(actions[12], Action::Repeat { times: None, steps } if steps.len() == 1));
    }

    #[test]
    fn texts_take_escapes_and_literal_braces() {
        let routine = clean("insert \"a \\\"b\\\"\\n{{x}} \\\\\"");

        assert_eq!(
            routine.steps[0].action,
            Action::Insert(text("a \"b\"\n{x} \\", 7..26))
        );
    }

    #[test]
    fn repeats_nest_by_indentation() {
        let routine = clean("repeat 2\n  repeat 3\n    insert \"x\"\n  insert \"y\"\ninsert \"z\"");

        assert_eq!(routine.steps.len(), 2);
        let Action::Repeat { steps, .. } = &routine.steps[0].action else {
            panic!("expected a repeat");
        };
        assert_eq!(steps.len(), 2);
        assert!(matches!(&steps[0].action, Action::Repeat { steps, .. } if steps.len() == 1));
    }

    #[test]
    fn mistakes_are_reported_by_line() {
        assert_eq!(
            messages("jump \"x\"\ninsert x\ninsert\ninsert \"a\" \"b\"\ninsert \"open\n\"text\""),
            [
                (0, "jump is not a step".to_owned()),
                (1, "Put the text to insert in quotes".to_owned()),
                (2, "Add the text to insert in quotes".to_owned()),
                (3, "Nothing more belongs on this line".to_owned()),
                (4, "Close the text with \"".to_owned()),
                (
                    5,
                    "Start the line with a step, like run or insert".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn keys_are_either_bare_or_quoted() {
        assert_eq!(
            messages("keys C-a \"b\"\nkeys"),
            [
                (
                    0,
                    "Write the keys either bare or as one text in quotes".to_owned()
                ),
                (1, "Add the keys to press".to_owned()),
            ]
        );
    }

    #[test]
    fn indentation_mistakes_are_reported() {
        assert_eq!(
            messages(
                "repeat 2\ninsert \"a\"\n  insert \"b\"\nrepeat 2\n  insert \"c\"\n   insert \"d\"\n\tinsert \"e\""
            ),
            [
                (0, "Indent the steps to repeat below this line".to_owned()),
                (2, "Only steps inside repeat are indented".to_owned()),
                (5, "Line this step up with the steps above it".to_owned()),
                (6, "Indent with spaces, not tabs".to_owned()),
            ]
        );
    }

    #[test]
    fn empty_repeats_are_marked_where_they_start() {
        let problems = parse("repeat 2\n  repeat 3\n  insert \"x\"").1;

        assert_eq!(problems.len(), 1);
        assert_eq!((problems[0].line, problems[0].columns.clone()), (1, 2..8));
    }

    #[test]
    fn headers_inside_repeat_and_bad_triggers_are_reported() {
        assert_eq!(
            messages(
                "repeat 2\n  key C-c x\n  insert \"a\"\non later\nevery soon minutes\nevery 5\non save"
            ),
            [
                (1, "Put key at the top level, not inside repeat".to_owned()),
                (3, "Use on save, on saved, on open or on start".to_owned()),
                (
                    4,
                    "Give a whole number of minutes, like every 5 minutes".to_owned()
                ),
                (5, "End with minutes, like every 5 minutes".to_owned()),
                (6, "Add the files it is for, like \"*.md\"".to_owned()),
            ]
        );
    }

    #[test]
    fn values_must_be_closed_and_named_well() {
        assert_eq!(
            messages("insert \"{open\"\ninsert \"{2bad}\"\nask 9x \"?\"\nkey \"{k}\""),
            [
                (0, "Close {value} with }".to_owned()),
                (1, "{2bad} is not a value name".to_owned()),
                (2, "9x is not a value name".to_owned()),
                (3, "Keys can't hold {values}".to_owned()),
            ]
        );
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        let routine = clean("\n# hello\n   # indented comment\n\ninsert \"a\"\n");

        assert_eq!(routine.steps.len(), 1);
        assert_eq!(routine.steps[0].line, 4);
    }
}
