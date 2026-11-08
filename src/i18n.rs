//! Translations. English text is the message id, as in gettext: an
//! untranslated string simply shows in English. Catalogs come from
//! `po/*.po`, compiled into the binary by `build.rs` — nothing to install,
//! every package (and the standalone binary) is translated the same way.
//!
//! Mark strings with `tr("…")`, `trn("one…", "{n}…", n)`,
//! `tr_args("… {name} …", &[("name", &value)])` for placeholders, or
//! `n_("…")` for a constant translated later by `tr()` —
//! `xgettext` finds them by these names (`scripts/update-po.sh`). The
//! language follows the environment like gettext does (`LANGUAGE`,
//! `LC_ALL`, `LC_MESSAGES`, `LANG`).

use std::fmt::Display;
use std::sync::OnceLock;

/// How a language picks a plural form, from the PO header's `plural=`
/// (`build.rs` refuses a rule it doesn't know).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluralRule {
    /// `n != 1` — English, German, Spanish…
    NotOne,
    /// `n > 1` — French, Brazilian Portuguese…
    MoreThanOne,
    /// `0` — a single form (Japanese, Chinese…).
    Single,
}

impl PluralRule {
    fn form(self, n: u64) -> usize {
        match self {
            PluralRule::NotOne => usize::from(n != 1),
            PluralRule::MoreThanOne => usize::from(n > 1),
            PluralRule::Single => 0,
        }
    }
}

/// One language's translations, sorted by message id.
pub struct Catalog {
    pub language: &'static str,
    pub plural_rule: PluralRule,
    pub messages: &'static [(&'static str, &'static str)],
    pub plurals: &'static [(&'static str, &'static [&'static str])],
}

impl Catalog {
    fn get(&self, msgid: &str) -> Option<&'static str> {
        let index = self
            .messages
            .binary_search_by(|(key, _)| (*key).cmp(msgid))
            .ok()?;
        Some(self.messages[index].1)
    }

    fn get_plural(&self, msgid: &str, n: u64) -> Option<&'static str> {
        let index = self
            .plurals
            .binary_search_by(|(key, _)| (*key).cmp(msgid))
            .ok()?;
        let forms = self.plurals[index].1;
        forms.get(self.plural_rule.form(n)).copied()
    }
}

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// The catalog for the user's language, chosen once. Unit tests always
/// get English: they compare text, and run in the developer's locale.
fn active() -> Option<&'static Catalog> {
    if cfg!(test) {
        return None;
    }
    static ACTIVE: OnceLock<Option<usize>> = OnceLock::new();
    let index = *ACTIVE.get_or_init(|| {
        let available: Vec<&str> = CATALOGS.iter().map(|c| c.language).collect();
        pick(
            &preferred_languages(&|name| std::env::var(name).ok()),
            &available,
        )
    });
    index.map(|i| &CATALOGS[i])
}

/// The languages the user asked for, best first, as gettext reads them:
/// the locale from `LC_ALL`, else `LC_MESSAGES`, else `LANG`; `LANGUAGE`
/// (a `:`-separated list) goes before it — unless the locale is `C`/
/// `POSIX`, which means untranslated, whatever `LANGUAGE` says.
fn preferred_languages(env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |name: &str| env(name).filter(|value| !value.is_empty());
    let locale = set("LC_ALL")
        .or_else(|| set("LC_MESSAGES"))
        .or_else(|| set("LANG"));
    let Some(locale) = locale else {
        return Vec::new();
    };
    if locale == "C" || locale == "POSIX" || locale.starts_with("C.") {
        return Vec::new();
    }
    let mut languages: Vec<String> = set("LANGUAGE")
        .map(|list| list.split(':').map(str::to_string).collect())
        .unwrap_or_default();
    languages.push(locale);
    languages
}

/// The first catalog matching one of `wanted` (`fr_CA.UTF-8@euro` tries
/// `fr_CA`, then `fr`), as an index into `available`.
fn pick(wanted: &[String], available: &[&str]) -> Option<usize> {
    for language in wanted {
        let base = language.split(['.', '@']).next().unwrap_or(language);
        let short = base.split('_').next().unwrap_or(base);
        for candidate in [base, short] {
            if let Some(index) = available.iter().position(|a| *a == candidate) {
                return Some(index);
            }
        }
    }
    None
}

/// Marks a string for translation without translating it — gettext's
/// `N_()`: for a `const` or a table, translated later by `tr()` where it's
/// shown (`tr` needs the catalog, chosen at runtime).
pub const fn n_(msgid: &'static str) -> &'static str {
    msgid
}

/// `msgid` in the user's language — `msgid` itself if untranslated.
pub fn tr(msgid: &'static str) -> &'static str {
    active().and_then(|c| c.get(msgid)).unwrap_or(msgid)
}

/// The singular or plural form for `n`, translated; `{n}` in it is
/// replaced by `n`.
pub fn trn(singular: &'static str, plural: &'static str, n: u64) -> String {
    let text = active()
        .and_then(|c| c.get_plural(singular, n))
        .unwrap_or(if n == 1 { singular } else { plural });
    fill(text, &[("n", &n)])
}

/// `msgid` translated, with its `{name}` placeholders filled — named
/// rather than positional, so a translation can reorder them.
pub fn tr_args(msgid: &'static str, args: &[(&str, &dyn Display)]) -> String {
    fill(tr(msgid), args)
}

/// Replaces `{name}` with the matching argument, in a single pass: a value
/// containing `{name}` is never expanded. An unknown `{…}` is kept.
fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after.find('}').and_then(|end| {
            let name = &after[..end];
            args.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (value.to_string(), end))
        });
        match value {
            Some((value, end)) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The `{name}` placeholders of a message, sorted — a translation must
/// keep exactly the same ones.
#[cfg(test)]
fn placeholders(text: &str) -> Vec<&str> {
    let mut names: Vec<&str> = text
        .split('{')
        .skip(1)
        .filter_map(|part| part.split_once('}').map(|(name, _)| name))
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn the_locale_comes_from_lc_all_then_lc_messages_then_lang() {
        let langs = preferred_languages(&env(&[("LANG", "fr_FR.UTF-8")]));
        assert_eq!(langs, ["fr_FR.UTF-8"]);
        let langs = preferred_languages(&env(&[
            ("LANG", "fr_FR.UTF-8"),
            ("LC_MESSAGES", "de_DE.UTF-8"),
        ]));
        assert_eq!(langs, ["de_DE.UTF-8"]);
        let langs = preferred_languages(&env(&[("LANG", "fr_FR"), ("LC_ALL", "es_ES")]));
        assert_eq!(langs, ["es_ES"]);
        // An empty variable counts as unset.
        let langs = preferred_languages(&env(&[("LC_ALL", ""), ("LANG", "fr_FR")]));
        assert_eq!(langs, ["fr_FR"]);
    }

    #[test]
    fn language_goes_first_except_under_the_c_locale() {
        let langs = preferred_languages(&env(&[("LANGUAGE", "pt_BR:fr"), ("LANG", "de_DE")]));
        assert_eq!(langs, ["pt_BR", "fr", "de_DE"]);
        for c in ["C", "POSIX", "C.UTF-8"] {
            let langs = preferred_languages(&env(&[("LANGUAGE", "fr"), ("LANG", c)]));
            assert!(langs.is_empty(), "{c}");
        }
        assert!(preferred_languages(&env(&[("LANGUAGE", "fr")])).is_empty());
    }

    #[test]
    fn a_catalog_is_picked_by_full_then_short_language() {
        let available = ["fr", "pt_BR"];
        let wanted = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(pick(&wanted(&["fr_CA.UTF-8@euro"]), &available), Some(0));
        assert_eq!(pick(&wanted(&["pt_BR.UTF-8"]), &available), Some(1));
        assert_eq!(pick(&wanted(&["pt_PT"]), &available), None);
        assert_eq!(pick(&wanted(&["de", "fr"]), &available), Some(0));
        assert_eq!(pick(&[], &available), None);
    }

    #[test]
    fn plural_forms_follow_the_language_rule() {
        assert_eq!(
            [0, 1, 2].map(|n| PluralRule::NotOne.form(n)),
            [1, 0, 1],
            "English: 0 files"
        );
        assert_eq!(
            [0, 1, 2].map(|n| PluralRule::MoreThanOne.form(n)),
            [0, 0, 1],
            "French: 0 fichier"
        );
        assert_eq!(PluralRule::Single.form(5), 0);
    }

    #[test]
    fn placeholders_are_filled_by_name_in_one_pass() {
        assert_eq!(
            fill("{b} then {a}", &[("a", &1), ("b", &"two")]),
            "two then 1"
        );
        assert_eq!(fill("{x} {unknown} {", &[("x", &"{x}")]), "{x} {unknown} {");
        assert_eq!(placeholders("{b} {a} {a} {} {not a name}"), ["a", "b"]);
    }

    #[test]
    fn every_translation_keeps_its_placeholders() {
        for catalog in CATALOGS {
            for (msgid, text) in catalog.messages {
                assert_eq!(
                    placeholders(msgid),
                    placeholders(text),
                    "{}: {msgid:?} -> {text:?}",
                    catalog.language
                );
            }
            for (msgid, forms) in catalog.plurals {
                for form in *forms {
                    for name in placeholders(form) {
                        assert!(
                            name == "n" || placeholders(msgid).contains(&name),
                            "{}: {msgid:?} plural form {form:?} has {{{name}}}",
                            catalog.language
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn catalogs_are_sorted_for_lookup() {
        for catalog in CATALOGS {
            assert!(catalog.messages.windows(2).all(|w| w[0].0 < w[1].0));
            assert!(catalog.plurals.windows(2).all(|w| w[0].0 < w[1].0));
        }
    }
}
