use ox_lang::heap::Heap;
use ox_lang::value::{Value, print};
use oxmacs_core::external::managed::{Checksum, Form, ToolSpec};

const KEYS: &str =
    "name, repository, summary, group, form, tag-prefix, archives, checksum and setting";

fn text(heap: &Heap, key: &str, value: Value) -> Result<String, String> {
    heap.text(value).map(str::to_owned).ok_or_else(|| {
        format!(
            ":tools' {key}: must be text, not {}",
            print::brief(heap, value)
        )
    })
}

fn word(heap: &Heap, key: &str, value: Value, choices: &[&str]) -> Result<String, String> {
    match value {
        Value::Keyword(name) if choices.contains(&name.name()) => Ok(name.name().to_owned()),
        _ => Err(format!(
            ":tools' {key}: is :{}, not {}",
            choices.join(" or :"),
            print::brief(heap, value)
        )),
    }
}

fn archives(heap: &Heap, value: Value) -> Result<Vec<(String, String, String)>, String> {
    let wrong = || {
        ":tools' archives: maps systems to architectures to files, such as {linux: {x86_64: \"tool.tar.gz\"}}"
            .to_owned()
    };
    let mut found = Vec::new();
    for (os, arches) in heap.entries(value).ok_or_else(wrong)? {
        let Value::Keyword(os) = os else {
            return Err(wrong());
        };
        for (arch, file) in heap.entries(arches).ok_or_else(wrong)? {
            let (Value::Keyword(arch), Some(file)) = (arch, heap.text(file)) else {
                return Err(wrong());
            };
            found.push((
                os.name().to_owned(),
                arch.name().to_owned(),
                file.to_owned(),
            ));
        }
    }
    Ok(found)
}

fn tool(heap: &Heap, plugin: &str, value: Value) -> Result<ToolSpec, String> {
    let entries = heap
        .entries(value)
        .ok_or_else(|| format!(":tools must be a vector of maps with {KEYS}"))?;
    let mut spec = ToolSpec {
        name: String::new(),
        repository: String::new(),
        summary: String::new(),
        group: plugin.to_owned(),
        form: Form::Program,
        tag_prefix: String::new(),
        archives: Vec::new(),
        checksum: Checksum::Sidecar,
        setting: None,
    };
    for (key, value) in entries {
        let Value::Keyword(key) = key else {
            return Err(format!(
                ":tools takes {KEYS}, not {}",
                print::brief(heap, key)
            ));
        };
        match key.name() {
            "name" => spec.name = text(heap, "name", value)?,
            "repository" => spec.repository = text(heap, "repository", value)?,
            "summary" => spec.summary = text(heap, "summary", value)?,
            "group" => spec.group = text(heap, "group", value)?,
            "tag-prefix" => spec.tag_prefix = text(heap, "tag-prefix", value)?,
            "archives" => spec.archives = archives(heap, value)?,
            "form" => {
                spec.form = match word(heap, "form", value, &["program", "library"])?.as_str() {
                    "library" => Form::Library,
                    _ => Form::Program,
                };
            }
            "checksum" => {
                spec.checksum =
                    match word(heap, "checksum", value, &["sidecar", "digest"])?.as_str() {
                        "digest" => Checksum::Digest,
                        _ => Checksum::Sidecar,
                    };
            }
            "setting" => match value {
                Value::Keyword(setting) => spec.setting = Some(setting.name().to_owned()),
                _ => {
                    return Err(format!(
                        ":tools' setting: names one of the plugin's settings, such as :program, not {}",
                        print::brief(heap, value)
                    ));
                }
            },
            other => return Err(format!(":tools takes {KEYS}, not {other}")),
        }
    }
    if spec.name.is_empty() || spec.repository.is_empty() || spec.archives.is_empty() {
        return Err(":tools need a name, a GitHub repository and archives".to_owned());
    }
    Ok(spec)
}

pub(crate) fn declared(heap: &Heap, plugin: &str, value: Value) -> Result<Vec<ToolSpec>, String> {
    heap.elements(value)
        .ok_or_else(|| format!(":tools must be a vector of maps with {KEYS}"))?
        .into_iter()
        .map(|item| tool(heap, plugin, item))
        .collect()
}
