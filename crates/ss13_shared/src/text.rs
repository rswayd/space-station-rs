//! BandaStation's text fixes for Cyrillic (`modular_bandastation/cyrillic_fixes`, the
//! `BANDASTATION EDIT`s of `code/__HELPERS/text.dm`): the Russian keyboard layout in key names,
//! the IC character filter of `sanitize(apply_ic_filter = TRUE)`, and `reject_bad_name` with
//! Cyrillic letters. All of them count characters, never bytes.

/// `GLOB.ru_key_to_en_key`: the key under a Russian letter on a ЙЦУКЕН keyboard.
fn ru_key(c: char) -> Option<char> {
    Some(match c {
        'й' => 'q',
        'ц' => 'w',
        'у' => 'e',
        'к' => 'r',
        'е' => 't',
        'н' => 'y',
        'г' => 'u',
        'ш' => 'i',
        'щ' => 'o',
        'з' => 'p',
        'х' => '[',
        'ъ' => ']',
        'ф' => 'a',
        'ы' => 's',
        'в' => 'd',
        'а' => 'f',
        'п' => 'g',
        'р' => 'h',
        'о' => 'j',
        'л' => 'k',
        'д' => 'l',
        'ж' => ';',
        'э' => '\'',
        'я' => 'z',
        'ч' => 'x',
        'с' => 'c',
        'м' => 'v',
        'и' => 'b',
        'т' => 'n',
        'ь' => 'm',
        'б' => ',',
        'ю' => '.',
        _ => return None,
    })
}

/// `convert_ru_key_to_en_key`: a key name that is one Russian letter (any case) becomes the
/// upper-case English key under it (`"й"` → `"Q"`); anything else is returned as it is.
pub fn ru_key_to_en_key(key: &str) -> String {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c
            .to_lowercase()
            .next()
            .and_then(ru_key)
            .map_or_else(|| key.to_owned(), |en| en.to_ascii_uppercase().to_string()),
        _ => key.to_owned(),
    }
}

/// Whether the class of BandaStation's IC filter keeps `c` (before case folding).
///
/// The DM source writes the class as `a-zA-Za-åa-ö-w-я 0-9/@%"!#?¨'.,:;*+` plus the kana, CJK
/// and Hangul blocks, with the `i` flag. The ranges `a-å`, `a-ö` and `w-я` (a mis-encoded
/// `а-яА-ЯёЁ`) overlap into U+0061..U+044F, the lone `-` is a literal: that is the set the live
/// server keeps, so it is kept as is here, parentheses and dashes `—` included in what is cut.
fn ic_class(c: char) -> bool {
    matches!(c,
        'A'..='Z' | 'a'..='\u{44F}' | '0'..='9' | ' ' | '-' | '/' | '@' | '%' | '"' | '!' | '#'
        | '?' | '\'' | '.' | ',' | ':' | ';' | '*' | '+'
        | '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}')
}

/// [`ic_class`] with the `i` flag: a character whose other case is in the class is kept too
/// (`ё` through `Ё`).
fn ic_allowed(c: char) -> bool {
    fn single(mut it: impl Iterator<Item = char>) -> Option<char> {
        let first = it.next()?;
        it.next().is_none().then_some(first)
    }
    ic_class(c)
        || single(c.to_lowercase()).is_some_and(ic_class)
        || single(c.to_uppercase()).is_some_and(ic_class)
}

/// BandaStation `sanitize(text, apply_ic_filter = TRUE)` without the HTML encoding (chat text is
/// never markup here): tabs and line breaks become `#`, then every character outside the IC
/// class goes (emoji, angle brackets, `&`, `()`, `[]`, `_`, `$`, `—`, `…`, `№`...).
pub fn ic_filter(text: &str) -> String {
    text.chars()
        .map(|c| if matches!(c, '\n' | '\t') { '#' } else { c })
        .filter(|&c| ic_allowed(c))
        .collect()
}

/// The arguments of tg `reject_bad_name`, with its defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameRules {
    /// Numbers and `~|@:#$%&*+` (silicon names).
    pub allow_numbers: bool,
    pub max_length: usize,
    /// Non-ASCII characters other than Cyrillic letters are dropped (or rejected when strict).
    pub ascii_only: bool,
    /// A character that would be dropped rejects the whole name instead.
    pub strict: bool,
    /// `Bob's` becomes `Bob'S`.
    pub cap_after_symbols: bool,
    /// The first letter of each word is capitalized.
    pub cap_at_start: bool,
}

impl Default for NameRules {
    fn default() -> Self {
        Self {
            allow_numbers: false,
            max_length: crate::chat::MAX_NAME_LEN,
            ascii_only: true,
            strict: false,
            cap_after_symbols: true,
            cap_at_start: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    None,
    Spaces,
    Symbols,
    Numbers,
    Letters,
}

/// tg `reject_bad_name` with BandaStation's Cyrillic (`А-Я`, `Ё` upper case, `а-я`, `ё` lower
/// case): `None` for a name that is rejected, else the filtered name with its words
/// capitalized. Characters it does not know are dropped even when strict, as tg's `else
/// continue`.
// shortcut: no `filter_name_ic` (IC word filter config is not loaded); add it with that config.
pub fn reject_bad_name(text: &str, rules: NameRules) -> Option<String> {
    if text.is_empty() || text.chars().count() > 3 * crate::chat::MAX_NAME_LEN {
        return None;
    }
    let mut alphanumeric = 0;
    let mut last = Group::None;
    let mut out = String::new();
    let mut count = 0;
    for c in text.chars() {
        let mut push = c.to_string();
        match c {
            'A'..='Z' | 'А'..='Я' | 'Ё' => {
                alphanumeric += 1;
                last = Group::Letters;
            }
            'a'..='z' | 'а'..='я' | 'ё' => {
                let word_start = matches!(last, Group::None | Group::Spaces) && rules.cap_at_start;
                if word_start || (rules.cap_after_symbols && last == Group::Symbols) {
                    push = c.to_uppercase().collect();
                }
                alphanumeric += 1;
                last = Group::Letters;
            }
            '0'..='9' => {
                if !rules.allow_numbers {
                    if rules.strict {
                        return None;
                    }
                    continue;
                }
                alphanumeric += 1;
                last = Group::Numbers;
            }
            '\'' | '-' | '.' => {
                if last == Group::None {
                    if rules.strict {
                        return None;
                    }
                    continue;
                }
                last = Group::Symbols;
            }
            '~' | '|' | '@' | ':' | '#' | '$' | '%' | '&' | '*' | '+' => {
                if last == Group::None || !rules.allow_numbers {
                    if rules.strict {
                        return None;
                    }
                    continue;
                }
                last = Group::Symbols;
            }
            ' ' => {
                if matches!(last, Group::None | Group::Spaces) {
                    if rules.strict {
                        return None;
                    }
                    continue;
                }
                last = Group::Spaces;
            }
            c if u32::from(c) >= 127 => {
                if rules.ascii_only {
                    if rules.strict {
                        return None;
                    }
                    continue;
                }
                last = Group::Symbols;
            }
            _ => continue,
        }
        out.push_str(&push);
        count += 1;
        if count >= rules.max_length {
            break;
        }
    }
    if alphanumeric < 2 {
        return None;
    }
    if last == Group::Spaces {
        out.pop();
    }
    Some(out)
}

/// tg `sanitize_name` (BandaStation edit): [`reject_bad_name`] strict, then [`ic_filter`];
/// `None` when nothing is left.
pub fn sanitize_name(text: &str, allow_numbers: bool, cap_after_symbols: bool) -> Option<String> {
    let name = reject_bad_name(
        text,
        NameRules {
            allow_numbers,
            strict: true,
            cap_after_symbols,
            ..NameRules::default()
        },
    )?;
    let name = ic_filter(&name);
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_layout_keys_become_english_ones() {
        assert_eq!(ru_key_to_en_key("й"), "Q");
        assert_eq!(ru_key_to_en_key("Ж"), ";");
        assert_eq!(ru_key_to_en_key("ю"), ".");
        assert_eq!(ru_key_to_en_key("Q"), "Q");
        assert_eq!(ru_key_to_en_key("CtrlЙ"), "CtrlЙ");
        assert_eq!(ru_key_to_en_key(""), "");
    }

    #[test]
    fn the_ic_filter_keeps_letters_and_its_punctuation() {
        assert_eq!(ic_filter("Привет, ёжик! Ё-моё?"), "Привет, ёжик! Ё-моё?");
        assert_eq!(
            ic_filter("hello: 100% *шёпот*; \"да\""),
            "hello: 100% *шёпот*; \"да\""
        );
        assert_eq!(
            ic_filter("<b>x</b> (скобки) [кв] a_b & $"),
            "bx/b скобки кв ab  "
        );
        assert_eq!(ic_filter("смайл 😀 — «кавычки»…"), "смайл   «кавычки»");
        assert_eq!(ic_filter("a\tb\nc"), "a#b#c");
        assert_eq!(ic_filter("こんにちは 漢字 한국"), "こんにちは 漢字 한국");
    }

    #[test]
    fn names_take_cyrillic_and_capitalize_words() {
        let name = |t| reject_bad_name(t, NameRules::default());
        assert_eq!(name("вася пупкин").as_deref(), Some("Вася Пупкин"));
        assert_eq!(name("ёжик о'нил").as_deref(), Some("Ёжик О'Нил"));
        assert_eq!(name("  иван  2 ").as_deref(), Some("Иван"));
        assert_eq!(name("я"), None);
        assert_eq!(
            reject_bad_name(&"я".repeat(60), NameRules::default()).map(|n| n.chars().count()),
            Some(crate::chat::MAX_NAME_LEN)
        );
        assert_eq!(sanitize_name("Вася  Пупкин", false, true), None);
        assert_eq!(sanitize_name("R2-D2", false, true), None);
        assert_eq!(sanitize_name("r2-d2", true, true).as_deref(), Some("R2-D2"));
        assert_eq!(
            sanitize_name("иван <b>", false, true).as_deref(),
            Some("Иван B")
        );
    }
}
