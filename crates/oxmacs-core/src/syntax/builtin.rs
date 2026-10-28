use tree_sitter::Node;

use crate::syntax::{Feature, Grammar, Indentation, Language, LanguageSpec, Outline, Roots};
use crate::text::Buffer;

static RUST_GRAMMAR: Grammar = Grammar::new(
    "rust",
    || tree_sitter_rust::LANGUAGE.into(),
    include_str!("../../queries/rust/highlights.scm"),
    "",
);

static TOML_GRAMMAR: Grammar = Grammar::new(
    "toml",
    || tree_sitter_toml_ng::LANGUAGE.into(),
    include_str!("../../queries/toml/highlights.scm"),
    "",
);

static R_GRAMMAR: Grammar = Grammar::new(
    "r",
    || tree_sitter_r::LANGUAGE.into(),
    include_str!("../../queries/r/highlights.scm"),
    "",
);

const BRACKETS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')];

const EDITING: &[Feature] = &[
    Feature::Highlighting,
    Feature::Indentation,
    Feature::Comments,
    Feature::Pairs,
    Feature::Motion,
    Feature::Items,
];

static RUST: LanguageSpec = LanguageSpec {
    name: "Rust",
    extensions: &["rs"],
    file_names: &[],
    aliases: &["rust", "rs"],
    line_comment: Some("//"),
    pairs: BRACKETS,
    indentation: Indentation {
        unit: 4,
        continues: true,
        fresh_words: &["where"],
        from_owner: false,
        hanging: false,
        guess: None,
    },
    grammar: Some(&RUST_GRAMMAR),
    grammars: &[],
    outline: Outline {
        item: rust_item,
        context: |node| matches!(node.kind(), "impl_item" | "trait_item" | "mod_item"),
        defun: |node| {
            matches!(
                node.kind(),
                "function_item"
                    | "function_signature_item"
                    | "impl_item"
                    | "struct_item"
                    | "enum_item"
                    | "union_item"
                    | "trait_item"
                    | "mod_item"
                    | "macro_definition"
                    | "const_item"
                    | "static_item"
                    | "type_item"
            )
        },
        sections: false,
        statements: &[],
    },
    features: &[
        Feature::Highlighting,
        Feature::Indentation,
        Feature::Comments,
        Feature::Pairs,
        Feature::Formatting,
        Feature::Lint,
        Feature::Motion,
        Feature::Items,
    ],
    roots: Roots {
        markers: &["Cargo.toml"],
        locate: Some(&[
            "cargo",
            "locate-project",
            "--workspace",
            "--message-format",
            "plain",
        ]),
    },
};

static TOML: LanguageSpec = LanguageSpec {
    name: "TOML",
    extensions: &["toml"],
    file_names: &["Cargo.lock"],
    aliases: &["toml"],
    line_comment: Some("#"),
    pairs: BRACKETS,
    indentation: Indentation::KEEP,
    grammar: Some(&TOML_GRAMMAR),
    grammars: &[],
    outline: Outline {
        item: toml_item,
        context: |_| false,
        defun: |node| matches!(node.kind(), "table" | "table_array_element"),
        sections: false,
        statements: &[],
    },
    features: EDITING,
    roots: Roots::NONE,
};

static R: LanguageSpec = LanguageSpec {
    name: "R",
    extensions: &["R", "r"],
    file_names: &[".Rprofile"],
    aliases: &["r"],
    line_comment: Some("#"),
    pairs: &[
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
    ],
    indentation: Indentation {
        unit: 2,
        continues: true,
        fresh_words: &[],
        from_owner: true,
        hanging: true,
        guess: None,
    },
    grammar: Some(&R_GRAMMAR),
    grammars: &[],
    outline: Outline {
        item: r_item,
        context: is_function_assignment,
        defun: is_function_assignment,
        sections: true,
        statements: &["program", "braced_expression"],
    },
    features: &[
        Feature::Highlighting,
        Feature::Indentation,
        Feature::Comments,
        Feature::Pairs,
        Feature::Formatting,
        Feature::Motion,
        Feature::Items,
        Feature::Completion,
        Feature::Repl,
    ],
    roots: Roots::marked(&["DESCRIPTION", "renv.lock", "*.Rproj"]),
};

impl Language {
    pub const RUST: Self = Self::new(&RUST);
    pub const TOML: Self = Self::new(&TOML);
    pub const R: Self = Self::new(&R);
    pub const BUILT_IN: [Self; 3] = [Self::RUST, Self::TOML, Self::R];
}

fn field(node: Node<'_>, buffer: &Buffer, name: &str) -> Option<String> {
    node.child_by_field_name(name)
        .map(|child| buffer.text_in_bytes(child.byte_range()))
}

fn rust_item(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    let keyword = match node.kind() {
        "function_item" | "function_signature_item" => "fn",
        "struct_item" => "struct",
        "enum_item" => "enum",
        "union_item" => "union",
        "trait_item" => "trait",
        "mod_item" => "mod",
        "const_item" => "const",
        "static_item" => "static",
        "type_item" => "type",
        "macro_definition" => {
            return field(node, buffer, "name").map(|name| format!("macro {name}!"));
        }
        "impl_item" => {
            let target = field(node, buffer, "type")?;
            return Some(match field(node, buffer, "trait") {
                Some(name) => format!("impl {name} for {target}"),
                None => format!("impl {target}"),
            });
        }
        _ => return None,
    };
    field(node, buffer, "name").map(|name| format!("{keyword} {name}"))
}

fn toml_item(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    if !matches!(node.kind(), "table" | "table_array_element") {
        return None;
    }
    let mut cursor = node.walk();
    let key = node
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"))?;
    let key = buffer.text_in_bytes(key.byte_range());
    Some(if node.kind() == "table" {
        format!("[{key}]")
    } else {
        format!("[[{key}]]")
    })
}

fn r_item(node: Node<'_>, buffer: &Buffer) -> Option<String> {
    is_function_assignment(node)
        .then(|| field(node, buffer, "lhs"))
        .flatten()
        .map(|name| format!("function {}", name.trim_matches(['"', '\'', '`'])))
}

fn is_function_assignment(node: Node<'_>) -> bool {
    node.kind() == "binary_operator"
        && node
            .child_by_field_name("rhs")
            .is_some_and(|rhs| rhs.kind() == "function_definition")
        && node
            .child_by_field_name("operator")
            .is_some_and(|operator| matches!(operator.kind(), "<-" | "=" | "<<-"))
}
