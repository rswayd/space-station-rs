use std::fmt;
use std::path::PathBuf;

/// One problem in prototype data, with as much location as is known.
/// `file` is `None` for per-instance overrides (the caller knows their origin, e.g. a map file).
#[derive(Debug)]
pub struct ProtoError {
    pub file: Option<PathBuf>,
    /// 1-based line and column (in characters).
    pub pos: Option<(u32, u32)>,
    pub proto: Option<String>,
    pub component: Option<String>,
    pub kind: ErrorKind,
}

impl std::error::Error for ProtoError {}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{}", file.display())?,
            None => f.write_str("<overrides>")?,
        }
        if let Some((line, col)) = self.pos {
            write!(f, ":{line}:{col}")?;
        }
        if let Some(proto) = &self.proto {
            write!(f, ": prototype `{proto}`")?;
        }
        if let Some(component) = &self.component {
            write!(f, ", component `{component}`")?;
        }
        write!(f, ": {}", self.kind)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("cannot read: {0}")]
    Io(#[from] std::io::Error),
    #[error("syntax: {0}")]
    Syntax(String),
    #[error("duplicate prototype id, first defined in {}", .0.display())]
    DuplicateId(PathBuf),
    #[error("unknown parent `{0}`")]
    UnknownParent(String),
    #[error("inheritance cycle: {}", .0.join(" -> "))]
    Cycle(Vec<String>),
    #[error("unknown component (not registered with `register_proto_component`)")]
    UnknownComponent,
    /// Wrong type, unknown field, malformed value.
    #[error("bad value: {0}")]
    BadValue(String),
    /// A field value that does not fully describe its type. `None` when the whole component is.
    #[error(
        "incomplete value{}: overridden values replace the inherited ones and must list every nested field",
        .0.as_ref().map(|f| format!(" for field `{f}`")).unwrap_or_default()
    )]
    Incomplete(Option<String>),
    #[error("unknown prototype")]
    UnknownPrototype,
}

/// Every error found while loading; loading does not stop at the first one.
#[derive(Debug)]
pub struct LoadError(pub Vec<ProtoError>);

impl std::error::Error for LoadError {}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, e) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{e}")?;
        }
        Ok(())
    }
}
