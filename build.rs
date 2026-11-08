//! Compiles the translations in `po/*.po` into the binary (see `src/i18n.rs`):
//! no `.mo` files to install, the same binary is translated whichever way
//! it was packaged. Generates `$OUT_DIR/catalogs.rs`.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=po");
    let mut po_files: Vec<_> = std::fs::read_dir("po")
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "po"))
                .collect()
        })
        .unwrap_or_default();
    po_files.sort();

    let mut out = String::from("pub static CATALOGS: &[Catalog] = &[\n");
    for path in &po_files {
        println!("cargo:rerun-if-changed={}", path.display());
        catalog(path, &mut out);
    }
    out.push_str("];\n");

    let dest = Path::new(&std::env::var("OUT_DIR").expect("set by cargo")).join("catalogs.rs");
    std::fs::write(dest, out).expect("OUT_DIR is writable");
}

fn catalog(path: &Path, out: &mut String) {
    let language = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("po file names are language codes");
    let catalog =
        polib::po_file::parse(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let rule = plural_rule(&catalog.metadata.plural_rules.expr).unwrap_or_else(|| {
        panic!(
            "{}: plural rule {:?} unknown to src/i18n.rs (PluralRule)",
            path.display(),
            catalog.metadata.plural_rules.expr
        )
    });

    let mut singular = Vec::new();
    let mut plural = Vec::new();
    for message in catalog.messages() {
        // Untranslated and fuzzy (unreviewed) entries fall back to English.
        if !message.is_translated() || message.is_fuzzy() || message.msgid().is_empty() {
            continue;
        }
        let key = match message.msgctxt() {
            Some(context) => format!("{context}\u{4}{}", message.msgid()),
            None => message.msgid().to_string(),
        };
        if message.is_plural() {
            let forms = message.msgstr_plural().expect("plural message");
            plural.push((key, forms.clone()));
        } else {
            let text = message.msgstr().expect("singular message").to_string();
            singular.push((key, text));
        }
    }
    // Sorted for `binary_search_by` at runtime.
    singular.sort();
    plural.sort();

    let _ = writeln!(
        out,
        "    Catalog {{\n        language: {language:?},\n        plural_rule: PluralRule::{rule},\n        messages: &["
    );
    for (key, text) in &singular {
        let _ = writeln!(out, "            ({key:?}, {text:?}),");
    }
    out.push_str("        ],\n        plurals: &[\n");
    for (key, forms) in &plural {
        let _ = writeln!(out, "            ({key:?}, &{forms:?}),");
    }
    out.push_str("        ],\n    },\n");
}

/// The `PluralRule` variant for a PO header's `plural=` expression.
fn plural_rule(expr: &str) -> Option<&'static str> {
    let expr: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let expr = expr.trim_start_matches('(').trim_end_matches(')');
    match expr {
        "n!=1" => Some("NotOne"),
        "n>1" => Some("MoreThanOne"),
        "0" => Some("Single"),
        _ => None,
    }
}
