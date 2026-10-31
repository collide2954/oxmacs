use crate::runtime::plugins::Bundle;

pub const BUNDLED: &[Bundle] = &[
    WEATHER,
    DIRED,
    GIT,
    SCREENSAVER,
    PROFILER,
    PROCESSES,
    CUSTOMIZE,
    VAULT,
    IRC,
    CALENDAR,
    LIFE,
    REMOTE,
    MARKDOWN,
    MANUAL,
    R,
    ZAP,
    KEYCAST,
    OX,
    RUST,
    TOML,
    PYTHON,
    GO,
    C,
    CPP,
    TYPESCRIPT,
    JAVASCRIPT,
    JSON,
    TABS,
    EXPLORER,
    ZEN,
    EPUB,
    PDF,
    RODEO,
    BELLOW,
    WSL,
    PIXI,
];

const PIXI: Bundle = Bundle {
    name: "pixi",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/pixi/plugin.ox")),
        ("pixi.ox", include_str!("../../../plugins/pixi/pixi.ox")),
        (
            "project.ox",
            include_str!("../../../plugins/pixi/project.ox"),
        ),
    ],
};

const WSL: Bundle = Bundle {
    name: "wsl",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/wsl/plugin.ox")),
        ("wsl.ox", include_str!("../../../plugins/wsl/wsl.ox")),
        (
            "distros.ox",
            include_str!("../../../plugins/wsl/distros.ox"),
        ),
    ],
};

const BELLOW: Bundle = Bundle {
    name: "bellow",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/bellow/plugin.ox"),
        ),
        (
            "bellow.ox",
            include_str!("../../../plugins/bellow/bellow.ox"),
        ),
    ],
};

const RODEO: Bundle = Bundle {
    name: "rodeo",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/rodeo/plugin.ox"),
        ),
        ("rodeo.ox", include_str!("../../../plugins/rodeo/rodeo.ox")),
    ],
};

const PDF: Bundle = Bundle {
    name: "pdf",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/pdf/plugin.ox")),
        ("pdf.ox", include_str!("../../../plugins/pdf/pdf.ox")),
        (
            "reading.ox",
            include_str!("../../../plugins/pdf/reading.ox"),
        ),
    ],
};

const EPUB: Bundle = Bundle {
    name: "epub",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/epub/plugin.ox")),
        ("epub.ox", include_str!("../../../plugins/epub/epub.ox")),
        (
            "package.ox",
            include_str!("../../../plugins/epub/package.ox"),
        ),
        ("html.ox", include_str!("../../../plugins/epub/html.ox")),
        (
            "reading.ox",
            include_str!("../../../plugins/epub/reading.ox"),
        ),
        (
            "library.ox",
            include_str!("../../../plugins/epub/library.ox"),
        ),
    ],
};

const ZEN: Bundle = Bundle {
    name: "zen",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/zen/plugin.ox")),
        ("zen.ox", include_str!("../../../plugins/zen/zen.ox")),
        ("look.ox", include_str!("../../../plugins/zen/look.ox")),
    ],
};

const EXPLORER: Bundle = Bundle {
    name: "explorer",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/explorer/plugin.ox"),
        ),
        (
            "explorer.ox",
            include_str!("../../../plugins/explorer/explorer.ox"),
        ),
        ("tree.ox", include_str!("../../../plugins/explorer/tree.ox")),
    ],
};

const TABS: Bundle = Bundle {
    name: "tabs",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/tabs/plugin.ox")),
        ("tabs.ox", include_str!("../../../plugins/tabs/tabs.ox")),
        ("strip.ox", include_str!("../../../plugins/tabs/strip.ox")),
    ],
};

pub(crate) const OX: Bundle = Bundle {
    name: "ox-lang",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/ox-lang/plugin.ox"),
        ),
        (
            "ox-lang.ox",
            include_str!("../../../plugins/ox-lang/ox-lang.ox"),
        ),
    ],
};

const RUST: Bundle = Bundle {
    name: "rust",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/rust/plugin.ox")),
        ("rust.ox", include_str!("../../../plugins/rust/rust.ox")),
    ],
};

const TOML: Bundle = Bundle {
    name: "toml",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/toml/plugin.ox")),
        ("toml.ox", include_str!("../../../plugins/toml/toml.ox")),
    ],
};

const PYTHON: Bundle = Bundle {
    name: "python",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/python/plugin.ox"),
        ),
        (
            "python.ox",
            include_str!("../../../plugins/python/python.ox"),
        ),
        ("spec.ox", include_str!("../../../plugins/python/spec.ox")),
    ],
};

const GO: Bundle = Bundle {
    name: "go",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/go/plugin.ox")),
        ("go.ox", include_str!("../../../plugins/go/go.ox")),
        ("spec.ox", include_str!("../../../plugins/go/spec.ox")),
    ],
};

const C: Bundle = Bundle {
    name: "c",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/c/plugin.ox")),
        ("c.ox", include_str!("../../../plugins/c/c.ox")),
        ("spec.ox", include_str!("../../../plugins/c/spec.ox")),
    ],
};

const CPP: Bundle = Bundle {
    name: "cpp",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/cpp/plugin.ox")),
        ("cpp.ox", include_str!("../../../plugins/cpp/cpp.ox")),
        ("spec.ox", include_str!("../../../plugins/cpp/spec.ox")),
    ],
};

const TYPESCRIPT: Bundle = Bundle {
    name: "typescript",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/typescript/plugin.ox"),
        ),
        (
            "typescript.ox",
            include_str!("../../../plugins/typescript/typescript.ox"),
        ),
        (
            "spec.ox",
            include_str!("../../../plugins/typescript/spec.ox"),
        ),
    ],
};

const JAVASCRIPT: Bundle = Bundle {
    name: "javascript",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/javascript/plugin.ox"),
        ),
        (
            "javascript.ox",
            include_str!("../../../plugins/javascript/javascript.ox"),
        ),
        (
            "spec.ox",
            include_str!("../../../plugins/javascript/spec.ox"),
        ),
    ],
};

const JSON: Bundle = Bundle {
    name: "json",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/json/plugin.ox")),
        ("json.ox", include_str!("../../../plugins/json/json.ox")),
        ("spec.ox", include_str!("../../../plugins/json/spec.ox")),
    ],
};

const ZAP: Bundle = Bundle {
    name: "zap",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/zap/plugin.ox")),
        ("zap.ox", include_str!("../../../plugins/zap/zap.ox")),
    ],
};

const KEYCAST: Bundle = Bundle {
    name: "keycast",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/keycast/plugin.ox"),
        ),
        (
            "keycast.ox",
            include_str!("../../../plugins/keycast/keycast.ox"),
        ),
        (
            "stack.ox",
            include_str!("../../../plugins/keycast/stack.ox"),
        ),
    ],
};

const R: Bundle = Bundle {
    name: "r",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/r/plugin.ox")),
        ("r.ox", include_str!("../../../plugins/r/r.ox")),
        ("session.ox", include_str!("../../../plugins/r/session.ox")),
        ("helper.ox", include_str!("../../../plugins/r/helper.ox")),
        ("help.ox", include_str!("../../../plugins/r/help.ox")),
        ("pane.ox", include_str!("../../../plugins/r/pane.ox")),
        ("plots.ox", include_str!("../../../plugins/r/plots.ox")),
        ("data.ox", include_str!("../../../plugins/r/data.ox")),
        (
            "complete.ox",
            include_str!("../../../plugins/r/complete.ox"),
        ),
        (
            "definition.ox",
            include_str!("../../../plugins/r/definition.ox"),
        ),
        ("layout.ox", include_str!("../../../plugins/r/layout.ox")),
        ("package.ox", include_str!("../../../plugins/r/package.ox")),
        (
            "markdown.ox",
            include_str!("../../../plugins/r/markdown.ox"),
        ),
        (
            "language.ox",
            include_str!("../../../plugins/r/language.ox"),
        ),
        ("jobs.ox", include_str!("../../../plugins/r/jobs.ox")),
        ("debug.ox", include_str!("../../../plugins/r/debug.ox")),
    ],
};

const MANUAL: Bundle = Bundle {
    name: "manual",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/manual/plugin.ox"),
        ),
        (
            "manual.ox",
            include_str!("../../../plugins/manual/manual.ox"),
        ),
        ("start.ox", include_str!("../../../plugins/manual/start.ox")),
        (
            "language.ox",
            include_str!("../../../plugins/manual/language.ox"),
        ),
        ("data.ox", include_str!("../../../plugins/manual/data.ox")),
        ("flow.ox", include_str!("../../../plugins/manual/flow.ox")),
        (
            "modules.ox",
            include_str!("../../../plugins/manual/modules.ox"),
        ),
        ("tasks.ox", include_str!("../../../plugins/manual/tasks.ox")),
        (
            "editor.ox",
            include_str!("../../../plugins/manual/editor.ox"),
        ),
        ("ui.ox", include_str!("../../../plugins/manual/ui.ox")),
        (
            "plugins.ox",
            include_str!("../../../plugins/manual/plugins.ox"),
        ),
        ("tools.ox", include_str!("../../../plugins/manual/tools.ox")),
        (
            "tutorial.ox",
            include_str!("../../../plugins/manual/tutorial.ox"),
        ),
    ],
};

const MARKDOWN: Bundle = Bundle {
    name: "markdown",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/markdown/plugin.ox"),
        ),
        (
            "markdown.ox",
            include_str!("../../../plugins/markdown/markdown.ox"),
        ),
        (
            "render.ox",
            include_str!("../../../plugins/markdown/render.ox"),
        ),
        (
            "grammar.ox",
            include_str!("../../../plugins/markdown/grammar.ox"),
        ),
        (
            "style.ox",
            include_str!("../../../plugins/markdown/style.ox"),
        ),
        (
            "preview.ox",
            include_str!("../../../plugins/markdown/preview.ox"),
        ),
    ],
};

const REMOTE: Bundle = Bundle {
    name: "remote",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/remote/plugin.ox"),
        ),
        (
            "remote.ox",
            include_str!("../../../plugins/remote/remote.ox"),
        ),
        ("hosts.ox", include_str!("../../../plugins/remote/hosts.ox")),
        ("shell.ox", include_str!("../../../plugins/remote/shell.ox")),
        ("ops.ox", include_str!("../../../plugins/remote/ops.ox")),
    ],
};

const LIFE: Bundle = Bundle {
    name: "life",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/life/plugin.ox")),
        ("life.ox", include_str!("../../../plugins/life/life.ox")),
        ("rules.ox", include_str!("../../../plugins/life/rules.ox")),
    ],
};

const CALENDAR: Bundle = Bundle {
    name: "calendar",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/calendar/plugin.ox"),
        ),
        (
            "calendar.ox",
            include_str!("../../../plugins/calendar/calendar.ox"),
        ),
        (
            "store.ox",
            include_str!("../../../plugins/calendar/store.ox"),
        ),
        ("ics.ox", include_str!("../../../plugins/calendar/ics.ox")),
        (
            "rrule.ox",
            include_str!("../../../plugins/calendar/rrule.ox"),
        ),
        (
            "caldav.ox",
            include_str!("../../../plugins/calendar/caldav.ox"),
        ),
        (
            "reminders.ox",
            include_str!("../../../plugins/calendar/reminders.ox"),
        ),
    ],
};

const IRC: Bundle = Bundle {
    name: "irc",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/irc/plugin.ox")),
        ("irc.ox", include_str!("../../../plugins/irc/irc.ox")),
        ("client.ox", include_str!("../../../plugins/irc/client.ox")),
        (
            "protocol.ox",
            include_str!("../../../plugins/irc/protocol.ox"),
        ),
        ("format.ox", include_str!("../../../plugins/irc/format.ox")),
        ("logs.ox", include_str!("../../../plugins/irc/logs.ox")),
    ],
};

const VAULT: Bundle = Bundle {
    name: "vault",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/vault/plugin.ox"),
        ),
        ("vault.ox", include_str!("../../../plugins/vault/vault.ox")),
        ("store.ox", include_str!("../../../plugins/vault/store.ox")),
        (
            "import.ox",
            include_str!("../../../plugins/vault/import.ox"),
        ),
        ("words.ox", include_str!("../../../plugins/vault/words.ox")),
    ],
};

const CUSTOMIZE: Bundle = Bundle {
    name: "customize",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/customize/plugin.ox"),
        ),
        (
            "customize.ox",
            include_str!("../../../plugins/customize/customize.ox"),
        ),
        (
            "form.ox",
            include_str!("../../../plugins/customize/form.ox"),
        ),
        (
            "written.ox",
            include_str!("../../../plugins/customize/written.ox"),
        ),
        (
            "save.ox",
            include_str!("../../../plugins/customize/save.ox"),
        ),
    ],
};

const PROFILER: Bundle = Bundle {
    name: "profiler",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/profiler/plugin.ox"),
        ),
        (
            "profiler.ox",
            include_str!("../../../plugins/profiler/profiler.ox"),
        ),
        (
            "report.ox",
            include_str!("../../../plugins/profiler/report.ox"),
        ),
    ],
};

const PROCESSES: Bundle = Bundle {
    name: "processes",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/processes/plugin.ox"),
        ),
        (
            "processes.ox",
            include_str!("../../../plugins/processes/processes.ox"),
        ),
        (
            "table.ox",
            include_str!("../../../plugins/processes/table.ox"),
        ),
    ],
};

const SCREENSAVER: Bundle = Bundle {
    name: "screensaver",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/screensaver/plugin.ox"),
        ),
        (
            "screensaver.ox",
            include_str!("../../../plugins/screensaver/screensaver.ox"),
        ),
        (
            "noise.ox",
            include_str!("../../../plugins/screensaver/noise.ox"),
        ),
        (
            "starfield.ox",
            include_str!("../../../plugins/screensaver/starfield.ox"),
        ),
        (
            "matrix.ox",
            include_str!("../../../plugins/screensaver/matrix.ox"),
        ),
        (
            "savers.ox",
            include_str!("../../../plugins/screensaver/savers.ox"),
        ),
        (
            "shading.ox",
            include_str!("../../../plugins/screensaver/shading.ox"),
        ),
        (
            "backdrops.ox",
            include_str!("../../../plugins/screensaver/backdrops.ox"),
        ),
        (
            "tones.ox",
            include_str!("../../../plugins/screensaver/tones.ox"),
        ),
        (
            "pond.ox",
            include_str!("../../../plugins/screensaver/pond.ox"),
        ),
        (
            "fireworks.ox",
            include_str!("../../../plugins/screensaver/fireworks.ox"),
        ),
        (
            "harmonograph.ox",
            include_str!("../../../plugins/screensaver/harmonograph.ox"),
        ),
        (
            "aurora.ox",
            include_str!("../../../plugins/screensaver/aurora.ox"),
        ),
        (
            "clock.ox",
            include_str!("../../../plugins/screensaver/clock.ox"),
        ),
        (
            "flow.ox",
            include_str!("../../../plugins/screensaver/flow.ox"),
        ),
        (
            "orbits.ox",
            include_str!("../../../plugins/screensaver/orbits.ox"),
        ),
        (
            "snowfall.ox",
            include_str!("../../../plugins/screensaver/snowfall.ox"),
        ),
        (
            "tunnel.ox",
            include_str!("../../../plugins/screensaver/tunnel.ox"),
        ),
        (
            "lava.ox",
            include_str!("../../../plugins/screensaver/lava.ox"),
        ),
        (
            "constellations.ox",
            include_str!("../../../plugins/screensaver/constellations.ox"),
        ),
        (
            "plasma.ox",
            include_str!("../../../plugins/screensaver/plasma.ox"),
        ),
        (
            "bubbles.ox",
            include_str!("../../../plugins/screensaver/bubbles.ox"),
        ),
        (
            "kaleidoscope.ox",
            include_str!("../../../plugins/screensaver/kaleidoscope.ox"),
        ),
        (
            "wireframes.ox",
            include_str!("../../../plugins/screensaver/wireframes.ox"),
        ),
        (
            "city.ox",
            include_str!("../../../plugins/screensaver/city.ox"),
        ),
        (
            "pendulums.ox",
            include_str!("../../../plugins/screensaver/pendulums.ox"),
        ),
        (
            "tree.ox",
            include_str!("../../../plugins/screensaver/tree.ox"),
        ),
        (
            "ocean.ox",
            include_str!("../../../plugins/screensaver/ocean.ox"),
        ),
        (
            "glass.ox",
            include_str!("../../../plugins/screensaver/glass.ox"),
        ),
        (
            "galaxy.ox",
            include_str!("../../../plugins/screensaver/galaxy.ox"),
        ),
        (
            "ants.ox",
            include_str!("../../../plugins/screensaver/ants.ox"),
        ),
        (
            "campfire.ox",
            include_str!("../../../plugins/screensaver/campfire.ox"),
        ),
        (
            "sorting.ox",
            include_str!("../../../plugins/screensaver/sorting.ox"),
        ),
        (
            "storm.ox",
            include_str!("../../../plugins/screensaver/storm.ox"),
        ),
        (
            "fireflies.ox",
            include_str!("../../../plugins/screensaver/fireflies.ox"),
        ),
        (
            "mandelbrot.ox",
            include_str!("../../../plugins/screensaver/mandelbrot.ox"),
        ),
    ],
};

const GIT: Bundle = Bundle {
    name: "git",
    files: &[
        ("plugin.ox", include_str!("../../../plugins/git/plugin.ox")),
        ("git.ox", include_str!("../../../plugins/git/git.ox")),
        ("repo.ox", include_str!("../../../plugins/git/repo.ox")),
        ("diff.ox", include_str!("../../../plugins/git/diff.ox")),
        ("status.ox", include_str!("../../../plugins/git/status.ox")),
        ("pane.ox", include_str!("../../../plugins/git/pane.ox")),
        ("commit.ox", include_str!("../../../plugins/git/commit.ox")),
    ],
};

const DIRED: Bundle = Bundle {
    name: "dired",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/dired/plugin.ox"),
        ),
        ("dired.ox", include_str!("../../../plugins/dired/dired.ox")),
        (
            "listing.ox",
            include_str!("../../../plugins/dired/listing.ox"),
        ),
    ],
};

const WEATHER: Bundle = Bundle {
    name: "weather",
    files: &[
        (
            "plugin.ox",
            include_str!("../../../plugins/weather/plugin.ox"),
        ),
        (
            "weather.ox",
            include_str!("../../../plugins/weather/weather.ox"),
        ),
        (
            "forecast.ox",
            include_str!("../../../plugins/weather/forecast.ox"),
        ),
        ("page.ox", include_str!("../../../plugins/weather/page.ox")),
    ],
};

#[cfg(test)]
mod weather;

#[cfg(test)]
mod bellow;

#[cfg(test)]
mod catalog;

#[cfg(test)]
mod git;

#[cfg(test)]
mod screensaver;

#[cfg(test)]
mod profiler;

#[cfg(test)]
mod processes;

#[cfg(test)]
mod customize;

#[cfg(test)]
mod vault;

#[cfg(test)]
mod irc;

#[cfg(test)]
mod calendar;

#[cfg(test)]
mod life;

#[cfg(test)]
mod remote;

#[cfg(test)]
mod markdown;

#[cfg(test)]
mod manual;

#[cfg(test)]
mod r;

#[cfg(test)]
mod zap;

#[cfg(test)]
mod keycast;

#[cfg(test)]
mod dired;

#[cfg(test)]
mod languages;

#[cfg(test)]
mod tabs;

#[cfg(test)]
mod explorer;

#[cfg(test)]
mod zen;

#[cfg(test)]
mod epub;

#[cfg(test)]
mod pdf;

#[cfg(test)]
mod rodeo;

#[cfg(test)]
mod pixi;

#[cfg(test)]
mod wsl;
