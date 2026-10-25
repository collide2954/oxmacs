use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::BUNDLED;
use crate::plugins::{Listed, catalog};
use crate::{EDITOR_MODULES, checking_isolate};

fn modules() -> Vec<(&'static str, &'static str)> {
    ox_library::modules()
        .into_iter()
        .chain(EDITOR_MODULES.iter().copied())
        .collect()
}

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
}

fn plugins() -> Vec<Listed> {
    catalog(&mut checking_isolate(), BUNDLED)
}

fn shown_module(name: &str) -> String {
    if name == "ox" {
        "prelude".to_owned()
    } else {
        name.to_owned()
    }
}

fn markdown_plugins() -> String {
    let mut table =
        String::from("| Plugin | Version | Default | What it does |\n| --- | --- | --- | --- |\n");
    for plugin in plugins() {
        let default = if plugin.opt_in { "opt-in" } else { "on" };
        let _ = writeln!(
            table,
            "| `{}` | {} | {default} | {} |",
            plugin.name, plugin.version, plugin.summary
        );
    }
    table
}

fn markdown_modules() -> String {
    let mut table = String::from("| Module | What it gives |\n| --- | --- |\n");
    for (name, summary) in modules() {
        let shown = shown_module(name);
        let shown = if name == "ox" {
            shown
        } else {
            format!("`{shown}`")
        };
        let _ = writeln!(table, "| {shown} | {summary} |");
    }
    table
}

fn text_plugins() -> String {
    let listed = plugins();
    let width = listed
        .iter()
        .map(|plugin| plugin.name.len())
        .max()
        .unwrap_or(6);
    let mut table = format!(
        "```table\n{:width$}  Version  Default  What it does\n",
        "Plugin"
    );
    for plugin in listed {
        let default = if plugin.opt_in { "opt-in" } else { "on" };
        let _ = writeln!(
            table,
            "{:width$}  {:7}  {default:7}  {}",
            plugin.name,
            plugin.version.to_string(),
            plugin.summary
        );
    }
    table.push_str("```\n");
    table
}

fn text_modules() -> String {
    let width = modules()
        .iter()
        .map(|(name, _)| shown_module(name).len())
        .max()
        .unwrap_or(6);
    let mut table = format!("```table\n{:width$}  What it gives\n", "Module");
    for (name, summary) in modules() {
        let _ = writeln!(table, "{:width$}  {summary}", shown_module(name));
    }
    table.push_str("```\n");
    table
}

#[test]
fn every_built_in_module_is_described() {
    let isolate = checking_isolate();
    let installed: Vec<String> = isolate
        .module_names()
        .into_iter()
        .filter(|name| name == "ox" || name.starts_with("ox/"))
        .collect();
    let described: Vec<&str> = modules().into_iter().map(|(name, _)| name).collect();

    let missing: Vec<&String> = installed
        .iter()
        .filter(|name| !described.contains(&name.as_str()))
        .collect();
    let gone: Vec<&&str> = described
        .iter()
        .filter(|name| !installed.iter().any(|known| known == *name))
        .collect();
    assert!(missing.is_empty(), "undescribed modules: {missing:?}");
    assert!(
        gone.is_empty(),
        "described modules that don't exist: {gone:?}"
    );
}

#[test]
fn the_readme_lists_every_plugin_and_module() {
    let readme = fs::read_to_string(workspace().join("README.md")).unwrap();

    for table in [markdown_plugins(), markdown_modules()] {
        assert!(
            readme.contains(&table),
            "README.md should hold this table:\n\n{table}"
        );
    }
}

#[test]
fn the_manual_lists_every_plugin_and_module() {
    let manual = workspace().join("crates/oxmacs-ox/plugins/manual");
    let plugins_page = fs::read_to_string(manual.join("plugins.ox")).unwrap();
    let modules_page = fs::read_to_string(manual.join("modules.ox")).unwrap();
    let (plugins, modules) = (text_plugins(), text_modules());

    assert!(
        plugins_page.contains(&plugins),
        "plugins.ox should hold:\n\n{plugins}"
    );
    assert!(
        modules_page.contains(&modules),
        "modules.ox should hold:\n\n{modules}"
    );
}
