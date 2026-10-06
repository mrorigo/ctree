To dynamically support multiple languages in a Tree-sitter CLI, you need:

1. **A Language Enum & File Extension Mapper:** Maps extensions (`.rs`, `.py`, `.ts`, `.tsx`, `.go`, `.c`, `.h`) to a unified language definition.
2. **Grammar & Config Dispatch:** Each language provides both its `tree_sitter::Language` handle and a node classifier (since node names differ slightly across grammars, e.g. Python's `if_statement` vs. Rust's `if_expression`).
3. **Stateless Thread Integration:** Because `tree_sitter::Language` is thread-safe (`Copy`/`Sync`), workers can look up the language by extension and load it into thread-local `Parser` instances.

---

### `Cargo.toml`

```toml
[package]
name = "ctree"
version = "0.3.0"
edition = "2021"

[dependencies]
rayon = "1.10"
walkdir = "2.5"
tree-sitter = "0.24"

# Official Tree-sitter grammars
tree-sitter-rust = "0.23"
tree-sitter-python = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-go = "0.23"
tree-sitter-c = "0.23"

```

---

### `src/languages.rs`

Define the supported language abstractions and their grammar-specific AST rules.

```rust
use tree_sitter::{Language, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    Python,
    TypeScript,
    Tsx,
    Go,
    C,
}

impl SupportedLanguage {
    /// Detect language by file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" | "jsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "c" | "h" => Some(Self::C),
            _ => None,
        }
    }

    /// Retrieve the corresponding Tree-sitter Language handle
    pub fn tree_sitter_language(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TypeScript (TSX)",
            Self::Go => "Go",
            Self::C => "C",
        }
    }

    /// Structural containers that increase nesting penalties
    pub fn is_nesting_node(self, node: Node) -> bool {
        let k = node.kind();
        match self {
            Self::Rust => matches!(
                k,
                "function_item"
                    | "closure_expression"
                    | "if_expression"
                    | "for_expression"
                    | "while_expression"
                    | "loop_expression"
                    | "match_expression"
            ),
            Self::Python => matches!(
                k,
                "function_definition"
                    | "async_function_definition"
                    | "lambda"
                    | "if_statement"
                    | "for_statement"
                    | "while_statement"
                    | "match_statement"
            ),
            Self::TypeScript | Self::Tsx => matches!(
                k,
                "function_declaration"
                    | "function_expression"
                    | "arrow_function"
                    | "method_definition"
                    | "if_statement"
                    | "for_statement"
                    | "for_in_statement"
                    | "while_statement"
                    | "do_statement"
                    | "switch_statement"
            ),
            Self::Go => matches!(
                k,
                "function_declaration"
                    | "method_declaration"
                    | "func_literal"
                    | "if_statement"
                    | "for_statement"
                    | "expression_switch_statement"
                    | "type_switch_statement"
                    | "select_statement"
            ),
            Self::C => matches!(
                k,
                "function_definition"
                    | "if_statement"
                    | "for_statement"
                    | "while_statement"
                    | "do_statement"
                    | "switch_statement"
            ),
        }
    }

    /// Branching points (base cost + nesting penalty)
    pub fn is_branching_node(self, node: Node) -> bool {
        let k = node.kind();
        match self {
            Self::Rust => {
                if matches!(k, "if_expression" | "for_expression" | "while_expression" | "loop_expression" | "match_arm") {
                    return true;
                }
                is_binary_logical(node, &["&&", "||"])
            }
            Self::Python => {
                if matches!(k, "if_statement" | "for_statement" | "while_statement" | "case_clause" | "conditional_expression") {
                    return true;
                }
                is_binary_logical(node, &["and", "or"])
            }
            Self::TypeScript | Self::Tsx => {
                if matches!(k, "if_statement" | "for_statement" | "for_in_statement" | "while_statement" | "do_statement" | "switch_case" | "ternary_expression") {
                    return true;
                }
                is_binary_logical(node, &["&&", "||", "??"])
            }
            Self::Go => {
                if matches!(k, "if_statement" | "for_statement" | "expression_case_clause" | "type_case_clause" | "communication_case") {
                    return true;
                }
                is_binary_logical(node, &["&&", "||"])
            }
            Self::C => {
                if matches!(k, "if_statement" | "for_statement" | "while_statement" | "do_statement" | "case_statement" | "conditional_expression") {
                    return true;
                }
                is_binary_logical(node, &["&&", "||"])
            }
        }
    }

    /// Flat interrupts (+1 without nesting penalty)
    pub fn is_flat_jump_node(self, node: Node) -> bool {
        let k = node.kind();
        match self {
            Self::Rust => matches!(k, "break_expression" | "continue_expression" | "return_expression"),
            Self::Python => matches!(k, "break_statement" | "continue_statement" | "return_statement" | "raise_statement"),
            Self::TypeScript | Self::Tsx => matches!(k, "break_statement" | "continue_statement" | "return_statement" | "throw_statement"),
            Self::Go => matches!(k, "break_statement" | "continue_statement" | "return_statement" | "goto_statement"),
            Self::C => matches!(k, "break_statement" | "continue_statement" | "return_statement" | "goto_statement"),
        }
    }
}

fn is_binary_logical(node: Node, ops: &[&str]) -> bool {
    if node.kind() == "binary_expression" || node.kind() == "boolean_operator" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if ops.contains(&child.kind()) {
                return true;
            }
        }
    }
    false
}

```

---

### `src/main.rs`

The directory scanner looks up the grammar dynamically based on the file extension and reports metrics grouped by language.

```rust
mod languages;

use languages::SupportedLanguage;
use rayon::prelude::*;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tree-sitter::Parser;
use walkdir::WalkDir;

#[derive(Default, Debug, Clone)]
pub struct LineStats {
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
}

#[derive(Default, Debug, Clone)]
pub struct AstStats {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub cognitive_complexity: usize,
}

#[derive(Debug, Clone)]
pub struct FileReport {
    pub path: PathBuf,
    pub lang: SupportedLanguage,
    pub lines: LineStats,
    pub ast: AstStats,
}

#[derive(Default, Debug)]
pub struct LangSummary {
    pub files: usize,
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
    pub nodes: usize,
    pub peak_depth: usize,
    pub complexity: usize,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let root_path = args.get(1).map(String::as_str).unwrap_or(".");
    let start_time = Instant::now();

    // 1. Gather all files supported by any registered grammar
    let candidates: Vec<(PathBuf, SupportedLanguage)> = WalkDir::new(root_path)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') && name != "target" && name != "node_modules" && name != "vendor"
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let ext = e.path().extension()?.to_str()?;
            let lang = SupportedLanguage::from_extension(ext)?;
            Some((e.into_path(), lang))
        })
        .collect();

    // 2. Parallel processing using Rayon
    let reports: Vec<FileReport> = candidates
        .par_iter()
        .filter_map(|(path, lang)| process_file(path, *lang))
        .collect();

    let elapsed = start_time.elapsed();

    // 3. Aggregate totals per language
    let mut by_language: HashMap<SupportedLanguage, LangSummary> = HashMap::new();
    for r in &reports {
        let entry = by_language.entry(r.lang).or_default();
        entry.files += 1;
        entry.code += r.lines.code;
        entry.comments += r.lines.comments;
        entry.blanks += r.lines.blanks;
        entry.nodes += r.ast.total_nodes;
        entry.complexity += r.ast.cognitive_complexity;
        if r.ast.max_depth > entry.peak_depth {
            entry.peak_depth = r.ast.max_depth;
        }
    }

    render_summary_table(&by_language, elapsed);
}

fn process_file(path: &Path, lang: SupportedLanguage) -> Option<FileReport> {
    let source_code = fs::read_to_string(path).ok()?;

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = parser.parse(&source_code, None)?;

    let lines = calculate_line_metrics(&source_code);
    let ast = analyze_ast(&tree, lang);

    Some(FileReport {
        path: path.to_path_buf(),
        lang,
        lines,
        ast,
    })
}

fn calculate_line_metrics(source: &str) -> LineStats {
    let mut code = 0;
    let mut comments = 0;
    let mut blanks = 0;

    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            blanks += 1;
        } else if line.starts_with("//") || line.starts_with('#') || line.starts_with("/*") {
            comments += 1;
        } else {
            code += 1;
        }
    }

    LineStats { code, comments, blanks }
}

fn analyze_ast(tree: &tree_sitter::Tree, lang: SupportedLanguage) -> AstStats {
    let mut stats = AstStats::default();
    let mut cursor = tree.walk();

    let mut depth: usize = 0;
    let mut nesting_level: usize = 0;

    'walk: loop {
        let node = cursor.node();

        stats.total_nodes += 1;
        if depth > stats.max_depth {
            stats.max_depth = depth;
        }

        if lang.is_nesting_node(node) {
            nesting_level += 1;
        } else if lang.is_branching_node(node) {
            stats.cognitive_complexity += 1 + nesting_level;
        } else if lang.is_flat_jump_node(node) {
            stats.cognitive_complexity += 1;
        }

        if cursor.goto_first_child() {
            depth += 1;
            continue 'walk;
        }

        if cursor.goto_next_sibling() {
            continue 'walk;
        }

        loop {
            if !cursor.goto_parent() {
                break 'walk;
            }
            depth -= 1;

            let parent = cursor.node();
            if lang.is_nesting_node(parent) {
                nesting_level = nesting_level.saturating_sub(1);
            }

            if cursor.goto_next_sibling() {
                break;
            }
        }
    }

    stats
}

fn render_summary_table(
    by_lang: &HashMap<SupportedLanguage, LangSummary>,
    elapsed: std::time::Duration,
) {
    println!(
        "{:<18} {:>6} {:>8} {:>8} {:>10} {:>7} {:>8}",
        "LANGUAGE", "FILES", "CODE", "COMMENTS", "NODES", "DEPTH", "COGN"
    );
    println!("{}", "-".repeat(73));

    let mut entries: Vec<_> = by_lang.iter().collect();
    entries.sort_by_key(|(_, s)| std::cmp::Reverse(s.code));

    let mut tot_files = 0;
    let mut tot_code = 0;
    let mut tot_comments = 0;
    let mut tot_nodes = 0;
    let mut tot_complexity = 0;
    let mut max_depth = 0;

    for (lang, s) in entries {
        tot_files += s.files;
        tot_code += s.code;
        tot_comments += s.comments;
        tot_nodes += s.nodes;
        tot_complexity += s.complexity;
        if s.peak_depth > max_depth {
            max_depth = s.peak_depth;
        }

        println!(
            "{:<18} {:>6} {:>8} {:>8} {:>10} {:>7} {:>8}",
            lang.display_name(),
            s.files,
            s.code,
            s.comments,
            s.nodes,
            s.peak_depth,
            s.complexity
        );
    }

    println!("{}", "=".repeat(73));
    println!(
        "{:<18} {:>6} {:>8} {:>8} {:>10} {:>7} {:>8}",
        format!("TOTAL ({:.2?})", elapsed),
        tot_files,
        tot_code,
        tot_comments,
        tot_nodes,
        max_depth,
        tot_complexity
    );
}

```

---

### Key Design Aspects

1. **Lightweight Dynamic Registry:** `SupportedLanguage` is an enum of lightweight `Copy` identifiers. Adding a language is as simple as adding an enum variant, an entry in `from_extension()`, and linking its corresponding crate grammar in `tree_sitter_language()`.
2. **Grammar Encapsulation:** Language-specific dialect differences (such as `TypeScript` vs. `TSX`, Python indentation statements, or C ternary operators) are kept behind `lang.is_branching_node()` and `lang.is_nesting_node()` methods.
3. **Zero Contention in Rayon:** Because no grammar objects or parsers are shared across thread boundaries, threads run completely decoupled at native speed.
