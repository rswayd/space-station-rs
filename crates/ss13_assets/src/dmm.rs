//! `.dmm` parsing: TGM and classic formats, prefab dictionary + grid.
//!
//! Both formats share one grammar; TGM only adds line breaks and tabs, which the parser treats as
//! whitespace. Semantics follow tg's runtime loader (`code/modules/mapping/reader.dm`):
//! a grid block `(x,y,z) = {"..."}` lists rows top-down, so its last line is row `y` and its
//! first line is row `y + lines - 1`; every line holds `len / key_len` keys starting at column `x`.
//! Duplicate prefab keys keep the first definition, as BYOND does.

use std::collections::HashMap;
use std::path::Path;

/// Header written by `dmm2tgm.py`; its presence marks the TGM layout.
pub const TGM_HEADER: &str =
    "//MAP CONVERTED BY dmm2tgm.py THIS HEADER COMMENT PREVENTS RECONVERSION, DO NOT REMOVE";

/// Grid value of a cell no block covers.
pub const NO_PREFAB: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmmFormat {
    Tgm,
    Classic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DmmMap {
    pub format: DmmFormat,
    pub key_len: usize,
    /// Highest covered coordinate on each axis; coordinates are 1-based like BYOND.
    pub size: (u32, u32, u32),
    pub prefabs: Vec<Prefab>,
    /// Prefab index per cell, [`NO_PREFAB`] where no block covers it.
    /// Index of 1-based `(x, y, z)` is `((z-1) * size.1 + (y-1)) * size.0 + (x-1)`:
    /// x fastest, y = 1 is the bottom row (BYOND convention).
    pub grid: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Prefab {
    pub key: String,
    /// In file order: movables first, then the turf, then the area.
    pub objects: Vec<DmmObject>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DmmObject {
    pub path: String,
    pub vars: Vec<(String, DmValue)>,
}

/// A constant from a var edit.
#[derive(Debug, Clone, PartialEq)]
pub enum DmValue {
    Null,
    Number(f64),
    /// `\"`, `\\`, `\n`, `\t`, `\[`, `\]`, `\'` are decoded; DM text macros (`\improper`,
    /// `\the`, ...) and unknown escapes stay verbatim with their backslash for the text layer.
    Str(String),
    Path(String),
    /// `'icons/foo.dmi'`, contents verbatim.
    Resource(String),
    /// Entries with an optional associated value. A bare identifier key (`list(NO = "x")`)
    /// is a [`DmValue::Str`], as BYOND makes it.
    List(Vec<(DmValue, Option<DmValue>)>),
    /// Anything else (`newlist(...)`, `icon(...)`, `/obj{...}`, bare identifiers), trimmed source
    /// text. tg's loader turns these into plain text too, so nothing a map can express is lost.
    Raw(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DmmError {
    #[error("line {line}: {msg}")]
    Syntax { line: usize, msg: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not UTF-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

impl DmmMap {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, DmmError> {
        let bytes = std::fs::read(path)?;
        Self::parse(std::str::from_utf8(&bytes)?)
    }

    /// Parses either format. Accepts a UTF-8 BOM and CRLF line endings.
    pub fn parse(text: &str) -> Result<Self, DmmError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        if text.contains('\r') {
            parse_normalized(&text.replace("\r\n", "\n"))
        } else {
            parse_normalized(text)
        }
    }

    /// Prefab index at 1-based BYOND coordinates.
    pub fn index(&self, x: u32, y: u32, z: u32) -> Option<u32> {
        let (sx, sy, sz) = self.size;
        if x == 0 || y == 0 || z == 0 || x > sx || y > sy || z > sz {
            return None;
        }
        let i =
            ((z as usize - 1) * sy as usize + (y as usize - 1)) * sx as usize + (x as usize - 1);
        Some(self.grid[i]).filter(|&p| p != NO_PREFAB)
    }

    /// Prefab at 1-based BYOND coordinates, `(1, 1, 1)` is bottom-left of the first z-level.
    pub fn get(&self, x: u32, y: u32, z: u32) -> Option<&Prefab> {
        self.index(x, y, z).map(|i| &self.prefabs[i as usize])
    }
}

impl DmmObject {
    /// First var edit with this name.
    pub fn var(&self, name: &str) -> Option<&DmValue> {
        self.vars.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

struct Block<'a> {
    x: u32,
    y_top: u32,
    z: u32,
    lines: Vec<&'a str>,
    offset: usize,
}

fn parse_normalized(text: &str) -> Result<DmmMap, DmmError> {
    let format = if text.starts_with(TGM_HEADER) {
        DmmFormat::Tgm
    } else {
        DmmFormat::Classic
    };
    let mut p = Parser {
        s: text,
        b: text.as_bytes(),
        pos: 0,
    };
    let mut prefabs = Vec::new();
    let mut by_key: HashMap<&str, u32> = HashMap::new();
    let mut key_len = 0usize;
    let mut blocks = Vec::new();

    loop {
        p.skip_ws_and_comments();
        match p.peek() {
            None => break,
            Some(b'"') => {
                let start = p.pos;
                p.pos += 1;
                let key = p.take_until(b'"')?;
                if key_len == 0 {
                    key_len = key.len();
                } else if key.len() != key_len {
                    return Err(p.err_at(start, "inconsistent key length"));
                }
                p.skip_ws();
                p.expect(b'=')?;
                p.skip_ws();
                p.expect(b'(')?;
                let objects = p.object_list()?;
                if !by_key.contains_key(key) {
                    by_key.insert(key, prefabs.len() as u32);
                    prefabs.push(Prefab {
                        key: key.to_owned(),
                        objects,
                    });
                }
            }
            Some(b'(') => {
                let offset = p.pos;
                p.pos += 1;
                let x = p.coord()?;
                p.expect(b',')?;
                let y = p.coord()?;
                p.expect(b',')?;
                let z = p.coord()?;
                p.expect(b')')?;
                p.skip_ws();
                p.expect(b'=')?;
                p.skip_ws();
                p.expect(b'{')?;
                p.expect(b'"')?;
                let body = p.take_until_str("\"}")?;
                // Same trimming as tg: drop leading blank lines and one trailing blank line.
                let mut lines: Vec<&str> = body.split('\n').skip_while(|l| l.is_empty()).collect();
                if lines.last() == Some(&"") {
                    lines.pop();
                }
                if lines.is_empty() {
                    continue;
                }
                let y_top = y + lines.len() as u32 - 1;
                blocks.push(Block {
                    x,
                    y_top,
                    z,
                    lines,
                    offset,
                });
            }
            Some(_) => return Err(p.err("expected a prefab or a grid block")),
        }
    }

    let (mut sx, mut sy, mut sz) = (0u32, 0u32, 0u32);
    for b in &blocks {
        if key_len == 0 {
            return Err(p.err_at(b.offset, "grid block before any prefab"));
        }
        for line in &b.lines {
            if line.len() % key_len != 0 {
                return Err(p.err_at(b.offset, "grid line length is not a multiple of key length"));
            }
            sx = sx.max(b.x + (line.len() / key_len) as u32 - 1);
        }
        sy = sy.max(b.y_top);
        sz = sz.max(b.z);
    }

    let mut grid = vec![NO_PREFAB; sx as usize * sy as usize * sz as usize];
    for b in &blocks {
        for (i, line) in b.lines.iter().enumerate() {
            let y = b.y_top - i as u32;
            let row = ((b.z as usize - 1) * sy as usize + (y as usize - 1)) * sx as usize;
            for (j, key) in line.as_bytes().chunks_exact(key_len).enumerate() {
                // A key split inside a multi-byte char cannot match any prefab key.
                let key = std::str::from_utf8(key).unwrap_or("");
                let Some(&idx) = by_key.get(key) else {
                    return Err(p.err_at(b.offset, &format!("unknown prefab key {key:?}")));
                };
                grid[row + b.x as usize - 1 + j] = idx;
            }
        }
    }

    Ok(DmmMap {
        format,
        key_len,
        size: (sx, sy, sz),
        prefabs,
        grid,
    })
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn err_at(&self, pos: usize, msg: &str) -> DmmError {
        let line = self.b[..pos.min(self.b.len())]
            .iter()
            .filter(|&&c| c == b'\n')
            .count()
            + 1;
        DmmError::Syntax {
            line,
            msg: msg.to_owned(),
        }
    }

    fn err(&self, msg: &str) -> DmmError {
        self.err_at(self.pos, msg)
    }

    fn expect(&mut self, c: u8) -> Result<(), DmmError> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            self.skip_ws();
            if self.b[self.pos..].starts_with(b"//") {
                while !matches!(self.peek(), None | Some(b'\n')) {
                    self.pos += 1;
                }
            } else {
                return;
            }
        }
    }

    /// Text up to (excluding) `end`, consuming `end`.
    fn take_until(&mut self, end: u8) -> Result<&'a str, DmmError> {
        let start = self.pos;
        let Some(n) = self.b[start..].iter().position(|&c| c == end) else {
            return Err(self.err(&format!("unterminated, expected '{}'", end as char)));
        };
        self.pos = start + n + 1;
        Ok(&self.s[start..start + n])
    }

    fn take_until_str(&mut self, end: &str) -> Result<&'a str, DmmError> {
        let start = self.pos;
        let Some(n) = self.s[start..].find(end) else {
            return Err(self.err(&format!("unterminated, expected {end:?}")));
        };
        self.pos = start + n + end.len();
        Ok(&self.s[start..start + n])
    }

    fn coord(&mut self) -> Result<u32, DmmError> {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let n: u32 = self.s[start..self.pos]
            .parse()
            .map_err(|_| self.err("expected a coordinate"))?;
        self.skip_ws();
        if n == 0 {
            return Err(self.err_at(start, "coordinates are 1-based"));
        }
        Ok(n)
    }

    /// After `(`: `/path{vars}, /path, ...)`.
    fn object_list(&mut self) -> Result<Vec<DmmObject>, DmmError> {
        let mut objects = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(b')') && objects.is_empty() {
                self.pos += 1;
                return Ok(objects);
            }
            if self.peek() != Some(b'/') {
                return Err(self.err("expected a type path"));
            }
            let start = self.pos;
            while !matches!(
                self.peek(),
                None | Some(b'{' | b',' | b')' | b' ' | b'\t' | b'\n' | b'\r')
            ) {
                self.pos += 1;
            }
            let path = self.s[start..self.pos].to_owned();
            self.skip_ws();
            let mut vars = Vec::new();
            if self.peek() == Some(b'{') {
                self.pos += 1;
                loop {
                    self.skip_ws();
                    match self.peek() {
                        Some(b'}') => {
                            self.pos += 1;
                            break;
                        }
                        Some(b';') => {
                            self.pos += 1;
                            continue;
                        }
                        _ => {}
                    }
                    let name_start = self.pos;
                    while matches!(self.peek(), Some(c) if c == b'_' || c.is_ascii_alphanumeric()) {
                        self.pos += 1;
                    }
                    if name_start == self.pos {
                        return Err(self.err("expected a var name"));
                    }
                    let name = self.s[name_start..self.pos].to_owned();
                    self.skip_ws();
                    self.expect(b'=')?;
                    self.skip_ws();
                    let value = self.value()?;
                    vars.push((name, value));
                    self.skip_ws();
                    if !matches!(self.peek(), Some(b';' | b'}')) {
                        return Err(self.err("expected ';' or '}' after a var"));
                    }
                }
                self.skip_ws();
            }
            objects.push(DmmObject { path, vars });
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b')') => {
                    self.pos += 1;
                    return Ok(objects);
                }
                _ => return Err(self.err("expected ',' or ')' after an object")),
            }
        }
    }

    fn value(&mut self) -> Result<DmValue, DmmError> {
        let start = self.pos;
        match self.peek() {
            Some(b'"') => {
                self.pos += 1;
                self.string(false)
            }
            Some(b'{') if self.b.get(self.pos + 1) == Some(&b'"') => {
                self.pos += 2;
                self.string(true)
            }
            Some(b'\'') => {
                self.pos += 1;
                Ok(DmValue::Resource(self.take_until(b'\'')?.to_owned()))
            }
            _ if self.b[self.pos..].starts_with(b"list(") => {
                self.pos += 5;
                self.list()
            }
            Some(b'/') => {
                while matches!(self.peek(), Some(c) if c == b'_' || c == b'/' || c.is_ascii_alphanumeric())
                {
                    self.pos += 1;
                }
                if self.at_value_end() {
                    Ok(DmValue::Path(self.s[start..self.pos].to_owned()))
                } else {
                    self.raw(start)
                }
            }
            Some(b'-' | b'.' | b'0'..=b'9') => {
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-' | b'#'))
                {
                    self.pos += 1;
                }
                match parse_number(&self.s[start..self.pos]) {
                    Some(n) if self.at_value_end() => Ok(DmValue::Number(n)),
                    _ => self.raw(start),
                }
            }
            _ => match self.raw(start)? {
                DmValue::Raw(r) if r == "null" => Ok(DmValue::Null),
                v => Ok(v),
            },
        }
    }

    fn at_value_end(&self) -> bool {
        let rest = &self.b[self.pos..];
        let i = rest
            .iter()
            .position(|&c| c != b' ' && c != b'\t')
            .unwrap_or(rest.len());
        matches!(
            rest.get(i),
            None | Some(b';' | b',' | b')' | b'}' | b'=' | b'\n')
        )
    }

    /// Source text from `start` to the next top-level `; , ) } =` or newline, balancing brackets
    /// and skipping quoted text.
    fn raw(&mut self, start: usize) -> Result<DmValue, DmmError> {
        self.pos = start;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            match c {
                b'"' | b'\'' => {
                    self.pos += 1;
                    self.skip_quoted(c)?;
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' if depth > 0 => depth -= 1,
                b';' | b',' | b')' | b'}' | b'=' | b'\n' if depth == 0 => break,
                _ => {}
            }
            self.pos += 1;
        }
        let text = self.s[start..self.pos].trim();
        if text.is_empty() {
            return Err(self.err_at(start, "expected a value"));
        }
        Ok(DmValue::Raw(text.to_owned()))
    }

    fn skip_quoted(&mut self, quote: u8) -> Result<(), DmmError> {
        while let Some(c) = self.peek() {
            self.pos += if c == b'\\' { 2 } else { 1 };
            if c == quote {
                return Ok(());
            }
        }
        Err(self.err("unterminated quoted text"))
    }

    /// After the opening quote; `braced` means a `{"..."}` string.
    fn string(&mut self, braced: bool) -> Result<DmValue, DmmError> {
        let start = self.pos;
        let mut out = String::new();
        let mut run = self.pos;
        loop {
            let Some(c) = self.peek() else {
                return Err(self.err_at(start, "unterminated string"));
            };
            match c {
                b'"' if !braced || self.b.get(self.pos + 1) == Some(&b'}') => {
                    out.push_str(&self.s[run..self.pos]);
                    self.pos += if braced { 2 } else { 1 };
                    return Ok(DmValue::Str(out));
                }
                b'\\' => {
                    out.push_str(&self.s[run..self.pos]);
                    self.pos += 1;
                    let rest = &self.b[self.pos..];
                    let word_len = rest
                        .iter()
                        .position(|c| !c.is_ascii_alphabetic())
                        .unwrap_or(rest.len());
                    let word = &self.s[self.pos..self.pos + word_len];
                    if TEXT_MACROS.contains(&word) {
                        out.push('\\');
                        out.push_str(word);
                        self.pos += word_len;
                    } else {
                        match self.peek() {
                            Some(b'n') => out.push('\n'),
                            Some(b't') => out.push('\t'),
                            Some(e @ (b'"' | b'\\' | b'[' | b']' | b'\'')) => out.push(e as char),
                            // Unknown escape: keep the backslash, the next char stays in the run
                            // (it may be multi-byte).
                            Some(_) => {
                                out.push('\\');
                                run = self.pos;
                                continue;
                            }
                            None => return Err(self.err_at(start, "unterminated string")),
                        }
                        self.pos += 1;
                    }
                    run = self.pos;
                }
                _ => self.pos += 1,
            }
        }
    }

    /// After `list(`.
    fn list(&mut self) -> Result<DmValue, DmmError> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b')') => {
                    self.pos += 1;
                    return Ok(DmValue::List(items));
                }
                Some(b',') => {
                    self.pos += 1;
                    continue;
                }
                None => return Err(self.err("unterminated list")),
                _ => {}
            }
            let mut key = self.value()?;
            self.skip_ws();
            let assoc = if self.peek() == Some(b'=') {
                self.pos += 1;
                self.skip_ws();
                if let DmValue::Raw(ident) = &key
                    && is_ident(ident)
                {
                    key = DmValue::Str(ident.clone());
                }
                Some(self.value()?)
            } else {
                None
            };
            items.push((key, assoc));
            self.skip_ws();
            if !matches!(self.peek(), Some(b',' | b')')) {
                return Err(self.err("expected ',' or ')' in list"));
            }
        }
    }
}

fn is_ident(s: &str) -> bool {
    !s.starts_with(|c: char| c.is_ascii_digit())
        && s.bytes().all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

fn parse_number(text: &str) -> Option<f64> {
    match text {
        "1.#INF" => Some(f64::INFINITY),
        "-1.#INF" => Some(f64::NEG_INFINITY),
        // Rust also parses "inf"/"nan" spellings that DM never writes as numbers.
        _ if text
            .bytes()
            .any(|c| c.is_ascii_alphabetic() && c != b'e' && c != b'E') =>
        {
            None
        }
        _ => text.parse().ok(),
    }
}

/// DM text macros; kept verbatim so a later text layer can apply them.
const TEXT_MACROS: &[&str] = &[
    "the", "The", "a", "an", "A", "An", "he", "He", "she", "She", "his", "His", "hers", "Hers",
    "him", "Him", "himself", "Himself", "herself", "Herself", "its", "Its", "itself", "Itself",
    "proper", "improper", "th", "s", "icon", "ref", "roman", "Roman", "b", "i", "u", "bold",
    "italic", "font", "red", "blue", "green", "black", "white", "yellow", "x", "X",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> DmValue {
        DmValue::Str(v.into())
    }

    const TGM: &str = r#"//MAP CONVERTED BY dmm2tgm.py THIS HEADER COMMENT PREVENTS RECONVERSION, DO NOT REMOVE
"a" = (
/turf/open/space/basic,
/area/space)
"b" = (
/obj/machinery/camera/directional/east{
	c_tag = "Quote \"x\" back\\slash [not expr] \[esc\] semi; comma, paren) brace}";
	network = list("ss13","rd");
	pixel_x = -3;
	dir = 4
	},
/obj/item/paper{
	name = "\improper Nanotrasen\nmemo";
	icon = 'icons/obj/paper.dmi';
	spawn_loot = list(/obj/item/a = 10, /obj/item/b=90);
	assoc = list(NO="LITTERING", "2" = "Lower    Deck", "head"=null);
	nested = list(list(1, 2), "x" = list("y" = 3e+007));
	empty = list();
	stuff = newlist(/obj/item/c{name = "q, (r)"});
	gas = /datum/gas/nitrogen;
	nothing = null;
	temp = 273.15
	},
/turf/open/floor/iron,
/area/station/hallway)

(1,1,1) = {"
a
a
b
"}
(2,1,1) = {"
b
a
a
"}
"#;

    #[test]
    fn tgm_fixture() {
        let m = DmmMap::parse(TGM).unwrap();
        assert_eq!(m.format, DmmFormat::Tgm);
        assert_eq!(m.key_len, 1);
        assert_eq!(m.size, (2, 3, 1));
        assert_eq!(m.prefabs.len(), 2);
        // Last line of a block is y = 1.
        assert_eq!(m.get(1, 1, 1).unwrap().key, "b");
        assert_eq!(m.get(1, 3, 1).unwrap().key, "a");
        assert_eq!(m.get(2, 3, 1).unwrap().key, "b");
        assert_eq!(m.get(3, 1, 1), None);
        assert_eq!(m.get(0, 1, 1), None);

        let b = &m.prefabs[1];
        let paths: Vec<_> = b.objects.iter().map(|o| o.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/obj/machinery/camera/directional/east",
                "/obj/item/paper",
                "/turf/open/floor/iron",
                "/area/station/hallway"
            ]
        );
        let cam = &b.objects[0];
        assert_eq!(
            cam.var("c_tag"),
            Some(&s(
                "Quote \"x\" back\\slash [not expr] [esc] semi; comma, paren) brace}"
            ))
        );
        assert_eq!(
            cam.var("network"),
            Some(&DmValue::List(vec![(s("ss13"), None), (s("rd"), None)]))
        );
        assert_eq!(cam.var("pixel_x"), Some(&DmValue::Number(-3.0)));
        assert_eq!(cam.var("dir"), Some(&DmValue::Number(4.0)));

        let paper = &b.objects[1];
        assert_eq!(paper.var("name"), Some(&s("\\improper Nanotrasen\nmemo")));
        assert_eq!(
            paper.var("icon"),
            Some(&DmValue::Resource("icons/obj/paper.dmi".into()))
        );
        assert_eq!(
            paper.var("spawn_loot"),
            Some(&DmValue::List(vec![
                (
                    DmValue::Path("/obj/item/a".into()),
                    Some(DmValue::Number(10.0))
                ),
                (
                    DmValue::Path("/obj/item/b".into()),
                    Some(DmValue::Number(90.0))
                ),
            ]))
        );
        assert_eq!(
            paper.var("assoc"),
            Some(&DmValue::List(vec![
                (s("NO"), Some(s("LITTERING"))),
                (s("2"), Some(s("Lower    Deck"))),
                (s("head"), Some(DmValue::Null)),
            ]))
        );
        assert_eq!(
            paper.var("nested"),
            Some(&DmValue::List(vec![
                (
                    DmValue::List(vec![
                        (DmValue::Number(1.0), None),
                        (DmValue::Number(2.0), None)
                    ]),
                    None
                ),
                (
                    s("x"),
                    Some(DmValue::List(vec![(s("y"), Some(DmValue::Number(3e7)))]))
                ),
            ]))
        );
        assert_eq!(paper.var("empty"), Some(&DmValue::List(vec![])));
        assert_eq!(
            paper.var("stuff"),
            Some(&DmValue::Raw(
                "newlist(/obj/item/c{name = \"q, (r)\"})".into()
            ))
        );
        assert_eq!(
            paper.var("gas"),
            Some(&DmValue::Path("/datum/gas/nitrogen".into()))
        );
        assert_eq!(paper.var("nothing"), Some(&DmValue::Null));
        assert_eq!(paper.var("temp"), Some(&DmValue::Number(273.15)));
    }

    #[test]
    fn classic_fixture_crlf_bom() {
        let src = "\u{feff}\"aa\" = (/turf/space,/area/space)\r\n\
            \"ab\" = (/obj/structure/sign{desc = \"A \\\"sign\\\"\"; name = {\"multi\r\nline\"}},/turf/simulated/floor{icon_state = \"x\"},/area/hall)\r\n\
            \"aa\" = (/turf/ignored,/area/ignored)\r\n\
            \r\n\
            (1,1,1) = {\"\r\naaabaa\r\nababab\r\n\"}\r\n\
            (1,1,2) = {\"\r\nabaa\r\n\"}\r\n";
        let m = DmmMap::parse(src).unwrap();
        assert_eq!(m.format, DmmFormat::Classic);
        assert_eq!(m.key_len, 2);
        assert_eq!(m.size, (3, 2, 2));
        assert_eq!(
            m.prefabs.len(),
            2,
            "duplicate key keeps the first definition"
        );
        assert_eq!(m.prefabs[0].objects[0].path, "/turf/space");
        // Top line is y = 2.
        let keys: Vec<_> = [
            (1, 2, 1),
            (2, 2, 1),
            (3, 2, 1),
            (1, 1, 1),
            (2, 1, 1),
            (3, 1, 1),
        ]
        .iter()
        .map(|&(x, y, z)| m.get(x, y, z).unwrap().key.as_str())
        .collect();
        assert_eq!(keys, ["aa", "ab", "aa", "ab", "ab", "ab"]);
        assert_eq!(m.get(2, 1, 2).unwrap().key, "aa");
        assert_eq!(m.get(3, 1, 2), None);
        assert_eq!(m.get(1, 2, 2), None);
        let sign = &m.prefabs[1].objects[0];
        assert_eq!(sign.var("desc"), Some(&s("A \"sign\"")));
        assert_eq!(sign.var("name"), Some(&s("multi\nline")));
        assert_eq!(m.prefabs[1].objects[1].var("icon_state"), Some(&s("x")));
    }

    #[test]
    fn errors() {
        let bad_key = "\"a\" = (/turf,/area)\n(1,1,1) = {\"\nz\n\"}\n";
        assert!(matches!(
            DmmMap::parse(bad_key),
            Err(DmmError::Syntax { line: 2, .. })
        ));
        let mixed_len = "\"a\" = (/turf,/area)\n\"bb\" = (/turf,/area)\n";
        assert!(DmmMap::parse(mixed_len).is_err());
        let unterminated = "\"a\" = (/turf{name = \"x},/area)\n";
        assert!(DmmMap::parse(unterminated).is_err());
    }
}
