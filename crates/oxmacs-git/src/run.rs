use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use oxmacs_core::editor::{Editor, JobEvent, JobSpec, Outcome};

use crate::view;

#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub(crate) steps: VecDeque<Vec<String>>,
    pub(crate) scratch: Vec<PathBuf>,
    pub(crate) done: Option<fn(&mut Editor, &Path)>,
}

impl Plan {
    pub(crate) fn step<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.steps
            .push_back(args.into_iter().map(Into::into).collect());
        self
    }

    pub(crate) fn scratch(mut self, path: PathBuf) -> Self {
        self.scratch.push(path);
        self
    }
}

pub(crate) fn args<'a>(words: &[&str], paths: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    words
        .iter()
        .map(|word| (*word).to_owned())
        .chain(paths.into_iter().cloned())
        .collect()
}

pub(crate) fn run(editor: &mut Editor, root: PathBuf, mut plan: Plan) {
    let Some(args) = plan.steps.pop_front() else {
        finish(editor, &root, &plan);
        if let Some(done) = plan.done {
            done(editor, &root);
        }
        return;
    };
    let name = format!("git {}", args.first().map_or("", String::as_str));
    let mut output: Vec<String> = Vec::new();
    let directory = root.clone();
    editor.start_job(
        JobSpec {
            program: "git".to_owned(),
            args,
            directory,
            env: Vec::new(),
            label: "git",
            name,
            buffer: None,
            interactive: false,
        },
        move |editor, _, event| match event {
            JobEvent::Line(line) | JobEvent::Output(line) => output.push(line.to_owned()),
            JobEvent::Finished(outcome) => {
                if *outcome == Outcome::Exited(Some(0)) {
                    run(editor, root.clone(), std::mem::take(&mut plan));
                    return;
                }
                let reason = output
                    .iter()
                    .rev()
                    .find(|line| !line.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| match outcome {
                        Outcome::Exited(code) => format!("exited with {code:?}"),
                        Outcome::Failed(error) => error.clone(),
                    });
                finish(editor, &root, &plan);
                editor.show_message(format!("Git: {}", reason.trim()));
            }
        },
    );
}

fn finish(editor: &mut Editor, root: &Path, plan: &Plan) {
    for path in &plan.scratch {
        fs::remove_file(path).ok();
    }
    view::refresh_root(editor, root);
}
