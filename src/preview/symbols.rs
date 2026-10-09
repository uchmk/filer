//! Declarations (functions, types, modules) picked out of the scopes the
//! highlighter already computes, for the outline next to source previews.
//!
//! Sublime grammars name what a line declares with `entity.name.<kind>`, so
//! one pass over the parse serves both the colors and the outline, for every
//! language bat ships a grammar for.

use std::sync::OnceLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{Scope, ScopeStack, ScopeStackOp};

use super::TocEntry;

/// Deeper nesting is shown at this level.
const MAX_LEVEL: usize = 4;
/// A name longer than this is not one anybody reads in an outline.
const MAX_NAME: usize = 80;
/// Kinds shown with their kind in front; functions go bare.
const TAGGED: [&str; 10] =
    ["class", "struct", "enum", "union", "trait", "interface", "impl", "namespace", "macro", "type"];

/// Fed the parse of a file line by line; keeps the first declaration on each
/// line, nested by how far it is indented.
#[derive(Default)]
pub struct Collector {
    stack: ScopeStack,
    /// Indents of the enclosing entries, outermost first.
    indents: Vec<usize>,
    entries: Vec<TocEntry>,
}

impl Collector {
    pub fn feed(&mut self, line_idx: usize, line: &str, ops: &[(usize, ScopeStackOp)]) {
        let mut found = None;
        for (piece, op) in ScopeRegionIterator::new(ops, line) {
            // A bad pop only means the grammar and the stack disagree; the
            // colors carry on regardless, and so does the outline.
            let _ = self.stack.apply(op);
            if found.is_none() && !piece.trim().is_empty() {
                found = classify(self.stack.as_slice()).map(|kind| (kind, piece.trim()));
            }
        }
        let Some((kind, name)) = found else { return };
        let label = match kind {
            Kind::Function => name.to_owned(),
            Kind::Tagged(tag) => format!("{tag} {name}"),
            // `[dependencies.serde]` comes in pieces; the line says it whole.
            Kind::Table => {
                let line = line.trim();
                line.rfind(']').map_or(line, |end| &line[..=end]).to_owned()
            }
        };

        let indent = line.len() - line.trim_start().len();
        while self.indents.last().is_some_and(|&w| w >= indent) {
            self.indents.pop();
        }
        self.indents.push(indent);
        let level = self.indents.len().min(MAX_LEVEL);
        self.entries.push(TocEntry {
            level: level as u8,
            label: format!("{}{}", "  ".repeat(level - 1), clip(&label)),
            line: line_idx,
        });
    }

    /// The entries found in the lines fed so far.
    pub fn so_far(&self) -> Vec<TocEntry> {
        self.entries.clone()
    }

    pub fn finish(self) -> Vec<TocEntry> {
        self.entries
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Function,
    Tagged(&'static str),
    /// A TOML table header.
    Table,
}

struct Scopes {
    entity: Scope,
    call: Scope,
    /// Where TypeScript names types it merely refers to.
    type_refs: [Scope; 3],
    /// Grammars that also use a bare `entity.name.type` for references.
    typescript: [Scope; 2],
}

fn scopes() -> &'static Scopes {
    static SCOPES: OnceLock<Scopes> = OnceLock::new();
    SCOPES.get_or_init(|| {
        let s = |name| Scope::new(name).expect("valid scope");
        Scopes {
            entity: s("entity.name"),
            call: s("meta.function-call"),
            type_refs: [s("meta.type.annotation"), s("meta.return.type"), s("meta.type.parameters")],
            typescript: [s("source.ts"), s("source.tsx")],
        }
    })
}

/// What the innermost `entity.name` on the stack declares, if anything.
fn classify(stack: &[Scope]) -> Option<Kind> {
    let s = scopes();
    let name = stack.iter().rev().find(|sc| s.entity.is_prefix_of(**sc))?;
    let within = |p: &Scope| stack.iter().any(|sc| p.is_prefix_of(*sc));
    let full = name.build_string();
    let mut atoms = full.split('.').skip(2);
    let tag = |t: &str| TAGGED.iter().find(|&&k| k == t).map(|&k| Kind::Tagged(k));
    match atoms.next()? {
        // TypeScript's and PHP's grammars name calls like definitions.
        "function" => (!within(&s.call)).then_some(Kind::Function),
        "table" => Some(Kind::Table),
        "module" => Some(Kind::Tagged("mod")),
        // `entity.name.type.class.ts`, `entity.name.type.go`, ...
        "type" => {
            if s.type_refs.iter().any(within) {
                return None;
            }
            match atoms.next() {
                Some("alias") => Some(Kind::Tagged("type")),
                Some("module") => Some(Kind::Tagged("namespace")),
                Some(sub) if tag(sub).is_some() => tag(sub),
                _ if s.typescript.iter().any(within) => None,
                _ => Some(Kind::Tagged("type")),
            }
        }
        kind => tag(kind),
    }
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(MAX_NAME) {
        Some((at, _)) => format!("{}…", &s[..at]),
        None => s.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use syntect::parsing::{ParseState, SyntaxSet};
    use syntect::util::LinesWithEndings;

    use super::*;

    fn syntaxes() -> &'static SyntaxSet {
        static SET: OnceLock<SyntaxSet> = OnceLock::new();
        SET.get_or_init(two_face::syntax::extra_newlines)
    }

    fn outline(ext: &str, src: &str) -> Vec<(usize, u8, String)> {
        let ss = syntaxes();
        let mut ps = ParseState::new(ss.find_syntax_by_extension(ext).unwrap());
        let mut c = Collector::default();
        for (i, line) in LinesWithEndings::from(src).enumerate() {
            c.feed(i, line, &ps.parse_line(line, ss).unwrap());
        }
        let entries = c.finish();
        let lines = src.lines().count();
        assert!(entries.iter().all(|e| e.line < lines), "{entries:?}");
        entries.into_iter().map(|e| (e.line, e.level, e.label)).collect()
    }

    fn labels(ext: &str, src: &str) -> Vec<String> {
        outline(ext, src).into_iter().map(|e| e.2).collect()
    }

    #[test]
    fn rust_items_nest_by_indent() {
        let src = "\
pub struct App {
    x: u8,
}

enum E { A }
trait T { fn t(&self); }

impl App {
    pub fn new() -> Self {
        let s: Foo = foo();
        Self::bar()
    }
}

mod tests {
    fn helper() {}
}
fn main() {}
";
        assert_eq!(
            outline("rs", src),
            [
                (0, 1, "struct App".into()),
                (4, 1, "enum E".into()),
                (5, 1, "trait T".into()),
                (7, 1, "impl App".into()),
                (8, 2, "  new".into()),
                (14, 1, "mod tests".into()),
                (15, 2, "  helper".into()),
                (17, 1, "main".into()),
            ]
        );
    }

    #[test]
    fn python_methods_sit_under_their_class() {
        let src = "class Foo(Base):\n    def method(self):\n        call()\n\ndef top():\n    pass\n";
        assert_eq!(
            outline("py", src),
            [(0, 1, "class Foo".into()), (1, 2, "  method".into()), (4, 1, "top".into())]
        );
    }

    #[test]
    fn typescript_skips_calls_and_type_references() {
        let src = "\
const x = new Foo<Bar>();
let y: Map<string, Foo> = z as Baz;
function f(a: Foo): Bar { g(); return x.y(); }
export class C extends D implements I {
  method(x: number) {}
}
interface I { m(): void }
type T = A | B;
enum E { A }
namespace N {}
const arrow = (x) => x;
";
        assert_eq!(
            labels("ts", src),
            ["f", "class C", "  method", "interface I", "type T", "enum E", "namespace N", "arrow"]
        );
        // JSX.Element in a return type is a reference, not a namespace.
        assert_eq!(labels("tsx", "export function App(): JSX.Element { return <Foo /> }\n"), ["App"]);
    }

    #[test]
    fn go_and_c() {
        let go = "package main\n\nfunc main() { f() }\nfunc (r *R) Method() {}\ntype S struct {}\n";
        assert_eq!(labels("go", go), ["main", "Method", "type S"]);
        let c = "#include <stdio.h>\nstruct s { int a; };\nint main(void) {\n    printf(\"x\");\n}\n";
        assert_eq!(labels("c", c), ["struct s", "main"]);
    }

    #[test]
    fn toml_tables() {
        let src = "[package]\nname = \"x\"\n\n[dependencies.serde] # pinned\nversion = \"1\"\n[[bin]]\n";
        assert_eq!(labels("toml", src), ["[package]", "[dependencies.serde]", "[[bin]]"]);
    }

    #[test]
    fn plain_prose_has_no_outline() {
        assert!(labels("rs", "// just a comment\nlet x = 1;\n").is_empty());
        assert!(labels("yaml", "key:\n  nested: 1\n").is_empty());
    }

    #[test]
    fn deep_nesting_is_capped() {
        let src = "mod a {\n mod b {\n  mod c {\n   mod d {\n    mod e {\n     fn f() {}\n}}}}}\n";
        let levels: Vec<u8> = outline("rs", src).into_iter().map(|e| e.1).collect();
        assert_eq!(levels, [1, 2, 3, 4, 4, 4]);
    }
}
