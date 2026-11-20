//! basalt-plugin-parser-koka/src/lib.rs — Pure-Rust Koka parser WASM plugin for Basalt.
//!
//! Implements the parser ABI consumed by `basalt-core/src/parser.rs`:
//!   basalt_lang, basalt_lexical_profile, basalt_src_ptr, basalt_out_ptr,
//!   basalt_parse, basalt_retrieval_chunks, basalt_call_sites
//!
//! Retrieval chunks use the 104-byte v1 record layout:
//!   [offset u32 LE][length u32 LE][label 95 bytes + NUL][kind u8]

#![allow(static_mut_refs)]

const SRC_OFFSET: usize = 0;
const OUT_OFFSET: usize = 6 * 1024 * 1024;
const MEMORY_BYTES: usize = 12 * 1024 * 1024;

const SCOPE_KEYWORD: u8 = 1;
const SCOPE_STRING: u8 = 2;
const SCOPE_NUMBER: u8 = 3;
const SCOPE_COMMENT: u8 = 4;
const SCOPE_TYPE: u8 = 5;
const SCOPE_FUNCTION: u8 = 6;
const SCOPE_OPERATOR: u8 = 7;
const SCOPE_VARIABLE: u8 = 10;

const KIND_MODULE: u8 = 1;
const KIND_TYPE: u8 = 2;
const KIND_FUNCTION: u8 = 3;
const KIND_PROPERTY: u8 = 5;
const KIND_ENUM_CASE: u8 = 6;

static mut MEMORY: [u8; MEMORY_BYTES] = [0u8; MEMORY_BYTES];
static LANG_EXT: &[u8] = b"kk\0";
static EXT_ALIASES: &[u8] = b"kki\0";

static LEXICAL_PROFILE: &[u8] = b"{\
  \"version\": 1,\
  \"extensions\": [\"kk\", \"kki\"],\
  \"line_comments\": [\"//\"],\
  \"block_comment\": {\"open\": \"/*\", \"close\": \"*/\", \"nested\": true},\
  \"strings\": [\
    {\"open\": \"\\\"\", \"close\": \"\\\"\"},\
    {\"open\": \"@\\\"\", \"close\": \"\\\"\"}\
  ]\
}\0";

#[no_mangle]
pub extern "C" fn basalt_lang() -> i32 {
    LANG_EXT.as_ptr() as i32
}

#[no_mangle]
pub extern "C" fn basalt_ext_aliases() -> i32 {
    EXT_ALIASES.as_ptr() as i32
}

#[no_mangle]
pub extern "C" fn basalt_lexical_profile() -> i32 {
    LEXICAL_PROFILE.as_ptr() as i32
}

#[no_mangle]
pub unsafe extern "C" fn basalt_src_ptr() -> i32 {
    MEMORY[SRC_OFFSET..].as_ptr() as i32
}

#[no_mangle]
pub unsafe extern "C" fn basalt_out_ptr() -> i32 {
    MEMORY[OUT_OFFSET..].as_ptr() as i32
}

#[no_mangle]
pub unsafe extern "C" fn basalt_parse(
    src_ptr: i32,
    src_len: i32,
    out_ptr: i32,
    max_spans: i32,
) -> i32 {
    if src_ptr == 0 || out_ptr == 0 || src_len <= 0 || max_spans <= 0 {
        return 0;
    }
    let src = core::slice::from_raw_parts(src_ptr as usize as *const u8, src_len as usize);
    let out = core::slice::from_raw_parts_mut(
        out_ptr as usize as *mut u8,
        (max_spans as usize) * 12,
    );
    let spans = parse_spans(src, max_spans as usize);
    for (i, (offset, length, scope_id)) in spans.iter().enumerate() {
        let base = i * 12;
        out[base..base + 4].copy_from_slice(&offset.to_le_bytes());
        out[base + 4..base + 8].copy_from_slice(&length.to_le_bytes());
        out[base + 8] = *scope_id;
        out[base + 9] = 0;
        out[base + 10] = 0;
        out[base + 11] = 0;
    }
    spans.len() as i32
}

#[no_mangle]
pub unsafe extern "C" fn basalt_retrieval_chunks(
    src_ptr: i32,
    src_len: i32,
    out_ptr: i32,
    max_chunks: i32,
) -> i32 {
    if src_ptr == 0 || out_ptr == 0 || src_len <= 0 || max_chunks <= 0 {
        return 0;
    }
    let src = core::slice::from_raw_parts(src_ptr as usize as *const u8, src_len as usize);
    let out = core::slice::from_raw_parts_mut(
        out_ptr as usize as *mut u8,
        (max_chunks as usize) * 104,
    );
    let chunks = retrieval_chunks(src, max_chunks as usize);
    for (i, (offset, length, label, kind)) in chunks.iter().enumerate() {
        let base = i * 104;
        out[base..base + 4].copy_from_slice(&offset.to_le_bytes());
        out[base + 4..base + 8].copy_from_slice(&length.to_le_bytes());
        let label_bytes = label.as_bytes();
        let label_len = label_bytes.len().min(95);
        out[base + 8..base + 8 + label_len].copy_from_slice(&label_bytes[..label_len]);
        out[base + 8 + label_len] = 0;
        out[base + 103] = *kind;
    }
    chunks.len() as i32
}

#[no_mangle]
pub unsafe extern "C" fn basalt_call_sites(
    src_ptr: i32,
    src_len: i32,
    out_ptr: i32,
    max_sites: i32,
) -> i32 {
    if src_ptr == 0 || out_ptr == 0 || src_len <= 0 || max_sites <= 0 {
        return 0;
    }
    let src = core::slice::from_raw_parts(src_ptr as usize as *const u8, src_len as usize);
    let out = core::slice::from_raw_parts_mut(
        out_ptr as usize as *mut u8,
        (max_sites as usize) * 68,
    );
    let sites = call_sites(src, max_sites as usize);
    for (i, (offset, name)) in sites.iter().enumerate() {
        let base = i * 68;
        out[base..base + 4].copy_from_slice(&offset.to_le_bytes());
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len().min(63);
        out[base + 4..base + 4 + name_len].copy_from_slice(&name_bytes[..name_len]);
        out[base + 4 + name_len] = 0;
    }
    sites.len() as i32
}

// ── Lexer ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TokKind {
    Ident,
    UpperIdent,
    Keyword,
    String,
    Number,
    Comment,
    Operator,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Dot,
    Colon,
    Other,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: TokKind,
    start: usize,
    end: usize,
}

fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "abstract"
            | "alias"
            | "as"
            | "brk"
            | "co"
            | "con"
            | "ctl"
            | "div"
            | "effect"
            | "elif"
            | "else"
            | "exists"
            | "extend"
            | "extern"
            | "false"
            | "final"
            | "finally"
            | "fn"
            | "for"
            | "forall"
            | "fun"
            | "handle"
            | "handler"
            | "if"
            | "import"
            | "in"
            | "initially"
            | "inline"
            | "linear"
            | "mask"
            | "match"
            | "module"
            | "named"
            | "open"
            | "override"
            | "pub"
            | "raw"
            | "rec"
            | "reference"
            | "return"
            | "scoped"
            | "some"
            | "struct"
            | "then"
            | "true"
            | "type"
            | "unless"
            | "val"
            | "value"
            | "var"
            | "while"
            | "with"
    )
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'\''
}

fn scan_ident_end(src: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < src.len() {
        if is_ident_continue(src[i]) {
            i += 1;
        } else if src[i] == b'-' && i + 1 < src.len() && src[i + 1].is_ascii_alphabetic() {
            // Koka allows hyphens inside identifiers (e.g., `read-text-file`).
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn scan_string_end(src: &[u8], start: usize, quote: u8, verbatim: bool) -> usize {
    let mut i = start;
    while i < src.len() {
        let b = src[i];
        if !verbatim && b == b'\\' {
            i = (i + 2).min(src.len());
            continue;
        }
        if b == quote {
            return i + 1;
        }
        i += 1;
    }
    src.len()
}

fn tokenize(src: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Line comment `//`
        if b == b'/' && i + 1 < src.len() && src[i + 1] == b'/' {
            let start = i;
            i += 2;
            while i < src.len() && src[i] != b'\n' {
                i += 1;
            }
            tokens.push(Token {
                kind: TokKind::Comment,
                start,
                end: i,
            });
            continue;
        }

        // Nested block comment `/* ... */`
        if b == b'/' && i + 1 < src.len() && src[i + 1] == b'*' {
            let start = i;
            i += 2;
            let mut depth = 1usize;
            while i < src.len() && depth > 0 {
                if src[i] == b'/' && i + 1 < src.len() && src[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if src[i] == b'*' && i + 1 < src.len() && src[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            tokens.push(Token {
                kind: TokKind::Comment,
                start,
                end: i,
            });
            continue;
        }

        // Verbatim string `@"..."`
        if b == b'@' && i + 1 < src.len() && src[i + 1] == b'"' {
            let start = i;
            let end = scan_string_end(src, i + 2, b'"', true);
            tokens.push(Token {
                kind: TokKind::String,
                start,
                end,
            });
            i = end;
            continue;
        }

        // String `"..."` or char `'...'`
        if b == b'"' || b == b'\'' {
            let start = i;
            let end = scan_string_end(src, i + 1, b, false);
            tokens.push(Token {
                kind: TokKind::String,
                start,
                end,
            });
            i = end;
            continue;
        }

        // Numbers
        if b.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < src.len()
                && (src[i].is_ascii_alphanumeric()
                    || src[i] == b'_'
                    || (src[i] == b'.' && i + 1 < src.len() && src[i + 1].is_ascii_digit()))
            {
                i += 1;
            }
            tokens.push(Token {
                kind: TokKind::Number,
                start,
                end: i,
            });
            continue;
        }

        // Identifiers & keywords
        if is_ident_start(b) {
            let start = i;
            let end = scan_ident_end(src, start);
            let text = core::str::from_utf8(&src[start..end]).unwrap_or("");
            let kind = if is_keyword(text) {
                TokKind::Keyword
            } else if b.is_ascii_uppercase() {
                TokKind::UpperIdent
            } else {
                TokKind::Ident
            };
            tokens.push(Token { kind, start, end });
            i = end;
            continue;
        }

        // Single/multi-char punctuation
        let kind = match b {
            b'{' => TokKind::LBrace,
            b'}' => TokKind::RBrace,
            b'(' => TokKind::LParen,
            b')' => TokKind::RParen,
            b'.' => TokKind::Dot,
            b':' => TokKind::Colon,
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'!' | b'<' | b'>' | b'&' | b'|' | b'^'
            | b'~' => TokKind::Operator,
            _ => TokKind::Other,
        };
        tokens.push(Token {
            kind,
            start: i,
            end: i + 1,
        });
        i += 1;
    }
    tokens
}

fn tok_str<'a>(src: &'a [u8], tok: Token) -> &'a str {
    core::str::from_utf8(&src[tok.start..tok.end]).unwrap_or("")
}

// ── Syntax spans ────────────────────────────────────────────────────────────

pub fn parse_spans(src: &[u8], max_spans: usize) -> Vec<(u32, u32, u8)> {
    let tokens = tokenize(src);
    let mut spans = Vec::with_capacity(tokens.len().min(max_spans));
    for (idx, tok) in tokens.iter().enumerate() {
        if spans.len() >= max_spans {
            break;
        }
        let scope = match tok.kind {
            TokKind::Keyword => SCOPE_KEYWORD,
            TokKind::String => SCOPE_STRING,
            TokKind::Number => SCOPE_NUMBER,
            TokKind::Comment => SCOPE_COMMENT,
            TokKind::UpperIdent => SCOPE_TYPE,
            TokKind::Operator => SCOPE_OPERATOR,
            TokKind::Ident => {
                let next_is_lparen = tokens
                    .get(idx + 1)
                    .map(|t| t.kind == TokKind::LParen)
                    .unwrap_or(false);
                if next_is_lparen {
                    SCOPE_FUNCTION
                } else {
                    SCOPE_VARIABLE
                }
            }
            _ => 0,
        };
        if scope != 0 {
            spans.push((tok.start as u32, (tok.end - tok.start) as u32, scope));
        }
    }
    spans
}

// ── Retrieval chunks ────────────────────────────────────────────────────────

fn find_decl_end(src: &[u8], tokens: &[Token], start_tok: usize) -> usize {
    let mut brace_depth = 0usize;
    let mut saw_brace = false;
    let start_line_end = src[tokens[start_tok].start..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| tokens[start_tok].start + p)
        .unwrap_or(src.len());

    for tok in &tokens[start_tok..] {
        match tok.kind {
            TokKind::LBrace => {
                saw_brace = true;
                brace_depth += 1;
            }
            TokKind::RBrace => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                    if saw_brace && brace_depth == 0 {
                        return tok.end;
                    }
                }
            }
            _ => {}
        }
    }
    start_line_end
}

pub fn retrieval_chunks(src: &[u8], max_chunks: usize) -> Vec<(u32, u32, String, u8)> {
    let tokens = tokenize(src);
    let mut chunks = Vec::new();
    let mut brace_depth = 0usize;
    let mut i = 0usize;

    while i < tokens.len() && chunks.len() < max_chunks {
        let tok = tokens[i];
        match tok.kind {
            TokKind::LBrace => {
                brace_depth += 1;
                i += 1;
                continue;
            }
            TokKind::RBrace => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
                continue;
            }
            TokKind::Keyword => {}
            _ => {
                i += 1;
                continue;
            }
        }

        let kw = tok_str(src, tok);
        match kw {
            "module" if brace_depth == 0 => {
                if let Some(name_tok) = tokens.get(i + 1) {
                    if matches!(name_tok.kind, TokKind::Ident | TokKind::UpperIdent) {
                        let name = tok_str(src, *name_tok);
                        let end = find_decl_end(src, &tokens, i);
                        chunks.push((
                            tok.start as u32,
                            (end - tok.start) as u32,
                            format!("module {name}"),
                            KIND_MODULE,
                        ));
                    }
                }
            }
            "type" | "struct" | "effect" | "alias" if brace_depth == 0 => {
                let mut name_idx = i + 1;
                while let Some(t) = tokens.get(name_idx) {
                    if t.kind == TokKind::Keyword
                        && matches!(tok_str(src, *t), "rec" | "co" | "open" | "extend" | "linear" | "value" | "reference" | "named" | "scoped")
                    {
                        name_idx += 1;
                    } else {
                        break;
                    }
                }
                if let Some(name_tok) = tokens.get(name_idx) {
                    if matches!(name_tok.kind, TokKind::Ident | TokKind::UpperIdent) {
                        let name = tok_str(src, *name_tok);
                        let end = find_decl_end(src, &tokens, i);
                        chunks.push((
                            tok.start as u32,
                            (end - tok.start) as u32,
                            format!("type {name}"),
                            KIND_TYPE,
                        ));
                    }
                }
            }
            "con" => {
                if let Some(name_tok) = tokens.get(i + 1) {
                    if matches!(name_tok.kind, TokKind::Ident | TokKind::UpperIdent) {
                        let name = tok_str(src, *name_tok);
                        chunks.push((
                            tok.start as u32,
                            (name_tok.end - tok.start) as u32,
                            format!("enum_case {name}"),
                            KIND_ENUM_CASE,
                        ));
                    }
                }
            }
            "fun" | "ctl" | "extern" | "handler" if brace_depth <= 1 => {
                if let Some(name_tok) = tokens.get(i + 1) {
                    if matches!(name_tok.kind, TokKind::Ident | TokKind::UpperIdent) {
                        let name = tok_str(src, *name_tok);
                        let end = find_decl_end(src, &tokens, i);
                        chunks.push((
                            tok.start as u32,
                            (end - tok.start) as u32,
                            format!("function {name}"),
                            KIND_FUNCTION,
                        ));
                    }
                }
            }
            "val" | "var" if brace_depth == 0 => {
                if let Some(name_tok) = tokens.get(i + 1) {
                    if matches!(name_tok.kind, TokKind::Ident | TokKind::UpperIdent) {
                        let name = tok_str(src, *name_tok);
                        let end = find_decl_end(src, &tokens, i);
                        chunks.push((
                            tok.start as u32,
                            (end - tok.start) as u32,
                            format!("property {name}"),
                            KIND_PROPERTY,
                        ));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    chunks
}

// ── Call sites ──────────────────────────────────────────────────────────────

pub fn call_sites(src: &[u8], max_sites: usize) -> Vec<(u32, String)> {
    let tokens = tokenize(src);
    let mut sites = Vec::new();
    for i in 0..tokens.len() {
        if sites.len() >= max_sites {
            break;
        }
        let tok = tokens[i];
        if !matches!(tok.kind, TokKind::Ident | TokKind::UpperIdent) {
            continue;
        }
        let prev_kw = i
            .checked_sub(1)
            .and_then(|p| tokens.get(p))
            .filter(|t| t.kind == TokKind::Keyword)
            .map(|t| tok_str(src, *t));
        if matches!(
            prev_kw,
            Some("fun" | "ctl" | "extern" | "handler" | "type" | "struct" | "effect" | "con" | "module" | "import" | "alias")
        ) {
            continue;
        }
        let next_is_lparen = tokens
            .get(i + 1)
            .map(|t| t.kind == TokKind::LParen)
            .unwrap_or(false);
        let prev_is_with = prev_kw == Some("with");
        if next_is_lparen || prev_is_with {
            let name = tok_str(src, tok);
            if !name.is_empty() {
                sites.push((tok.start as u32, name.to_string()));
            }
        }
    }
    sites
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_KK: &str = r#"
module demo/calc

import std/os/file

pub effect ask<a>
  ctl ask() : a

pub type shape
  con Circle(radius : float64)
  con Rect(w : float64, h : float64)

pub val pi-approx : float64 = 3.14159

pub fun add(a : int, b : int) : total int {
  a + b
}

pub fun read-config(path : string) : <exn,fsys> string {
  read-text-file(path.path)
}

pub fun greet(name : string) : console () {
  println("hello " ++ name)
}
"#;

    #[test]
    fn parses_koka_retrieval_chunks() {
        let chunks = retrieval_chunks(SAMPLE_KK.as_bytes(), 64);
        let labels: Vec<&str> = chunks.iter().map(|c| c.2.as_str()).collect();
        assert!(labels.contains(&"module demo"));
        assert!(labels.contains(&"type ask"));
        assert!(labels.contains(&"type shape"));
        assert!(labels.contains(&"enum_case Circle"));
        assert!(labels.contains(&"enum_case Rect"));
        assert!(labels.contains(&"property pi-approx"));
        assert!(labels.contains(&"function add"));
        assert!(labels.contains(&"function read-config"));
        assert!(labels.contains(&"function greet"));
    }

    #[test]
    fn extracts_hyphenated_call_sites() {
        let sites = call_sites(SAMPLE_KK.as_bytes(), 64);
        let names: Vec<&str> = sites.iter().map(|s| s.1.as_str()).collect();
        assert!(names.contains(&"read-text-file"));
        assert!(names.contains(&"println"));
        assert!(!names.contains(&"add"));
        assert!(!names.contains(&"greet"));
    }

    #[test]
    fn handles_nested_block_comments() {
        let src = b"/* outer /* inner */ still comment */ fun ok() : int { 1 }";
        let chunks = retrieval_chunks(src, 16);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, "function ok");
    }
}
