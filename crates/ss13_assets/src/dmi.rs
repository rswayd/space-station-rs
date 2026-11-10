//! `.dmi` parsing: a PNG sprite sheet plus a `zTXt` "Description" chunk listing icon states.
//!
//! Sheet layout (checked against tg's `tools/dmi` and, empirically, on the animated multi-dir
//! states of the source repo): icons are cut left-to-right, top-to-bottom with
//! `sheet_width / width` columns; states follow each other in Description order, and inside a
//! state the icon for frame `f`, direction index `d` (in `Dir::DMI_ORDER`) is `f * dirs + d`.

use std::io::Cursor;

use thiserror::Error;

pub const DEFAULT_SIZE: u32 = 32;
/// BYOND's delay for frames the Description does not list.
pub const DEFAULT_DELAY: f32 = 1.0;

#[derive(Debug, Error)]
pub enum DmiError {
    #[error("png: {0}")]
    Png(#[from] png::DecodingError),
    #[error("no Description text chunk")]
    NoDescription,
    #[error("line {line}: {msg}")]
    Description { line: usize, msg: String },
    #[error("sheet {sheet_w}x{sheet_h} holds {capacity} icons of {w}x{h}, states need {needed}")]
    SheetTooSmall {
        sheet_w: u32,
        sheet_h: u32,
        w: u32,
        h: u32,
        capacity: u64,
        needed: u64,
    },
}

/// Parsed Description block.
#[derive(Clone, Debug, PartialEq)]
pub struct DmiMeta {
    pub width: u32,
    pub height: u32,
    pub states: Vec<DmiState>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DmiState {
    /// Unescaped; may be empty, and names may repeat (e.g. a `movement` twin of a normal state).
    pub name: String,
    /// 1, 4 or 8.
    pub dirs: u8,
    pub frames: u32,
    /// Exactly `frames` entries when `frames > 1` (missing ones are `DEFAULT_DELAY`, extra ones
    /// dropped); empty for a single frame.
    pub delays: Vec<f32>,
    /// 0 = forever.
    pub loop_count: u32,
    pub rewind: bool,
    pub movement: bool,
    /// `[x, y, first_frame]` (1-based frame) entries, as written.
    pub hotspots: Vec<[i32; 3]>,
    /// Index of this state's first icon in the sheet.
    pub first_icon: u32,
}

impl DmiState {
    pub fn icon_count(&self) -> u32 {
        self.frames * u32::from(self.dirs)
    }

    /// Sheet icon index of frame `frame`, direction index `dir`.
    pub fn icon_index(&self, frame: u32, dir: u8) -> u32 {
        self.first_icon + frame * u32::from(self.dirs) + u32::from(dir)
    }
}

impl DmiMeta {
    pub fn icon_count(&self) -> u64 {
        self.states.iter().map(|s| u64::from(s.icon_count())).sum()
    }
}

/// A decoded `.dmi`: metadata plus the whole sheet as RGBA8.
pub struct Dmi {
    pub meta: DmiMeta,
    pub sheet_width: u32,
    pub sheet_height: u32,
    pub rgba: Vec<u8>,
}

impl Dmi {
    pub fn read(bytes: &[u8]) -> Result<Self, DmiError> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info()?;
        let text = description(reader.info())?.ok_or(DmiError::NoDescription)?;
        let meta = parse_description(&text)?;

        let mut buf = vec![
            0;
            reader
                .output_buffer_size()
                .expect("png size fits in memory")
        ];
        let out = reader.next_frame(&mut buf)?;
        buf.truncate(out.buffer_size());
        let rgba = to_rgba8(buf, out.color_type);
        let (sheet_width, sheet_height) = (out.width, out.height);

        let (w, h) = (meta.width, meta.height);
        let capacity = u64::from(sheet_width / w) * u64::from(sheet_height / h);
        let needed = meta.icon_count();
        if needed > capacity {
            return Err(DmiError::SheetTooSmall {
                sheet_w: sheet_width,
                sheet_h: sheet_height,
                w,
                h,
                capacity,
                needed,
            });
        }
        Ok(Self {
            meta,
            sheet_width,
            sheet_height,
            rgba,
        })
    }

    /// Pixel rect `[x, y, w, h]` of sheet icon `index`.
    pub fn icon_rect(&self, index: u32) -> [u32; 4] {
        let (w, h) = (self.meta.width, self.meta.height);
        let cols = self.sheet_width / w;
        [(index % cols) * w, (index / cols) * h, w, h]
    }

    /// RGBA8 pixels of sheet icon `index`, row by row.
    pub fn icon_pixels(&self, index: u32) -> Vec<u8> {
        let [x, y, w, h] = self.icon_rect(index);
        let stride = self.sheet_width as usize * 4;
        let row = w as usize * 4;
        let mut out = Vec::with_capacity(row * h as usize);
        for r in y..y + h {
            let start = r as usize * stride + x as usize * 4;
            out.extend_from_slice(&self.rgba[start..start + row]);
        }
        out
    }
}

fn description(info: &png::Info) -> Result<Option<String>, DmiError> {
    for c in &info.compressed_latin1_text {
        if c.keyword == "Description" {
            return Ok(Some(relatin1_to_utf8(c.get_text()?)));
        }
    }
    for c in &info.uncompressed_latin1_text {
        if c.keyword == "Description" {
            return Ok(Some(relatin1_to_utf8(c.text.clone())));
        }
    }
    for c in &info.utf8_text {
        if c.keyword == "Description" {
            return Ok(Some(c.get_text()?));
        }
    }
    Ok(None)
}

/// `png` decodes tEXt/zTXt as Latin-1, but BYOND writes state names as raw (UTF-8) bytes.
fn relatin1_to_utf8(s: String) -> String {
    if s.is_ascii() {
        return s;
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    String::from_utf8(bytes).unwrap_or(s)
}

/// A plain PNG file as RGBA8 (an image that is not a `.dmi`, e.g. a nanomap).
pub fn read_png_rgba8(bytes: &[u8]) -> Result<crate::page::PagePixels, DmiError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .expect("png size fits in memory")
    ];
    let out = reader.next_frame(&mut buf)?;
    buf.truncate(out.buffer_size());
    Ok(crate::page::PagePixels {
        width: out.width,
        height: out.height,
        rgba: to_rgba8(buf, out.color_type),
    })
}

fn to_rgba8(buf: Vec<u8>, color: png::ColorType) -> Vec<u8> {
    use png::ColorType::*;
    match color {
        Rgba => buf,
        Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        Indexed => unreachable!("EXPAND turns palettes into RGB(A)"),
    }
}

/// Parses the Description text (`# BEGIN DMI` ... `# END DMI`).
pub fn parse_description(text: &str) -> Result<DmiMeta, DmiError> {
    let mut width = DEFAULT_SIZE;
    let mut height = DEFAULT_SIZE;
    let mut states: Vec<DmiState> = Vec::new();
    // Raw delays are kept until the state's frame count is known.
    let mut raw_delays: Vec<Vec<f32>> = Vec::new();
    let mut begun = false;

    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let err = |msg: String| DmiError::Description { line, msg };
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        if l == "# BEGIN DMI" {
            begun = true;
            continue;
        }
        if l == "# END DMI" {
            break;
        }
        if !begun {
            return Err(err("missing `# BEGIN DMI`".into()));
        }
        let (key, value) = l
            .split_once('=')
            .map(|(k, v)| (k.trim(), v.trim()))
            .ok_or_else(|| err(format!("expected `key = value`, got {l:?}")))?;
        let num = |v: &str| {
            v.parse::<u32>()
                .map_err(|_| err(format!("{key}: bad number {v:?}")))
        };
        if key == "state" {
            states.push(DmiState {
                name: unquote(value).ok_or_else(|| err(format!("bad state name {value:?}")))?,
                dirs: 1,
                frames: 1,
                delays: Vec::new(),
                loop_count: 0,
                rewind: false,
                movement: false,
                hotspots: Vec::new(),
                first_icon: 0,
            });
            raw_delays.push(Vec::new());
            continue;
        }
        let Some(state) = states.last_mut() else {
            match key {
                "version" if value == "4.0" => {}
                "version" => return Err(err(format!("unsupported version {value}"))),
                "width" => width = num(value)?,
                "height" => height = num(value)?,
                _ => return Err(err(format!("unknown header key {key:?}"))),
            }
            continue;
        };
        match key {
            "dirs" => {
                state.dirs = match num(value)? {
                    d @ (1 | 4 | 8) => d as u8,
                    d => return Err(err(format!("dirs must be 1, 4 or 8, got {d}"))),
                }
            }
            "frames" => {
                state.frames = num(value)?;
                if state.frames == 0 {
                    return Err(err("frames = 0".into()));
                }
            }
            "delay" => {
                *raw_delays.last_mut().expect("pushed with state") = value
                    .split(',')
                    .map(|d| {
                        d.trim()
                            .parse::<f32>()
                            .map_err(|_| err(format!("bad delay {d:?}")))
                    })
                    .collect::<Result<_, _>>()?
            }
            "loop" => state.loop_count = num(value)?,
            "rewind" => state.rewind = num(value)? != 0,
            "movement" => state.movement = num(value)? != 0,
            "hotspot" => {
                let v: Vec<i32> = value
                    .split(',')
                    .map(|n| n.trim().parse::<i32>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| err(format!("bad hotspot {value:?}")))?;
                let hs: [i32; 3] = v
                    .try_into()
                    .map_err(|_| err(format!("hotspot needs x,y,frame: {value:?}")))?;
                state.hotspots.push(hs);
            }
            _ => return Err(err(format!("unknown state key {key:?}"))),
        }
    }
    if !begun {
        return Err(DmiError::Description {
            line: 0,
            msg: "missing `# BEGIN DMI`".into(),
        });
    }
    if width == 0 || height == 0 {
        return Err(DmiError::Description {
            line: 0,
            msg: format!("icon size {width}x{height}"),
        });
    }

    let mut next_icon = 0;
    for (state, mut delays) in states.iter_mut().zip(raw_delays) {
        state.first_icon = next_icon;
        next_icon += state.icon_count();
        if state.frames > 1 {
            delays.resize(state.frames as usize, DEFAULT_DELAY);
            state.delays = delays;
        }
    }
    Ok(DmiMeta {
        width,
        height,
        states,
    })
}

/// DM string literal body: `"a\"b\\c"` -> `a"b\c`.
fn unquote(v: &str) -> Option<String> {
    let inner = v.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => out.push('\n'),
                c => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a `.dmi` whose icon `i` is filled with the color `[i, i, i, 255]`.
    pub fn synth_dmi(description: &str, w: u32, h: u32, cols: u32, rows: u32) -> Vec<u8> {
        let (sw, sh) = (w * cols, h * rows);
        let mut rgba = vec![0u8; (sw * sh * 4) as usize];
        for y in 0..sh {
            for x in 0..sw {
                let i = ((y / h) * cols + x / w) as u8;
                let p = ((y * sw + x) * 4) as usize;
                rgba[p..p + 4].copy_from_slice(&[i, i, i, 255]);
            }
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, sw, sh);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_ztxt_chunk("Description".into(), description.into())
            .unwrap();
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&rgba).unwrap();
        writer.finish().unwrap();
        out
    }

    pub const DESC: &str = "# BEGIN DMI
version = 4.0
\twidth = 16
\theight = 24
state = \"\"
\tdirs = 1
\tframes = 1
state = \"walk\"
\tdirs = 4
\tframes = 2
\tdelay = 1.5,2,3
\tloop = 3
\trewind = 1
state = \"walk\"
\tdirs = 4
\tframes = 3
\tdelay = 1
\tmovement = 1
\thotspot = 4,5,1
state = \"q\\\"uote\\\\\"
\tdirs = 8
\tframes = 1
\tdelay = 7
# END DMI
";

    #[test]
    fn parses_metadata_and_edge_cases() {
        let m = parse_description(DESC).unwrap();
        assert_eq!((m.width, m.height), (16, 24));
        let names: Vec<&str> = m.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["", "walk", "walk", "q\"uote\\"]);

        let walk = &m.states[1];
        assert_eq!((walk.dirs, walk.frames, walk.first_icon), (4, 2, 1));
        // Extra delays are dropped.
        assert_eq!(walk.delays, [1.5, 2.0]);
        assert_eq!(
            (walk.loop_count, walk.rewind, walk.movement),
            (3, true, false)
        );

        let mv = &m.states[2];
        assert_eq!(mv.first_icon, 9);
        // Missing delays are padded with BYOND's default.
        assert_eq!(mv.delays, [1.0, 1.0, 1.0]);
        assert!(mv.movement);
        assert_eq!(mv.hotspots, [[4, 5, 1]]);

        // A single frame has no delays even when one is written.
        let q = &m.states[3];
        assert_eq!((q.dirs, q.first_icon), (8, 21));
        assert!(q.delays.is_empty());
        assert_eq!(m.icon_count(), 29);
    }

    #[test]
    fn size_defaults_to_32() {
        let m = parse_description(
            "# BEGIN DMI\nversion = 4.0\nstate = \"a\"\n\tdirs = 1\n\tframes = 1\n# END DMI",
        )
        .unwrap();
        assert_eq!((m.width, m.height), (32, 32));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "version = 4.0",
            "# BEGIN DMI\nversion = 3.0",
            "# BEGIN DMI\nversion = 4.0\nstate = \"a\"\n\tdirs = 2",
            "# BEGIN DMI\nversion = 4.0\nstate = \"a\"\n\tframes = 0",
            "# BEGIN DMI\nversion = 4.0\nstate = a",
            "# BEGIN DMI\nversion = 4.0\nstate = \"a\"\n\tbogus = 1",
        ] {
            assert!(parse_description(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn frame_layout_is_frame_major_row_major() {
        // 1 + 8 + 12 + 8 = 29 icons on a 6-column sheet (cols are not always ceil(sqrt(n))).
        let bytes = synth_dmi(DESC, 16, 24, 6, 5);
        let dmi = Dmi::read(&bytes).unwrap();
        assert_eq!((dmi.sheet_width, dmi.sheet_height), (96, 120));
        let walk = &dmi.meta.states[1];
        // frame 1, dir index 2 (East) -> icon 1 + 1*4 + 2 = 7 -> column 1, row 1.
        let idx = walk.icon_index(1, 2);
        assert_eq!(idx, 7);
        assert_eq!(dmi.icon_rect(idx), [16, 24, 16, 24]);
        let px = dmi.icon_pixels(idx);
        assert_eq!(px.len(), 16 * 24 * 4);
        assert!(px.chunks_exact(4).all(|p| p == [7, 7, 7, 255]));
    }

    #[test]
    fn rejects_sheet_smaller_than_states() {
        let bytes = synth_dmi(DESC, 16, 24, 6, 4);
        assert!(matches!(
            Dmi::read(&bytes),
            Err(DmiError::SheetTooSmall { needed: 29, .. })
        ));
    }
}
