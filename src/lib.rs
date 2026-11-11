use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs;
use std::path::Path;

use lex_just_parse::lexer::*;
use lex_just_parse::parser::{Parser, RefLexer, many1, sep_by};
use lex_just_parse::try_parse;

pub mod diff;
pub use diff::unified_diff;
pub mod query;
pub use query::*;
#[cfg(feature = "watch")]
pub mod watch;
#[cfg(feature = "watch")]
pub use watch::*;
#[cfg(feature = "lazy")]
pub mod lazy;

/// Formatting configuration options for GSS pretty printing.
///
/// Inspired by formatting options in `rustfmt`, `prettier`, and `taplo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Number of spaces per indentation level (default: 4).
    pub indent_width: usize,
    /// Whether to use tabs for indentation instead of spaces (default: false).
    pub use_tabs: bool,
    /// Maximum target print width for inline arrays before wrapping (default: 80).
    pub print_width: usize,
    /// Whether to allow key redefinition during parsing (default: false).
    pub allow_redefinition: bool,
    /// Whether to insert blank lines between top-level multiline blocks (default: false).
    pub separate_blocks: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_width: 4,
            use_tabs: false,
            print_width: 80,
            allow_redefinition: false,
            separate_blocks: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(u32),
    SignedNumber(i32),
    Float(f32),
    Bool(bool),
    String(String),
    Object(Object),
    Expr(Expr),
    Vec(Vec<Value>),
}

impl Value {
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        let any: &dyn std::any::Any = match self {
            Value::Number(x) => x,
            Value::SignedNumber(x) => x,
            Value::Float(x) => x,
            Value::Bool(x) => x,
            Value::String(x) => x,
            Value::Object(x) => x,
            Value::Expr(x) => x,
            Value::Vec(x) => x,
        };
        any.downcast_ref::<T>()
    }

    pub fn as_number(&self) -> Option<&u32> {
        if let Self::Number(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_signed_number(&self) -> Option<&i32> {
        if let Self::SignedNumber(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<&f32> {
        if let Self::Float(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        if let Self::Bool(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_string(&self) -> Option<&String> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&Object> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_expr(&self) -> Option<&Expr> {
        if let Self::Expr(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_vec(&self) -> Option<&Vec<Value>> {
        if let Self::Vec(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

pub type Percent = f32;
pub type Gss = Object;

pub trait FromGssValue: Sized {
    fn from_gss_value(value: &Value) -> Option<Self>;
}

macro_rules! impl_from_gss_for_int {
    ($($t:ty),*) => {
        $(
            impl FromGssValue for $t {
                fn from_gss_value(value: &Value) -> Option<Self> {
                    if let Some(x) = value.as_number() {
                        <$t>::try_from(*x).ok()
                    } else if let Some(x) = value.as_signed_number() {
                        <$t>::try_from(*x).ok()
                    } else if let Some(x) = value.as_float() {
                        if x.fract() == 0.0 && *x >= (<$t>::MIN as f32) && *x <= (<$t>::MAX as f32) {
                            Some(*x as $t)
                        } else {
                            None
                        }
                    } else if let Some(x) = value.downcast_ref::<$t>() {
                        Some(*x)
                    } else {
                        None
                    }
                }
            }
        )*
    };
}

impl_from_gss_for_int!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

impl FromGssValue for f32 {
    fn from_gss_value(value: &Value) -> Option<Self> {
        if let Some(x) = value.as_float() {
            Some(*x)
        } else if let Some(x) = value.as_number() {
            Some(*x as f32)
        } else {
            value.as_signed_number().map(|x| *x as f32)
        }
    }
}

impl FromGssValue for f64 {
    fn from_gss_value(value: &Value) -> Option<Self> {
        if let Some(x) = value.as_float() {
            Some(*x as f64)
        } else if let Some(x) = value.as_number() {
            Some(*x as f64)
        } else if let Some(x) = value.as_signed_number() {
            Some(*x as f64)
        } else {
            value.downcast_ref::<f64>().copied()
        }
    }
}

impl FromGssValue for bool {
    fn from_gss_value(value: &Value) -> Option<Self> {
        value.as_bool().copied()
    }
}

impl FromGssValue for String {
    fn from_gss_value(value: &Value) -> Option<Self> {
        value.as_string().cloned()
    }
}
impl<T: FromGssValue> FromGssValue for Vec<T> {
    fn from_gss_value(value: &Value) -> Option<Self> {
        value.as_vec()?.iter().map(T::from_gss_value).collect()
    }
}

impl FromGssValue for Value {
    fn from_gss_value(value: &Value) -> Option<Self> {
        Some(value.clone())
    }
}

pub trait ToGssValue {
    fn to_gss_value(value: &Self) -> Value;
}

macro_rules! impl_to_gss {
    ($($v:expr => $t:ty),*) => {
        $(
            impl ToGssValue for $t {
                fn to_gss_value(value: &Self) -> Value {
                    $v(*value as _)
                }
            }
        )*
    };
}

impl_to_gss! {
    Value::Number => u8,
    Value::Number => u16,
    Value::Number => u32,
    Value::Number => u64,
    Value::Number => u128,
    Value::Number => usize,
    Value::SignedNumber => i8,
    Value::SignedNumber => i16,
    Value::SignedNumber => i32,
    Value::SignedNumber => i64,
    Value::SignedNumber => i128,
    Value::SignedNumber => isize,
    Value::Float => f32,
    Value::Float => f64,
    Value::Bool => bool
}

impl ToGssValue for String {
    fn to_gss_value(value: &Self) -> Value {
        Value::String(value.clone())
    }
}

impl ToGssValue for Vec<Value> {
    fn to_gss_value(value: &Self) -> Value {
        Value::Vec(value.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    inner: HashMap<String, Value>,
    max_depth: usize,
    allow_redefinition: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Symbol(String),
    Access(Vec<String>),
    RelAccess(Vec<String>),
}

impl Object {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            max_depth: 20,
            allow_redefinition: false,
        }
    }

    pub fn with_allow_redefinition(mut self, allow: bool) -> Self {
        self.allow_redefinition = allow;
        self
    }

    pub fn set_allow_redefinition(&mut self, allow: bool) {
        self.allow_redefinition = allow;
    }

    pub fn allow_redefinition(&self) -> bool {
        self.allow_redefinition
    }
}

impl Default for Object {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Symbol(s) => write!(f, "{s}"),
            Expr::Access(seq) => write!(f, "{}", seq.join(".")),
            Expr::RelAccess(seq) => write!(f, ".{}", seq.join(".")),
        }
    }
}

fn escape_string_into(s: &str, out: &mut String) {
    out.reserve(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
}

#[inline]
fn write_indent(level: usize, opts: &FormatOptions, out: &mut String) {
    if opts.use_tabs {
        for _ in 0..level {
            out.push('\t');
        }
    } else {
        let count = level * opts.indent_width;
        for _ in 0..count {
            out.push(' ');
        }
    }
}

fn is_definitely_multiline(v: &Value) -> bool {
    match v {
        Value::Object(obj) => !obj.inner.is_empty(),
        Value::Vec(vec) => vec.iter().any(is_definitely_multiline),
        _ => false,
    }
}

fn try_format_vec_inline(vec: &[Value], level: usize, opts: &FormatOptions) -> Option<String> {
    for item in vec {
        if is_definitely_multiline(item) {
            return None;
        }
    }
    let mut inline = String::new();
    inline.push('[');
    for (i, item) in vec.iter().enumerate() {
        if i > 0 {
            inline.push_str(", ");
        }
        format_value_into(item, level, opts, &mut inline);
        if inline.contains('\n') {
            return None;
        }
    }
    inline.push(']');

    let indent_cols = if opts.use_tabs {
        level * 4
    } else {
        level * opts.indent_width
    };
    if indent_cols + inline.len() <= opts.print_width {
        Some(inline)
    } else {
        None
    }
}

fn format_vec_into(vec: &[Value], level: usize, opts: &FormatOptions, out: &mut String) {
    if vec.is_empty() {
        out.push_str("[]");
        return;
    }

    if let Some(inline) = try_format_vec_inline(vec, level, opts) {
        out.push_str(&inline);
    } else {
        out.push_str("[\n");
        for item in vec {
            write_indent(level + 1, opts, out);
            format_value_into(item, level + 1, opts, out);
            out.push_str(",\n");
        }
        write_indent(level, opts, out);
        out.push(']');
    }
}

fn is_multiline_value(v: &Value) -> bool {
    match v {
        Value::Object(obj) => !obj.inner.is_empty(),
        Value::Vec(vec) => {
            !vec.is_empty() && (vec.len() > 3 || vec.iter().any(is_definitely_multiline))
        }
        _ => false,
    }
}

fn format_object_into(obj: &Object, level: usize, opts: &FormatOptions, out: &mut String) {
    if obj.inner.is_empty() {
        if level > 0 {
            out.push_str("{}");
        }
        return;
    }

    let mut entries: Vec<(&str, &Value)> = obj.inner.iter().map(|(k, v)| (k.as_str(), v)).collect();
    entries.sort_unstable_by_key(|&(k, _)| k);

    if level == 0 {
        for (i, (k, v)) in entries.iter().enumerate() {
            if opts.separate_blocks
                && i > 0
                && (is_multiline_value(v) || is_multiline_value(entries[i - 1].1))
            {
                out.push('\n');
            }
            out.push_str(k);
            out.push_str(" = ");
            format_value_into(v, 0, opts, out);
            out.push_str(",\n");
        }
    } else {
        out.push_str("{\n");
        for (k, v) in entries {
            write_indent(level, opts, out);
            out.push_str(k);
            out.push_str(" = ");
            format_value_into(v, level, opts, out);
            out.push_str(",\n");
        }
        write_indent(level.saturating_sub(1), opts, out);
        out.push('}');
    }
}

fn format_value_into(v: &Value, level: usize, opts: &FormatOptions, out: &mut String) {
    use std::fmt::Write;

    match v {
        Value::String(string) => escape_string_into(string, out),
        Value::Number(i) => {
            let _ = write!(out, "{i}");
        }
        Value::SignedNumber(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => {
            let _ = write!(out, "{f}");
        }
        Value::Bool(b) => {
            let _ = write!(out, "{b}");
        }
        Value::Object(obj) => {
            format_object_into(obj, level + 1, opts, out);
        }
        Value::Expr(expr) => {
            let _ = write!(out, "{expr}");
        }
        Value::Vec(vec) => {
            format_vec_into(vec, level, opts, out);
        }
    }
}

pub fn format_value(v: &Value, level: usize) -> String {
    format_value_with_options(v, level, &FormatOptions::default())
}

pub fn format_value_with_options(v: &Value, level: usize, opts: &FormatOptions) -> String {
    let mut out = String::new();
    format_value_into(v, level, opts, &mut out);
    out
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_with_level(0))
    }
}

#[inline]
fn with_str_slice<R>(seq: &[String], f: impl FnOnce(&[&str]) -> R) -> R {
    if seq.len() <= 8 {
        let mut buf = [""; 8];
        for (i, s) in seq.iter().enumerate() {
            buf[i] = s.as_str();
        }
        f(&buf[..seq.len()])
    } else {
        let tmp: Vec<&str> = seq.iter().map(AsRef::as_ref).collect();
        f(&tmp)
    }
}

impl Object {
    pub fn format_with_level(&self, level: usize) -> String {
        self.format_with_options(level, &FormatOptions::default())
    }

    pub fn format_with_options(&self, level: usize, options: &FormatOptions) -> String {
        let mut out = String::with_capacity(self.inner.len() * 32);
        format_object_into(self, level, options, &mut out);
        out
    }

    pub fn to_gss_string(&self) -> String {
        self.format_with_level(0)
    }

    pub fn to_gss_string_with_options(&self, options: &FormatOptions) -> String {
        self.format_with_options(0, options)
    }

    pub fn dump(&self, level: usize) {
        if level == 0 {
            print!("{}", self.format_with_level(0));
        } else {
            println!("{}", self.format_with_level(level));
        }
    }

    pub fn get_fields(&self) -> std::collections::hash_map::Keys<'_, String, Value> {
        self.inner.keys()
    }

    pub fn get<T: 'static>(&self, path: &[&str]) -> Option<&T> {
        self.get_value_impl(path, 0, self.max_depth)
            .and_then(|v| v.downcast_ref::<T>())
    }

    pub fn get_as<T: FromGssValue>(&self, path: &[&str]) -> Option<T> {
        self.get_value_impl(path, 0, self.max_depth)
            .and_then(T::from_gss_value)
    }

    pub fn get_or_default<T: FromGssValue + Default>(&self, path: &[&str]) -> T {
        self.get_as::<T>(path).unwrap_or_default()
    }

    pub fn get_or<T: FromGssValue>(&self, path: &[&str], default: T) -> T {
        self.get_as::<T>(path).unwrap_or(default)
    }

    pub fn get_value(&self, path: &[&str]) -> Option<&Value> {
        self.get_value_impl(path, 0, self.max_depth)
    }

    fn get_value_impl<'a>(
        &'a self,
        path: &[&str],
        current_depth: usize,
        max_depth: usize,
    ) -> Option<&'a Value> {
        if current_depth >= max_depth {
            return None;
        }
        let mut obj = self;
        if let Some((last, prefix)) = path.split_last() {
            for c in prefix {
                {
                    let v = obj.inner.get(*c)?;
                    if let Some(o) = v.as_object() {
                        obj = o;
                    } else {
                        let expr = v.as_expr()?;
                        obj = match expr {
                            Expr::Symbol(s) => {
                                let val = self.get_value_impl(
                                    &[s.as_str()],
                                    current_depth + 1,
                                    max_depth,
                                )?;
                                val.as_object()?
                            }
                            Expr::Access(seq) => {
                                let val = with_str_slice(seq, |tmp| {
                                    self.get_value_impl(tmp, current_depth + 1, max_depth)
                                })?;
                                val.as_object()?
                            }
                            Expr::RelAccess(seq) => {
                                let val = with_str_slice(seq, |tmp| {
                                    obj.get_value_impl(tmp, current_depth + 1, max_depth)
                                })?;
                                val.as_object()?
                            }
                        };
                    }
                }
            }

            if let Some(v) = obj.inner.get(*last) {
                if let Some(expr) = v.downcast_ref::<Expr>() {
                    return match expr {
                        Expr::Symbol(s) => {
                            self.get_value_impl(&[s.as_str()], current_depth + 1, max_depth)
                        }
                        Expr::Access(seq) => with_str_slice(seq, |tmp| {
                            self.get_value_impl(tmp, current_depth + 1, max_depth)
                        }),
                        Expr::RelAccess(seq) => with_str_slice(seq, |tmp| {
                            obj.get_value_impl(tmp, current_depth + 1, max_depth)
                        }),
                    };
                }
                return Some(v);
            }
        }
        None
    }

    /// Default = 20
    pub fn set_max_depth(&mut self, max_depth: usize) {
        self.max_depth = max_depth;
    }
}

pub fn load_gss_from_file<P: AsRef<Path>>(file_path: P) -> Result<Gss, Box<dyn StdError>> {
    load_gss_from_file_with_options(file_path, false)
}

pub fn load_gss_from_file_with_options<P: AsRef<Path>>(
    file_path: P,
    allow_redefinition: bool,
) -> Result<Gss, Box<dyn StdError>> {
    let source = fs::read_to_string(file_path.as_ref())?;
    let mut lex = get_lexer(
        &source,
        #[cfg(feature = "interning")]
        &file_path,
    );

    let gss = parse(file_path, &mut lex, allow_redefinition)?;

    Ok(gss)
}

/// Parses a GSS string into a `Gss` style context.
pub fn parse_str(source: &str) -> Result<Gss, Box<dyn StdError>> {
    parse_str_with_options(source, false)
}

/// Parses a GSS string with custom parser options into a `Gss` style context.
pub fn parse_str_with_options(
    source: &str,
    allow_redefinition: bool,
) -> Result<Gss, Box<dyn StdError>> {
    let mut lex = get_lexer(
        source,
        #[cfg(feature = "interning")]
        "<input>",
    );

    parse("<input>", &mut lex, allow_redefinition)
}

/// Formats a GSS source string into a canonical, cleanly indented GSS string.
pub fn format_str(source: &str) -> Result<String, Box<dyn StdError>> {
    format_str_with_config(source, &FormatOptions::default())
}

/// Formats a GSS source string with custom parser options into a canonical GSS string.
pub fn format_str_with_options(
    source: &str,
    allow_redefinition: bool,
) -> Result<String, Box<dyn StdError>> {
    let options = FormatOptions {
        allow_redefinition,
        ..Default::default()
    };
    format_str_with_config(source, &options)
}

/// Formats a GSS source string with custom format options into a canonical GSS string.
pub fn format_str_with_config(
    source: &str,
    options: &FormatOptions,
) -> Result<String, Box<dyn StdError>> {
    let gss = parse_str_with_options(source, options.allow_redefinition)?;
    Ok(gss.to_gss_string_with_options(options))
}

/// Formats a GSS file and returns the canonical formatted string.
pub fn format_file<P: AsRef<Path>>(file_path: P) -> Result<String, Box<dyn StdError>> {
    format_file_with_config(file_path, &FormatOptions::default())
}

/// Formats a GSS file with custom parser options and returns the canonical formatted string.
pub fn format_file_with_options<P: AsRef<Path>>(
    file_path: P,
    allow_redefinition: bool,
) -> Result<String, Box<dyn StdError>> {
    let options = FormatOptions {
        allow_redefinition,
        ..Default::default()
    };
    format_file_with_config(file_path, &options)
}

/// Formats a GSS file with custom format options and returns the canonical formatted string.
pub fn format_file_with_config<P: AsRef<Path>>(
    file_path: P,
    options: &FormatOptions,
) -> Result<String, Box<dyn StdError>> {
    let gss = load_gss_from_file_with_options(file_path, options.allow_redefinition)?;
    Ok(gss.to_gss_string_with_options(options))
}

/// Parses a single GSS value from a string.
pub fn parse_value_str(source: &str) -> Result<Value, Box<dyn StdError>> {
    parse_value_str_with_options(source, false)
}

/// Parses a single GSS value from a string with custom parser options.
pub fn parse_value_str_with_options(
    source: &str,
    allow_redefinition: bool,
) -> Result<Value, Box<dyn StdError>> {
    let mut lex = get_lexer(
        source,
        #[cfg(feature = "interning")]
        "<input>",
    );
    parse_single_value_entry("<input>", &mut lex, allow_redefinition)
}

/// Loads and parses a single GSS value from a file.
pub fn load_value_from_file<P: AsRef<Path>>(file_path: P) -> Result<Value, Box<dyn StdError>> {
    load_value_from_file_with_options(file_path, false)
}

/// Loads and parses a single GSS value from a file with custom parser options.
pub fn load_value_from_file_with_options<P: AsRef<Path>>(
    file_path: P,
    allow_redefinition: bool,
) -> Result<Value, Box<dyn StdError>> {
    let source = fs::read_to_string(file_path.as_ref())?;
    let mut lex = get_lexer(
        &source,
        #[cfg(feature = "interning")]
        &file_path,
    );
    parse_single_value_entry(file_path, &mut lex, allow_redefinition)
}

/// Formats a single GSS value source string into canonical formatted GSS.
pub fn format_value_str(source: &str) -> Result<String, Box<dyn StdError>> {
    format_value_str_with_config(source, &FormatOptions::default())
}

/// Formats a single GSS value source string with custom parser options into canonical formatted GSS.
pub fn format_value_str_with_options(
    source: &str,
    allow_redefinition: bool,
) -> Result<String, Box<dyn StdError>> {
    let options = FormatOptions {
        allow_redefinition,
        ..Default::default()
    };
    format_value_str_with_config(source, &options)
}

/// Formats a single GSS value source string with custom format options into canonical formatted GSS.
pub fn format_value_str_with_config(
    source: &str,
    options: &FormatOptions,
) -> Result<String, Box<dyn StdError>> {
    let val = parse_value_str_with_options(source, options.allow_redefinition)?;
    Ok(format_value_with_options(&val, 0, options))
}

/// Formats a single GSS value from a file and returns the canonical formatted string.
pub fn format_value_file<P: AsRef<Path>>(file_path: P) -> Result<String, Box<dyn StdError>> {
    format_value_file_with_config(file_path, &FormatOptions::default())
}

/// Formats a single GSS value from a file with custom parser options and returns the canonical formatted string.
pub fn format_value_file_with_options<P: AsRef<Path>>(
    file_path: P,
    allow_redefinition: bool,
) -> Result<String, Box<dyn StdError>> {
    let options = FormatOptions {
        allow_redefinition,
        ..Default::default()
    };
    format_value_file_with_config(file_path, &options)
}

/// Formats a single GSS value from a file with custom format options and returns the canonical formatted string.
pub fn format_value_file_with_config<P: AsRef<Path>>(
    file_path: P,
    options: &FormatOptions,
) -> Result<String, Box<dyn StdError>> {
    let val = load_value_from_file_with_options(file_path, options.allow_redefinition)?;
    Ok(format_value_with_options(&val, 0, options))
}

fn parse_single_value_entry<'lex, P: AsRef<Path>>(
    file_path: P,
    lex: RefLexer<'lex>,
    allow_redefinition: bool,
) -> Result<Value, Box<dyn StdError>> {
    match parse_single_value(lex, allow_redefinition) {
        Parser::Success(_, val) => Ok(val),
        Parser::Fail(lex, err) => Err(format!(
            "{}:{}: {}",
            file_path.as_ref().display(),
            lex.peek().loc,
            err
        )
        .into()),
    }
}

fn parse_single_value<'lex>(
    mut lex: RefLexer<'lex>,
    allow_redefinition: bool,
) -> Parser<'lex, Value, Box<dyn StdError>> {
    let val = try_parse!(lex, parse_value(lex, allow_redefinition));
    if lex.peek().kind == TokenKind::Comma {
        lex.next();
    }
    try_parse!(lex, parse_eof(lex));
    Parser::Success(lex, val)
}

fn get_lexer<#[cfg(feature = "interning")] P: AsRef<Path>>(
    source: &str,
    #[cfg(feature = "interning")] file_path: P,
) -> Lexer<'_> {
    #[cfg(not(feature = "interning"))]
    let lex = Lexer::new(source);
    #[cfg(feature = "interning")]
    let lex = Lexer::new(file_path.as_ref().to_string_lossy(), source);
    lex
}

fn parse<'lex, P: AsRef<Path>>(
    file_path: P,
    lex: RefLexer<'lex>,
    allow_redefinition: bool,
) -> Result<Gss, Box<dyn StdError>> {
    match parse_gss(lex, allow_redefinition) {
        Parser::Success(_, object) => Ok(object),
        Parser::Fail(lex, err) => Err(format!(
            "{}:{}: {}",
            file_path.as_ref().display(),
            lex.peek().loc,
            err
        )
        .into()),
    }
}

fn parse_gss(mut lex: RefLexer, allow_redefinition: bool) -> Parser<Gss, Box<dyn StdError>> {
    let mut object = Object::new();
    object.set_allow_redefinition(allow_redefinition);
    if lex.peek().kind == TokenKind::EOF {
        return Parser::Success(lex, object);
    }
    let fields = try_parse!(
        lex,
        many1(lex, |mut lex| {
            let k = try_parse!(lex, parse_ident(lex));
            try_parse!(lex, parse_eq(lex));
            let v = try_parse!(lex, parse_value(lex, allow_redefinition));
            if lex.peek().kind == TokenKind::Comma {
                lex.next();
            }
            Parser::Success(lex, (k, v))
        })
    );
    try_parse!(lex, parse_eof(lex));
    object.inner.reserve(fields.len());
    for (key, value) in fields {
        let key_str = key.source().to_string();
        if object.inner.insert(key_str, value).is_some() && !allow_redefinition {
            return Parser::Fail(lex, format!("Redefinition of key {key}").into());
        }
    }
    Parser::Success(lex, object)
}

fn parse_object(mut lex: RefLexer, allow_redefinition: bool) -> Parser<Object, Box<dyn StdError>> {
    let mut object = Object::new();
    object.set_allow_redefinition(allow_redefinition);
    if lex.peek().kind == TokenKind::OpenCurly {
        try_parse!(lex, parse_open_curly(lex));
    }
    if lex.peek().kind == TokenKind::CloseCurly {
        lex.next();
        return Parser::Success(lex, object);
    }
    let fields = try_parse!(
        lex,
        sep_by(
            lex,
            |mut lex| {
                let k = try_parse!(lex, parse_ident(lex));
                try_parse!(lex, parse_eq(lex));
                let v = try_parse!(lex, parse_value(lex, allow_redefinition));
                Parser::Success(lex, (k, v))
            },
            parse_maybe_comma
        )
    );
    try_parse!(lex, parse_close_curly(lex));
    object.inner.reserve(fields.len());
    for (key, value) in fields {
        let key_str = key.source().to_string();
        if object.inner.insert(key_str, value).is_some() && !allow_redefinition {
            return Parser::Fail(lex, format!("Redefinition of key {key}").into());
        }
    }
    Parser::Success(lex, object)
}

fn parse_list(
    mut lex: RefLexer,
    allow_redefinition: bool,
) -> Parser<Vec<Value>, Box<dyn StdError>> {
    try_parse!(lex, parse_open_bracket(lex));
    if lex.peek().kind == TokenKind::CloseBracket {
        lex.next();
        return Parser::Success(lex, Vec::new());
    }
    let items = try_parse!(
        lex,
        sep_by(
            lex,
            |mut lex| {
                if lex.peek().kind == TokenKind::CloseBracket {
                    return Parser::Fail(lex, "End of list".into());
                }
                let v = try_parse!(lex, parse_value(lex, allow_redefinition));
                Parser::Success(lex, v)
            },
            parse_maybe_comma
        )
    );
    try_parse!(lex, parse_close_bracket(lex));
    Parser::Success(lex, items)
}

fn parse_value(mut lex: RefLexer, allow_redefinition: bool) -> Parser<Value, Box<dyn StdError>> {
    if lex.peek().kind == TokenKind::OpenBracket {
        let list = try_parse!(lex, parse_list(lex, allow_redefinition));
        return Parser::Success(lex, Value::Vec(list));
    }
    let t = lex.next();
    match t.kind {
        TokenKind::Number(base) => {
            let x = match u32::from_str_radix(t.source(), base.radix()) {
                Ok(x) => x,
                Err(err) => return Parser::Fail(lex, err.into()),
            };
            if lex.peek().kind == TokenKind::Mod {
                lex.next();
                return Parser::Success(lex, {
                    let x = x as f32 / 100.;
                    Value::Float(x)
                });
            }
            Parser::Success(lex, Value::Number(x))
        }
        TokenKind::RealNumber => {
            let x = match t.source().parse::<f32>() {
                Ok(x) => x,
                Err(err) => return Parser::Fail(lex, err.into()),
            };
            if lex.peek().kind == TokenKind::Mod {
                lex.next();
                return Parser::Success(lex, {
                    let x = x / 100.;
                    Value::Float(x)
                });
            }
            Parser::Success(lex, Value::Float(x))
        }
        TokenKind::Minus => {
            let next_tok = lex.next();
            match next_tok.kind {
                TokenKind::Number(base) => {
                    let mag = match i64::from_str_radix(next_tok.source(), base.radix()) {
                        Ok(x) => x,
                        Err(err) => return Parser::Fail(lex, err.into()),
                    };
                    if lex.peek().kind == TokenKind::Mod {
                        lex.next();
                        return Parser::Success(lex, {
                            let x = -(mag as f32) / 100.;
                            Value::Float(x)
                        });
                    }
                    let val = match i32::try_from(-mag) {
                        Ok(v) => v,
                        Err(err) => return Parser::Fail(lex, err.into()),
                    };
                    Parser::Success(lex, Value::SignedNumber(val))
                }
                TokenKind::RealNumber => {
                    let x = match next_tok.source().parse::<f32>() {
                        Ok(x) => -x,
                        Err(err) => return Parser::Fail(lex, err.into()),
                    };
                    if lex.peek().kind == TokenKind::Mod {
                        lex.next();
                        return Parser::Success(lex, {
                            let x = x / 100.;
                            Value::Float(x)
                        });
                    }
                    Parser::Success(lex, Value::Float(x))
                }
                _ => Parser::Fail(lex, format!("Unexpect token `{next_tok}`").into()),
            }
        }
        TokenKind::Plus => {
            let next_tok = lex.next();
            match next_tok.kind {
                TokenKind::Number(base) => {
                    let x = match u32::from_str_radix(next_tok.source(), base.radix()) {
                        Ok(x) => x,
                        Err(err) => return Parser::Fail(lex, err.into()),
                    };
                    if lex.peek().kind == TokenKind::Mod {
                        lex.next();
                        return Parser::Success(lex, {
                            let x = x as f32 / 100.;
                            Value::Float(x)
                        });
                    }
                    Parser::Success(lex, Value::Number(x))
                }
                TokenKind::RealNumber => {
                    let x = match next_tok.source().parse::<f32>() {
                        Ok(x) => x,
                        Err(err) => return Parser::Fail(lex, err.into()),
                    };
                    if lex.peek().kind == TokenKind::Mod {
                        lex.next();
                        return Parser::Success(lex, {
                            let x = x / 100.;
                            Value::Float(x)
                        });
                    }
                    Parser::Success(lex, Value::Float(x))
                }
                _ => Parser::Fail(lex, format!("Unexpect token `{next_tok}`").into()),
            }
        }
        TokenKind::Identifier if t.source() == "true" => Parser::Success(lex, Value::Bool(true)),
        TokenKind::Identifier if t.source() == "false" => Parser::Success(lex, Value::Bool(false)),
        TokenKind::StringLiteral => Parser::Success(lex, {
            let x = t.unescape();
            Value::String(x)
        }),
        TokenKind::OpenCurly => {
            let object = try_parse!(lex, parse_object(lex, allow_redefinition));
            Parser::Success(lex, Value::Object(object))
        }
        TokenKind::OpenBracket => {
            let list = try_parse!(lex, parse_list(lex, allow_redefinition));
            Parser::Success(lex, Value::Vec(list))
        }
        TokenKind::Identifier => {
            if lex.peek().kind == TokenKind::Dot {
                lex.next();
                let mut seq = vec![t.source().to_string()];
                seq.extend(
                    try_parse!(lex, sep_by(lex, parse_ident, parse_dot))
                        .into_iter()
                        .map(|t| t.source().to_string()),
                );
                return Parser::Success(lex, {
                    let x = Expr::Access(seq);
                    Value::Expr(x)
                });
            }
            Parser::Success(lex, {
                let x = Expr::Symbol(t.source().to_string());
                Value::Expr(x)
            })
        }
        TokenKind::Dot => {
            if lex.peek().kind == TokenKind::Identifier {
                let t = lex.next();
                let mut seq = vec![t.source().to_string()];
                seq.extend(
                    try_parse!(lex, sep_by(lex, parse_ident, parse_dot))
                        .into_iter()
                        .map(|t| t.source().to_string()),
                );
                return Parser::Success(lex, {
                    let x = Expr::RelAccess(seq);
                    Value::Expr(x)
                });
            }
            Parser::Fail(lex, format!("Unexpect token `{t}`").into())
        }
        _ => Parser::Fail(lex, format!("Unexpect token `{t}`").into()),
    }
}

macro_rules! make_expect {
    ($name:ident, $kind:expr, $repr:literal) => {
        fn $name(lex: RefLexer) -> Parser<(), Box<dyn StdError>> {
            let actual = lex.peek().kind;
            if actual != $kind {
                return Parser::Fail(lex, format!("Expect {} got {actual:?}", $repr).into());
            }
            lex.next();
            Parser::Success(lex, ())
        }
    };
    (ret, $name:ident, $kind:expr, $repr:literal) => {
        fn $name(lex: RefLexer) -> Parser<Token, Box<dyn StdError>> {
            let actual = lex.peek().kind;
            if actual != $kind {
                return Parser::Fail(lex, format!("Expect {} got {actual:?}", $repr).into());
            }
            let t = lex.next();
            Parser::Success(lex, t)
        }
    };
}

make_expect! {parse_open_bracket, TokenKind::OpenBracket, "[" }
make_expect! {parse_close_bracket, TokenKind::CloseBracket, "]" }
make_expect! {parse_dot, TokenKind::Dot, "." }
make_expect! {parse_comma, TokenKind::Comma, "," }
make_expect! {parse_eq, TokenKind::Eq, "=" }
make_expect! {parse_open_curly, TokenKind::OpenCurly, "{" }
make_expect! {parse_close_curly, TokenKind::CloseCurly, "}" }
make_expect! {parse_eof, TokenKind::EOF, "EOF" }
make_expect! {ret, parse_ident, TokenKind::Identifier, "identifier" }

fn parse_maybe_comma(lex: RefLexer) -> Parser<(), Box<dyn StdError>> {
    if lex.peek().kind == TokenKind::Comma {
        return parse_comma(lex);
    }
    Parser::Success(lex, ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_success() {
        let source = r#"
            name = "GSS",
            version = 1,
            active = true,
            settings = {
                theme = "dark",
                debug = false,
            },
        "#;
        let gss = parse_str(source).expect("Should parse successfully");

        // Test basic values
        assert_eq!(gss.get::<String>(&["name"]), Some(&"GSS".to_string()));
        assert_eq!(gss.get::<u32>(&["version"]), Some(&1));
        assert_eq!(gss.get::<bool>(&["active"]), Some(&true));

        // Test nested values
        assert_eq!(
            gss.get::<String>(&["settings", "theme"]),
            Some(&"dark".to_string())
        );
        assert_eq!(gss.get::<bool>(&["settings", "debug"]), Some(&false));

        // Test non-existent keys / incorrect types
        assert_eq!(gss.get::<String>(&["non_existent"]), None);
        assert_eq!(gss.get::<String>(&["settings", "non_existent"]), None);
        assert_eq!(gss.get::<u32>(&["active"]), None); // Type mismatch
    }

    #[test]
    fn test_parse_list() {
        let source = r#"
            numbers = [1, 2, 3],
            mixed = ["hello", 42, true],
            empty = [],
            nested = [[1, 2], [3, 4]],
        "#;
        let gss = parse_str(source).expect("Should parse lists successfully");

        assert_eq!(gss.get_as::<Vec<u32>>(&["numbers"]), Some(vec![1, 2, 3]));
        assert!(gss.get::<Vec<Value>>(&["mixed"]).is_some());
        assert_eq!(gss.get_as::<Vec<u32>>(&["empty"]), Some(vec![]));
        assert_eq!(
            gss.get_as::<Vec<Vec<u32>>>(&["nested"]),
            Some(vec![vec![1, 2], vec![3, 4]])
        );
    }

    #[test]
    fn test_parse_redefinition() {
        let source = r#"
            key = 1,
            key = 2,
        "#;
        let result = parse_str(source);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Redefinition of key key"));
    }

    #[test]
    fn test_parse_missing_comma() {
        let source = r#"
            test = {
                key = 1
                other = 2,
            }
        "#;
        let result = parse_str(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_missing_eq() {
        let source = r#"
            key 1,
        "#;
        let result = parse_str(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_path() {
        let gss = parse_str("a = 1,").expect("Should parse");
        assert_eq!(gss.get::<u32>(&[]), None);
    }

    #[test]
    fn test_dump() {
        let source = r#"
            name = "GSS",
            version = 1,
            active = true,
            settings = {
                theme = "dark",
            },
        "#;
        let gss = parse_str(source).expect("Should parse");
        // Ensure dump runs without panicking
        gss.dump(0);
    }

    #[test]
    fn test_references() {
        let source = r#"
            root_val = 42,
            ref_symbol = root_val,
            nested = {
                value = 100,
                ref_symbol_nested = root_val,
            },
            ref_access = nested.value,
            other = {
                ref_access_nested = nested.value,
            },
            chained1 = root_val,
            chained2 = chained1,
            non_existent_ref = does_not_exist,
            nested_non_existent_ref = nested.does_not_exist,
        "#;
        let gss = parse_str(source).expect("Should parse references successfully");

        // Test Expr::Symbol at root level
        assert_eq!(gss.get::<u32>(&["ref_symbol"]), Some(&42));

        // Test Expr::Symbol inside nested object
        assert_eq!(gss.get::<u32>(&["nested", "ref_symbol_nested"]), Some(&42));

        // Test Expr::Access at root level
        assert_eq!(gss.get::<u32>(&["ref_access"]), Some(&100));

        // Test Expr::Access inside nested object
        assert_eq!(gss.get::<u32>(&["other", "ref_access_nested"]), Some(&100));

        // Test chained references
        assert_eq!(gss.get::<u32>(&["chained2"]), Some(&42));

        // Test invalid reference (non-existent key)
        assert_eq!(gss.get::<u32>(&["non_existent_ref"]), None);
        assert_eq!(gss.get::<u32>(&["nested_non_existent_ref"]), None);

        // Test type mismatch
        assert_eq!(gss.get::<String>(&["ref_symbol"]), None);

        // Test dump with references
        gss.dump(0);
    }

    #[test]
    fn test_load_files() {
        let gss1 = load_gss_from_file("tests/test.gss").expect("Should load test.gss");
        assert_eq!(gss1.get::<Percent>(&["style", "top"]), Some(&0.89));
        assert_eq!(gss1.get::<u32>(&["style", "count"]), Some(&69));
        assert_eq!(
            gss1.get::<String>(&["style", "inner", "link"]),
            Some(&"google.com".to_string())
        );

        let gss2 = load_gss_from_file("tests/test2.gss").expect("Should load test2.gss");
        assert_eq!(gss2.get::<u32>(&["style", "image1", "top"]), Some(&50));
        assert_eq!(gss2.get::<u32>(&["style", "image2", "top"]), Some(&50));
        assert_eq!(gss2.get::<u32>(&["style", "image2", "left"]), Some(&50));

        let gss3 = load_gss_from_file("tests/test3.gss").expect("Should load test3.gss");
        assert_eq!(gss3.get::<u32>(&["test", "key"]), Some(&1));
        assert_eq!(gss3.get::<u32>(&["test", "other"]), Some(&2));
        assert_eq!(gss3.get::<u32>(&["test", "hex"]), Some(&0x32));
    }

    #[test]
    fn test_cycle_detection() {
        // Direct cycle: a = a,
        let source_direct = r#"
            a = a,
        "#;
        let gss = parse_str(source_direct).expect("Should parse");
        assert_eq!(gss.get::<u32>(&["a"]), None);

        // Indirect cycle: a = b, b = a,
        let source_indirect = r#"
            a = b,
            b = a,
        "#;
        let gss = parse_str(source_indirect).expect("Should parse");
        assert_eq!(gss.get::<u32>(&["a"]), None);
        assert_eq!(gss.get::<u32>(&["b"]), None);

        // Path cycle: a = b.x, b = { x = a },
        let source_path = r#"
            a = b.x,
            b = {
                x = a,
            },
        "#;
        let gss = parse_str(source_path).expect("Should parse");
        assert_eq!(gss.get::<u32>(&["a"]), None);
    }

    #[test]
    fn test_percent() {
        let source_path = r#"
            a = 89%,
        "#;
        let gss = parse_str(source_path).expect("Should parse");
        assert_eq!(gss.get::<Percent>(&["a"]), Some(&0.89));
    }

    #[test]
    fn test_float() {
        let source_path = r#"
            a = 0.123,
        "#;
        let gss = parse_str(source_path).expect("Should parse");
        assert_eq!(gss.get::<f32>(&["a"]), Some(&0.123));
    }

    #[test]
    fn test_get_fields() {
        let source_path = r#"
            a = 0.123,
            b = true,
            c = "test",
        "#;
        let gss = parse_str(source_path).expect("Should parse");
        for field in gss.get_fields() {
            assert!(["a", "b", "c"].contains(&field.as_str()))
        }
    }

    #[test]
    fn test_get_or_default() {
        let source_path = r#"

        "#;
        let gss = parse_str(source_path).expect("Should parse");
        assert_eq!(gss.get_or_default::<f32>(&["a"]), 0.0);
        assert_eq!(gss.get_or::<f32>(&["b"], 23.0), 23.0);
    }

    #[test]
    fn test_get_inner_obj() {
        let source_path = r#"
            h = {
                inner = "Hi"
            }
            f = {
                g = h
            }
        "#;
        let gss = parse_str(source_path).expect("Should parse");
        assert_eq!(
            gss.get_or_default::<String>(&["f", "g", "inner"]),
            "Hi".to_string()
        );
    }

    #[test]
    fn test_dot_separated_lookup() {
        let source = r#"
            a = {
                b = {
                    c = "found_it"
                }
                d = {
                    e = a.b.c
                }
            }
        "#;
        let gss = parse_str(source).expect("Should parse");
        assert_eq!(
            gss.get::<String>(&["a", "b", "c"]),
            Some(&"found_it".to_string())
        );
        assert_eq!(
            gss.get::<String>(&["a", "b", "c"]),
            Some(&"found_it".to_string())
        );
        assert_eq!(
            gss.get::<String>(&["a", "d", "e"]),
            Some(&"found_it".to_string())
        );
    }

    #[test]
    fn test_get_as_common_types() {
        let source = r#"
            num = 42,
            float_val = 3.14,
            text = "antigravity",
            flag = true,
            big_num = 300,
        "#;
        let gss = parse_str(source).expect("Should parse");

        // Signed integer conversions
        assert_eq!(gss.get_as::<i8>(&["num"]), Some(42i8));
        assert_eq!(gss.get_as::<i16>(&["num"]), Some(42i16));
        assert_eq!(gss.get_as::<i32>(&["num"]), Some(42i32));
        assert_eq!(gss.get_as::<i64>(&["num"]), Some(42i64));
        assert_eq!(gss.get_as::<i128>(&["num"]), Some(42i128));
        assert_eq!(gss.get_as::<isize>(&["num"]), Some(42isize));

        // Unsigned integer conversions
        assert_eq!(gss.get_as::<u8>(&["num"]), Some(42u8));
        assert_eq!(gss.get_as::<u16>(&["num"]), Some(42u16));
        assert_eq!(gss.get_as::<u32>(&["num"]), Some(42u32));
        assert_eq!(gss.get_as::<u64>(&["num"]), Some(42u64));
        assert_eq!(gss.get_as::<u128>(&["num"]), Some(42u128));
        assert_eq!(gss.get_as::<usize>(&["num"]), Some(42usize));

        // Float conversions
        assert_eq!(gss.get_as::<f32>(&["float_val"]), Some(3.14f32));
        assert_eq!(gss.get_as::<f64>(&["float_val"]), Some(3.14f32 as f64));
        assert_eq!(gss.get_as::<f32>(&["num"]), Some(42.0f32));
        assert_eq!(gss.get_as::<f64>(&["num"]), Some(42.0f64));

        // Booleans & Strings
        assert_eq!(gss.get_as::<bool>(&["flag"]), Some(true));
        assert_eq!(
            gss.get_as::<String>(&["text"]),
            Some("antigravity".to_string())
        );

        // Bounds checking
        assert_eq!(gss.get_as::<u8>(&["big_num"]), None); // 300 > u8::MAX
        assert_eq!(gss.get_as::<i8>(&["big_num"]), None); // 300 > i8::MAX
        assert_eq!(gss.get_as::<u16>(&["big_num"]), Some(300u16));

        // Fallbacks with get_or and get_or_default
        assert_eq!(gss.get_or::<i32>(&["num"], 0), 42);
        assert_eq!(gss.get_or::<i32>(&["missing"], 99), 99);
        assert_eq!(gss.get_or_default::<i64>(&["num"]), 42i64);
        assert_eq!(gss.get_or_default::<i64>(&["missing"]), 0i64);
    }

    #[test]
    fn test_allow_redefinition() {
        let source_root = r#"
            key = 1,
            key = 2,
        "#;
        // Default (false) should fail
        assert!(parse_str(source_root).is_err());

        // With allow_redefinition = true
        let gss = parse_str_with_options(source_root, true).expect("Should allow redefinition");
        assert_eq!(gss.get_as::<u32>(&["key"]), Some(2));
        assert!(gss.allow_redefinition());

        // Nested redefinition
        let source_nested = r#"
            obj = {
                a = 10,
                a = 20,
            }
        "#;
        assert!(parse_str(source_nested).is_err());
        let gss_nested =
            parse_str_with_options(source_nested, true).expect("Should allow nested redefinition");
        assert_eq!(gss_nested.get_as::<u32>(&["obj", "a"]), Some(20));

        // Builder & Setter
        let mut obj = Object::new().with_allow_redefinition(true);
        assert!(obj.allow_redefinition());
        obj.set_allow_redefinition(false);
        assert!(!obj.allow_redefinition());
    }

    #[test]
    fn test_value_enum_specifics() {
        let source = "a = 42, b = { x = 1 }, c = \"hello\"";
        let gss = parse_str(source).unwrap();

        let a_val = gss.get_value(&["a"]).unwrap();
        let b_val = gss.get_value(&["b"]).unwrap();
        let c_val = gss.get_value(&["c"]).unwrap();

        // Assert equality works directly on Value
        assert_eq!(a_val, &Value::Number(42));
        assert_eq!(c_val, &Value::String("hello".to_string()));

        // Assert cloning works directly on Value
        let a_clone = a_val.clone();
        assert_eq!(a_clone, Value::Number(42));

        let b_clone = b_val.clone();
        if let Value::Object(ref obj) = b_clone {
            assert_eq!(obj.get::<u32>(&["x"]), Some(&1));
        } else {
            panic!("Expected Value::Object");
        }
    }

    #[test]
    fn test_vector_parsing_and_conversions() {
        let source = r#"
            ints = [1, 2, 3],
            mixed = ["hello", true, 42.5],
            nested = [[1, 2], [3, 4]],
            empty = [],
        "#;
        let gss = parse_str(source).expect("Should parse vectors");

        // Verify direct retrieval and conversion to Vec<T>
        assert_eq!(gss.get_as::<Vec<u32>>(&["ints"]), Some(vec![1, 2, 3]));
        assert_eq!(gss.get_as::<Vec<i32>>(&["ints"]), Some(vec![1, 2, 3]));

        // Verify nested lists
        let nested = gss.get_as::<Vec<Vec<u32>>>(&["nested"]).unwrap();
        assert_eq!(nested, vec![vec![1, 2], vec![3, 4]]);

        // Empty vector
        assert_eq!(gss.get_as::<Vec<u32>>(&["empty"]), Some(vec![]));

        // Mixed list types can be retrieved as Vec<Value>
        let mixed = gss.get_as::<Vec<Value>>(&["mixed"]).unwrap();
        assert_eq!(mixed.len(), 3);
        assert_eq!(mixed[0].as_string(), Some(&"hello".to_string()));
        assert_eq!(mixed[1].as_bool(), Some(&true));
        assert_eq!(mixed[2].as_float(), Some(&42.5));
    }

    #[test]
    fn test_format_str_and_to_gss_string() {
        let unformatted = r#"
            z = 10,
            a = {
                nested_b = "line1\nline2",
                nested_a = true
            },
            arr = [1, 2, 3]
        "#;
        let formatted = format_str(unformatted).expect("Should format successfully");
        let expected = "a = {\n    nested_a = true,\n    nested_b = \"line1\\nline2\",\n},\narr = [1, 2, 3],\nz = 10,\n";
        assert_eq!(formatted, expected);

        // Verify reparsing formatted output yields identical values
        let gss = parse_str(&formatted).expect("Formatted GSS must be valid");
        assert_eq!(gss.get_as::<u32>(&["z"]), Some(10));
        assert_eq!(gss.get_as::<bool>(&["a", "nested_a"]), Some(true));
        assert_eq!(
            gss.get_as::<String>(&["a", "nested_b"]),
            Some("line1\nline2".to_string())
        );
        assert_eq!(gss.get_as::<Vec<u32>>(&["arr"]), Some(vec![1, 2, 3]));
        assert_eq!(gss.to_string(), expected);
    }

    #[test]
    fn test_format_expressions() {
        let source = r#"
            a = 10,
            b = a,
            c = style.image.top,
            d = .sibling,
        "#;
        let formatted = format_str(source).expect("Should format successfully");
        let expected = "a = 10,\nb = a,\nc = style.image.top,\nd = .sibling,\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_nested_objects_and_empty() {
        let source = r#"
            empty = {},
            deep = {
                l1 = {
                    l2 = {
                        val = 42,
                    }
                }
            }
        "#;
        let formatted = format_str(source).expect("Should format successfully");
        let expected = "deep = {\n    l1 = {\n        l2 = {\n            val = 42,\n        },\n    },\n},\nempty = {},\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_dump_output() {
        let source = r#"
            name = "GSS",
            count = 42,
            sub = {
                enabled = true,
            },
        "#;
        let gss = parse_str(source).expect("Should parse");
        // Level 0
        assert_eq!(
            gss.format_with_level(0),
            "count = 42,\nname = \"GSS\",\nsub = {\n    enabled = true,\n},\n"
        );
        // Level 1
        assert_eq!(
            gss.format_with_level(1),
            "{\n    count = 42,\n    name = \"GSS\",\n    sub = {\n        enabled = true,\n    },\n}"
        );
    }

    #[test]
    fn test_single_value_parsing_and_formatting() {
        // Scalar values
        let num_val = parse_value_str("42").expect("Should parse number");
        assert_eq!(num_val.as_number(), Some(&42));
        assert_eq!(format_value_str("42").unwrap(), "42");

        let str_val = parse_value_str("\"hello world\"").expect("Should parse string");
        assert_eq!(str_val.as_string(), Some(&"hello world".to_string()));
        assert_eq!(
            format_value_str("\"hello world\"").unwrap(),
            "\"hello world\""
        );

        let bool_val = parse_value_str("true,").expect("Should parse bool with optional comma");
        assert_eq!(bool_val.as_bool(), Some(&true));

        // Array value
        let list_val = parse_value_str("[1, 2, 3]").expect("Should parse array");
        assert!(list_val.as_vec().is_some());
        assert_eq!(format_value_str("[1, 2, 3]").unwrap(), "[1, 2, 3]");

        // Object value
        let obj_val = parse_value_str("{ a = 1 }").expect("Should parse object value");
        assert!(obj_val.as_object().is_some());
        assert_eq!(format_value_str("{ a = 1 }").unwrap(), "{\n    a = 1,\n}");

        // Invalid trailing syntax check
        assert!(parse_value_str("42 extra").is_err());
    }

    #[test]
    fn test_signed_numbers() {
        let source = r#"
            neg_int = -42,
            pos_int = +42,
            neg_hex = -0x32,
            pos_hex = +0x32,
            min_i32 = -2147483648,
            neg_float = -3.14,
            pos_float = +3.14,
            neg_percent = -50%,
            pos_percent = +50%,
            neg_float_percent = -12.5%,
            signed_list = [-10, 0, +10, -20],
        "#;
        let gss = parse_str(source).expect("Should parse signed numbers");

        // Direct get
        assert_eq!(gss.get::<i32>(&["neg_int"]), Some(&-42));
        assert_eq!(gss.get::<u32>(&["pos_int"]), Some(&42));
        assert_eq!(gss.get::<i32>(&["neg_hex"]), Some(&-50));
        assert_eq!(gss.get::<u32>(&["pos_hex"]), Some(&50));
        assert_eq!(gss.get::<i32>(&["min_i32"]), Some(&i32::MIN));
        assert_eq!(gss.get::<f32>(&["neg_float"]), Some(&-3.14));
        assert_eq!(gss.get::<f32>(&["pos_float"]), Some(&3.14));
        assert_eq!(gss.get::<f32>(&["neg_percent"]), Some(&-0.5));
        assert_eq!(gss.get::<f32>(&["pos_percent"]), Some(&0.5));
        assert_eq!(gss.get::<f32>(&["neg_float_percent"]), Some(&-0.125));

        // get_as conversions
        assert_eq!(gss.get_as::<i8>(&["neg_int"]), Some(-42));
        assert_eq!(gss.get_as::<i16>(&["neg_int"]), Some(-42));
        assert_eq!(gss.get_as::<i64>(&["neg_int"]), Some(-42));
        assert_eq!(gss.get_as::<isize>(&["neg_int"]), Some(-42));
        assert_eq!(gss.get_as::<u32>(&["neg_int"]), None); // Negative cannot convert to unsigned
        assert_eq!(gss.get_as::<i32>(&["pos_int"]), Some(42));
        assert_eq!(gss.get_as::<f32>(&["neg_int"]), Some(-42.0));
        assert_eq!(gss.get_as::<f64>(&["neg_int"]), Some(-42.0));
        assert_eq!(
            gss.get_as::<Vec<i32>>(&["signed_list"]),
            Some(vec![-10, 0, 10, -20])
        );

        // Single value parsing and formatting
        assert_eq!(format_value_str("-42").unwrap(), "-42");
        assert_eq!(format_value_str("+42").unwrap(), "42");
        assert_eq!(format_value_str("-3.5").unwrap(), "-3.5");
        assert_eq!(format_value_str("-50%").unwrap(), "-0.5");
    }

    #[test]
    fn test_pretty_format_list_of_objects() {
        let source = r#"
            items = [
                { id = 1, name = "first" },
                { id = 2, name = "second" },
            ],
        "#;
        let formatted = format_str(source).expect("Should format list of objects");
        let expected = "items = [\n    {\n        id = 1,\n        name = \"first\",\n    },\n    {\n        id = 2,\n        name = \"second\",\n    },\n],\n";
        assert_eq!(formatted, expected);

        // Reparsing must succeed and be equivalent
        let gss = parse_str(&formatted).expect("Should reparse formatted output");
        let items = gss.get::<Vec<Value>>(&["items"]).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_pretty_format_wrapping_long_list() {
        let source = r#"
            urls = [
                "https://api.example.com/v1/auth/login",
                "https://api.example.com/v1/auth/logout",
                "https://api.example.com/v1/auth/refresh"
            ]
        "#;
        let formatted = format_str(source).expect("Should format long list");
        assert!(formatted.contains("urls = [\n"));
        assert!(formatted.contains("    \"https://api.example.com/v1/auth/login\",\n"));
        assert!(formatted.ends_with("],\n"));
    }

    #[test]
    fn test_format_options_custom_indent_and_tabs() {
        let source = "obj = { a = 1, b = 2 }";

        // 2 spaces
        let opts_2_spaces = FormatOptions {
            indent_width: 2,
            ..Default::default()
        };
        let formatted_2 = format_str_with_config(source, &opts_2_spaces).unwrap();
        assert_eq!(formatted_2, "obj = {\n  a = 1,\n  b = 2,\n},\n");

        // Tabs
        let opts_tabs = FormatOptions {
            use_tabs: true,
            ..Default::default()
        };
        let formatted_tabs = format_str_with_config(source, &opts_tabs).unwrap();
        assert_eq!(formatted_tabs, "obj = {\n\ta = 1,\n\tb = 2,\n},\n");
    }

    #[test]
    fn test_format_options_separate_blocks() {
        let source = r#"
            name = "App",
            database = {
                host = "localhost",
            },
            server = {
                port = 8080,
            },
            version = 1,
        "#;
        let opts = FormatOptions {
            separate_blocks: true,
            ..Default::default()
        };
        let formatted = format_str_with_config(source, &opts).unwrap();
        let expected = "database = {\n    host = \"localhost\",\n},\n\nname = \"App\",\n\nserver = {\n    port = 8080,\n},\n\nversion = 1,\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_to_gss_value_unsigned_integers() {
        assert_eq!(ToGssValue::to_gss_value(&0u8), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&42u8), Value::Number(42));
        assert_eq!(ToGssValue::to_gss_value(&u8::MAX), Value::Number(255));

        assert_eq!(ToGssValue::to_gss_value(&0u16), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&1000u16), Value::Number(1000));
        assert_eq!(ToGssValue::to_gss_value(&u16::MAX), Value::Number(65535));

        assert_eq!(ToGssValue::to_gss_value(&0u32), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&42u32), Value::Number(42));
        assert_eq!(ToGssValue::to_gss_value(&u32::MAX), Value::Number(u32::MAX));

        assert_eq!(ToGssValue::to_gss_value(&0u64), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&123456u64), Value::Number(123456));

        assert_eq!(ToGssValue::to_gss_value(&0u128), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&78910u128), Value::Number(78910));

        assert_eq!(ToGssValue::to_gss_value(&0usize), Value::Number(0));
        assert_eq!(ToGssValue::to_gss_value(&1024usize), Value::Number(1024));
    }

    #[test]
    fn test_to_gss_value_signed_integers() {
        assert_eq!(ToGssValue::to_gss_value(&0i8), Value::SignedNumber(0));
        assert_eq!(ToGssValue::to_gss_value(&42i8), Value::SignedNumber(42));
        assert_eq!(ToGssValue::to_gss_value(&(-42i8)), Value::SignedNumber(-42));
        assert_eq!(
            ToGssValue::to_gss_value(&i8::MIN),
            Value::SignedNumber(-128)
        );
        assert_eq!(ToGssValue::to_gss_value(&i8::MAX), Value::SignedNumber(127));

        assert_eq!(ToGssValue::to_gss_value(&0i16), Value::SignedNumber(0));
        assert_eq!(
            ToGssValue::to_gss_value(&1000i16),
            Value::SignedNumber(1000)
        );
        assert_eq!(
            ToGssValue::to_gss_value(&(-1000i16)),
            Value::SignedNumber(-1000)
        );

        assert_eq!(ToGssValue::to_gss_value(&0i32), Value::SignedNumber(0));
        assert_eq!(ToGssValue::to_gss_value(&42i32), Value::SignedNumber(42));
        assert_eq!(
            ToGssValue::to_gss_value(&(-42i32)),
            Value::SignedNumber(-42)
        );
        assert_eq!(
            ToGssValue::to_gss_value(&i32::MIN),
            Value::SignedNumber(i32::MIN)
        );
        assert_eq!(
            ToGssValue::to_gss_value(&i32::MAX),
            Value::SignedNumber(i32::MAX)
        );

        assert_eq!(ToGssValue::to_gss_value(&0i64), Value::SignedNumber(0));
        assert_eq!(
            ToGssValue::to_gss_value(&(-123456i64)),
            Value::SignedNumber(-123456)
        );

        assert_eq!(ToGssValue::to_gss_value(&0i128), Value::SignedNumber(0));
        assert_eq!(
            ToGssValue::to_gss_value(&(-78910i128)),
            Value::SignedNumber(-78910)
        );

        assert_eq!(ToGssValue::to_gss_value(&0isize), Value::SignedNumber(0));
        assert_eq!(
            ToGssValue::to_gss_value(&(-1024isize)),
            Value::SignedNumber(-1024)
        );
    }

    #[test]
    fn test_to_gss_value_floats() {
        assert_eq!(ToGssValue::to_gss_value(&0.0f32), Value::Float(0.0));
        assert_eq!(ToGssValue::to_gss_value(&2.5f32), Value::Float(2.5));
        assert_eq!(ToGssValue::to_gss_value(&(-42.5f32)), Value::Float(-42.5));

        assert_eq!(ToGssValue::to_gss_value(&0.0f64), Value::Float(0.0));
        assert_eq!(ToGssValue::to_gss_value(&2.5f64), Value::Float(2.5f32));
        assert_eq!(
            ToGssValue::to_gss_value(&(-42.5f64)),
            Value::Float(-42.5f32)
        );
    }

    #[test]
    fn test_to_gss_value_bool() {
        assert_eq!(ToGssValue::to_gss_value(&true), Value::Bool(true));
        assert_eq!(ToGssValue::to_gss_value(&false), Value::Bool(false));
    }

    #[test]
    fn test_to_gss_value_string() {
        let empty = String::new();
        assert_eq!(
            ToGssValue::to_gss_value(&empty),
            Value::String(String::new())
        );

        let text = "Hello GSS".to_string();
        assert_eq!(
            ToGssValue::to_gss_value(&text),
            Value::String("Hello GSS".to_string())
        );

        let unicode = "🚀 rust 🦀".to_string();
        assert_eq!(
            ToGssValue::to_gss_value(&unicode),
            Value::String("🚀 rust 🦀".to_string())
        );
    }

    #[test]
    fn test_to_gss_value_vec() {
        let empty: Vec<Value> = vec![];
        assert_eq!(ToGssValue::to_gss_value(&empty), Value::Vec(vec![]));

        let items = vec![
            Value::Number(10),
            Value::SignedNumber(-20),
            Value::Float(1.5),
            Value::Bool(true),
            Value::String("gss".to_string()),
        ];
        assert_eq!(ToGssValue::to_gss_value(&items), Value::Vec(items.clone()));
    }

    #[test]
    fn test_to_gss_value_generic_function() {
        fn to_val<T: ToGssValue>(val: &T) -> Value {
            ToGssValue::to_gss_value(val)
        }

        assert_eq!(to_val(&100u32), Value::Number(100));
        assert_eq!(to_val(&(-50i32)), Value::SignedNumber(-50));
        assert_eq!(to_val(&12.5f32), Value::Float(12.5));
        assert_eq!(to_val(&true), Value::Bool(true));
        assert_eq!(
            to_val(&"test".to_string()),
            Value::String("test".to_string())
        );
    }

    #[test]
    fn test_to_gss_value_roundtrip() {
        // u32 roundtrip
        let orig_u32 = 42u32;
        let gss_val = ToGssValue::to_gss_value(&orig_u32);
        assert_eq!(u32::from_gss_value(&gss_val), Some(orig_u32));

        // i32 roundtrip
        let orig_i32 = -42i32;
        let gss_val = ToGssValue::to_gss_value(&orig_i32);
        assert_eq!(i32::from_gss_value(&gss_val), Some(orig_i32));

        // f32 roundtrip
        let orig_f32 = 2.5f32;
        let gss_val = ToGssValue::to_gss_value(&orig_f32);
        assert_eq!(f32::from_gss_value(&gss_val), Some(orig_f32));

        // bool roundtrip
        let orig_bool = true;
        let gss_val = ToGssValue::to_gss_value(&orig_bool);
        assert_eq!(bool::from_gss_value(&gss_val), Some(orig_bool));

        // String roundtrip
        let orig_str = "roundtrip".to_string();
        let gss_val = ToGssValue::to_gss_value(&orig_str);
        assert_eq!(String::from_gss_value(&gss_val), Some(orig_str));

        // Vec<Value> roundtrip
        let orig_vec = vec![
            Value::Number(1),
            Value::SignedNumber(-1),
            Value::Bool(false),
        ];
        let gss_val = ToGssValue::to_gss_value(&orig_vec);
        assert_eq!(Vec::<Value>::from_gss_value(&gss_val), Some(orig_vec));
    }
}
