pub(super) mod compile;
pub(super) mod diagnostics_list;
pub(super) mod errors;
pub(super) mod lint;
pub(super) mod run;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::editor::build::errors::ErrorPattern;
use crate::editor::build::lint::{LintRun, Linter};
use crate::editor::{JobId, Outcome};
use crate::external::diagnostics::{Diagnostic, LintLine, Severity};
use crate::files;
use crate::syntax::Language;

#[derive(Debug)]
struct Run {
    job: JobId,
    language: Language,
    linter: Linter,
    root: PathBuf,
    scope: PathBuf,
    label: String,
    name: String,
    found: Vec<Diagnostic>,
    built: bool,
    failure: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Walks {
    #[default]
    Compilation,
    Lints,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::editor) enum Finished {
    Changed,
    Failed(String),
    Unknown,
}

#[derive(Debug)]
pub(in crate::editor) struct Builds {
    linters: Vec<(Language, Linter)>,
    patterns: Vec<ErrorPattern>,
    owners: Vec<String>,
    last_command: Option<String>,
    walks: Walks,
    due: Option<(Language, PathBuf)>,
    last: Option<(Language, PathBuf)>,
    runs: Vec<Run>,
    diagnostics: Vec<Diagnostic>,
    identities: HashMap<PathBuf, PathBuf>,
    root: Option<PathBuf>,
    current: Option<usize>,
    revision: u64,
}

impl Default for Builds {
    fn default() -> Self {
        Self {
            linters: Vec::new(),
            owners: vec![String::new(); errors::built_in().len()],
            patterns: errors::built_in(),
            last_command: None,
            walks: Walks::default(),
            due: None,
            last: None,
            runs: Vec::new(),
            diagnostics: Vec::new(),
            identities: HashMap::new(),
            root: None,
            current: None,
            revision: 0,
        }
    }
}

impl Builds {
    pub(in crate::editor) fn set_linter(&mut self, language: Language, linter: Option<Linter>) {
        self.linters.retain(|(known, _)| *known != language);
        if let Some(linter) = linter {
            self.linters.push((language, linter));
        }
    }

    pub(in crate::editor) fn set_patterns(&mut self, owner: &str, patterns: Vec<ErrorPattern>) {
        let kept: Vec<(String, ErrorPattern)> = self
            .owners
            .drain(..)
            .zip(self.patterns.drain(..))
            .filter(|(known, _)| known != owner)
            .chain(
                patterns
                    .into_iter()
                    .map(|pattern| (owner.to_owned(), pattern)),
            )
            .collect();
        (self.owners, self.patterns) = kept.into_iter().unzip();
    }

    pub(in crate::editor) fn patterns(&self) -> &[ErrorPattern] {
        &self.patterns
    }

    pub(in crate::editor) fn has_linter(&self, language: &Language) -> bool {
        self.linters.iter().any(|(known, _)| known == language)
    }

    pub(in crate::editor) fn last_command(&self) -> Option<&str> {
        self.last_command.as_deref()
    }

    pub(in crate::editor) fn compiling(&mut self, command: &str) {
        self.walks = Walks::Compilation;
        self.last_command = Some(command.to_owned());
    }

    pub(in crate::editor) fn walks_lints(&self) -> bool {
        self.walks == Walks::Lints
    }

    pub(in crate::editor) fn request(&mut self, language: Language, path: &Path) {
        if self.has_linter(&language) {
            self.due = Some((language, path.to_owned()));
        }
    }

    pub(in crate::editor) fn recheck(&mut self) -> bool {
        let Some(last) = self.last.clone() else {
            return false;
        };
        self.due = Some(last);
        true
    }

    pub(in crate::editor) fn take_due(&mut self) -> Option<(Language, PathBuf, Linter)> {
        let (language, path) = self.due.take()?;
        let linter = self
            .linters
            .iter()
            .find(|(known, _)| *known == language)
            .map(|(_, linter)| linter.clone())?;
        Some((language, path, linter))
    }

    pub(in crate::editor) fn stale(&mut self, language: &Language) -> Vec<JobId> {
        self.runs
            .extract_if(.., |stale| stale.language == *language)
            .map(|stale| stale.job)
            .collect()
    }

    pub(in crate::editor) fn started(
        &mut self,
        job: JobId,
        (language, path, linter): (Language, PathBuf, Linter),
        run: LintRun,
    ) {
        self.last = Some((language.clone(), path));
        self.runs.push(Run {
            job,
            language,
            linter,
            scope: files::identity(&run.root),
            root: run.root,
            label: run.label,
            name: run.name,
            found: Vec::new(),
            built: false,
            failure: None,
        });
    }

    pub(in crate::editor) fn output(&mut self, job: JobId, line: &str) {
        let Some(run) = self.runs.iter_mut().find(|run| run.job == job) else {
            return;
        };
        match (run.linter.format.read)(line, &run.root) {
            LintLine::Diagnostic(diagnostic) => {
                if !run.found.contains(&diagnostic) {
                    run.found.push(diagnostic);
                }
            }
            LintLine::Finished => run.built = true,
            LintLine::Other => {
                if run.failure.is_none() && line.starts_with("error") {
                    run.failure = Some(line.to_owned());
                }
            }
        }
    }

    pub(in crate::editor) fn finished(
        &mut self,
        job: JobId,
        outcome: &Outcome,
        language_of: impl Fn(&Path) -> Option<Language>,
    ) -> Finished {
        let Some(at) = self.runs.iter().position(|run| run.job == job) else {
            return Finished::Unknown;
        };
        let run = self.runs.remove(at);
        let completed = run.built || (run.linter.format.complete)(outcome);
        match outcome {
            Outcome::Exited(_) if completed => {
                let Self {
                    diagnostics,
                    identities,
                    ..
                } = self;
                diagnostics.retain(|diagnostic| {
                    let place = identities
                        .get(&diagnostic.path)
                        .map_or(diagnostic.path.as_path(), PathBuf::as_path);
                    !(place.starts_with(&run.scope)
                        && language_of(&diagnostic.path).as_ref() == Some(&run.language))
                });
                self.settle(run.found);
                self.root = Some(run.root);
                self.walks = Walks::Lints;
                Finished::Changed
            }
            Outcome::Exited(_) => {
                let reason = run.failure.unwrap_or_else(|| "failed".to_owned());
                Finished::Failed(format!("{}: {reason}", run.name))
            }
            Outcome::Failed(error) => Finished::Failed(format!("{}: {error}", run.name)),
        }
    }

    pub(in crate::editor) fn replace(&mut self, path: &Path, found: Vec<Diagnostic>) {
        let wanted = files::identity(path);
        let Self {
            diagnostics,
            identities,
            ..
        } = self;
        diagnostics.retain(|diagnostic| {
            identities
                .get(&diagnostic.path)
                .map_or(diagnostic.path.as_path(), PathBuf::as_path)
                != wanted
        });
        self.settle(found);
    }

    fn settle(&mut self, found: Vec<Diagnostic>) {
        self.diagnostics.extend(found);
        self.diagnostics.sort();
        let present: HashSet<&PathBuf> = self.diagnostics.iter().map(|found| &found.path).collect();
        self.identities.retain(|path, _| present.contains(path));
        for path in present {
            if !self.identities.contains_key(path) {
                self.identities.insert(path.clone(), files::identity(path));
            }
        }
        self.revision += 1;
        self.current = None;
    }

    fn identity_of<'a>(&'a self, path: &'a Path) -> &'a Path {
        self.identities.get(path).map_or(path, PathBuf::as_path)
    }

    pub(in crate::editor) fn revision(&self) -> u64 {
        self.revision
    }

    pub(in crate::editor) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub(in crate::editor) fn diagnostic(&self, index: usize) -> Option<&Diagnostic> {
        self.diagnostics.get(index)
    }

    pub(in crate::editor) fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub(in crate::editor) fn on_file<'a>(
        &'a self,
        identity: &'a Path,
    ) -> impl Iterator<Item = (usize, &'a Diagnostic)> {
        self.diagnostics
            .iter()
            .enumerate()
            .filter(move |(_, found)| self.identity_of(&found.path) == identity)
    }

    pub(in crate::editor) fn counts(&self) -> Vec<(PathBuf, usize, usize)> {
        let mut counts: Vec<(PathBuf, usize, usize)> = Vec::new();
        for diagnostic in &self.diagnostics {
            let known = counts
                .iter()
                .position(|(path, _, _)| *path == diagnostic.path);
            let at = known.unwrap_or_else(|| {
                counts.push((diagnostic.path.clone(), 0, 0));
                counts.len() - 1
            });
            match diagnostic.severity {
                Severity::Error => counts[at].1 += 1,
                Severity::Warning => counts[at].2 += 1,
            }
        }
        counts
    }

    pub(in crate::editor) fn status(
        &self,
        identity: &Path,
        language: Option<&Language>,
    ) -> Option<String> {
        if let Some(run) = self
            .runs
            .iter()
            .find(|run| identity.starts_with(&run.scope) && Some(&run.language) == language)
        {
            return Some(run.label.clone());
        }
        let count = |severity| {
            self.on_file(identity)
                .filter(|(_, diagnostic)| diagnostic.severity == severity)
                .count()
        };
        let counts: Vec<String> = [
            ("E", count(Severity::Error)),
            ("W", count(Severity::Warning)),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(label, count)| format!("{label}{count}"))
        .collect();
        (!counts.is_empty()).then(|| counts.join(" "))
    }

    pub(in crate::editor) fn step(&self, forward: bool) -> Option<usize> {
        errors::step(self.current, forward, self.diagnostics.len())
    }

    pub(in crate::editor) fn visit(&mut self, index: usize) -> Option<Diagnostic> {
        let diagnostic = self.diagnostics.get(index)?.clone();
        self.current = Some(index);
        Some(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::lint::fixtures::FINISHED;
    use super::lint::{CARGO, LintArg, LintRoot, LintRun, Linter};
    use super::{Builds, Finished};
    use crate::editor::{JobId, Outcome};
    use crate::external::diagnostics::sample;
    use crate::files;
    use crate::syntax::Language;

    fn builds() -> Builds {
        let mut builds = Builds::default();
        let cargo = Linter {
            program: "cargo".to_owned(),
            args: vec![LintArg::Text("check".to_owned())],
            root: LintRoot::Language,
            format: CARGO,
        };
        builds.set_linter(Language::rust(), Some(cargo));
        builds
    }

    fn run(root: &Path) -> LintRun {
        LintRun {
            root: root.to_path_buf(),
            program: "cargo".to_owned(),
            args: Vec::new(),
            label: "Checking".to_owned(),
            name: "cargo check".to_owned(),
        }
    }

    fn checking(builds: &mut Builds, root: &Path, job: u64) -> JobId {
        builds.request(Language::rust(), &root.join("src").join("main.rs"));
        let due = builds.take_due().unwrap();
        builds.started(JobId(job), due, run(root));
        JobId(job)
    }

    #[test]
    fn rechecking_before_anything_ran_leaves_what_is_due() {
        let mut builds = builds();
        builds.request(Language::rust(), Path::new("main.rs"));

        assert!(!builds.recheck());
        assert_eq!(
            builds.take_due().map(|due| due.1),
            Some(PathBuf::from("main.rs"))
        );
        assert!(builds.take_due().is_none());
    }

    #[test]
    fn a_finished_check_replaces_its_language_s_results_under_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let mut builds = builds();
        let job = checking(&mut builds, dir.path(), 1);
        let main = dir.path().join("src").join("main.rs");
        let identity = files::identity(&main);
        assert_eq!(
            builds.status(&identity, Some(&Language::rust())),
            Some("Checking".to_owned())
        );

        builds.output(job, &sample("error", "src/main.rs", (1, 1), (1, 2), "bad"));
        builds.output(job, FINISHED);
        let before = builds.revision();
        let finished =
            builds.finished(job, &Outcome::Exited(Some(101)), |_| Some(Language::rust()));

        assert_eq!(finished, Finished::Changed);
        assert_eq!(builds.revision(), before + 1);
        assert!(builds.walks_lints());
        assert_eq!(builds.counts(), [(main.clone(), 1, 0)]);
        assert_eq!(
            builds.status(&identity, Some(&Language::rust())),
            Some("E1".to_owned())
        );
        assert!(builds.recheck());

        let job = checking(&mut builds, dir.path(), 2);
        builds.output(job, FINISHED);
        builds.finished(job, &Outcome::Exited(Some(0)), |_| None);
        assert_eq!(builds.counts().len(), 1, "other languages' results stay");
    }

    #[test]
    fn failed_or_unknown_runs_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut builds = builds();
        let job = checking(&mut builds, dir.path(), 1);

        builds.output(job, "error: could not find `Cargo.toml`");
        assert_eq!(
            builds.finished(job, &Outcome::Exited(Some(101)), |_| None),
            Finished::Failed("cargo check: error: could not find `Cargo.toml`".to_owned())
        );
        assert_eq!(
            builds.finished(JobId(9), &Outcome::Exited(Some(0)), |_| None),
            Finished::Unknown
        );
        assert_eq!(builds.revision(), 0);
        assert_eq!(builds.stale(&Language::rust()), []);
    }

    #[test]
    fn compiling_takes_next_error_back_from_the_lints() {
        let dir = tempfile::tempdir().unwrap();
        let mut builds = builds();
        let job = checking(&mut builds, dir.path(), 1);
        builds.output(job, FINISHED);
        builds.finished(job, &Outcome::Exited(Some(0)), |_| None);
        assert!(builds.walks_lints());

        builds.compiling("cargo test");

        assert!(!builds.walks_lints());
        assert_eq!(builds.last_command(), Some("cargo test"));
    }
}
