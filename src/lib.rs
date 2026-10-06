// Rust guideline compliant 2026-10-06
//! Syntax-aware source code metrics for supported programming languages.

pub mod languages;

use languages::SupportedLanguage;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::Parser;

/// Counts source lines by their primary content category.
#[derive(Default, Debug, Clone, Serialize)]
pub struct LineStats {
    /// Lines containing code, including mixed code and comment lines.
    pub code: usize,
    /// Lines containing comments only.
    pub comments: usize,
    /// Empty or whitespace-only lines.
    pub blanks: usize,
}

/// Summarizes syntax tree size, depth, and cognitive complexity.
#[derive(Default, Debug, Clone, Serialize)]
pub struct AstStats {
    /// Number of named syntax tree nodes.
    pub total_nodes: usize,
    /// Maximum syntax tree depth, with the root at depth zero.
    pub max_depth: usize,
    /// Cognitive complexity using language-specific branch and nesting rules.
    pub cognitive_complexity: usize,
    /// Number of syntax errors and missing nodes reported by the parser.
    pub syntax_errors: usize,
}

/// Metrics collected for one source file.
#[derive(Debug, Clone, Serialize)]
pub struct FileReport {
    /// Path of the analyzed file.
    pub path: PathBuf,
    /// Detected source language.
    pub lang: SupportedLanguage,
    /// Source line metrics.
    pub lines: LineStats,
    /// Syntax tree metrics.
    pub ast: AstStats,
    /// Per-function complexity metrics.
    pub functions: Vec<FunctionReport>,
}

/// Complexity metrics for one function-like syntax scope.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionReport {
    /// Function name, or `<anonymous>` for closures and lambdas.
    pub name: String,
    /// One-based source line where the function begins.
    pub line: usize,
    /// Cognitive complexity contained by this function, excluding nested functions.
    pub complexity: usize,
}

/// Aggregated metrics for one language.
#[derive(Default, Debug, Clone, Serialize)]
pub struct LangSummary {
    /// Number of successfully parsed files.
    pub files: usize,
    /// Number of code lines.
    pub code: usize,
    /// Number of comment-only lines.
    pub comments: usize,
    /// Number of blank lines.
    pub blanks: usize,
    /// Number of named syntax nodes.
    pub nodes: usize,
    /// Maximum syntax tree depth among files.
    pub peak_depth: usize,
    /// Total cognitive complexity.
    pub complexity: usize,
    /// Syntax errors and missing nodes across files.
    pub syntax_errors: usize,
}

/// Options that control a source tree scan.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Root directories or files to scan.
    pub paths: Vec<PathBuf>,
    /// Include hidden files and directories.
    pub hidden: bool,
    /// Include files ignored by gitignore rules.
    pub no_ignore: bool,
    /// Include individual source file rows in output.
    pub files: bool,
    /// Skip syntax parsing, while retaining line metrics.
    pub no_ast: bool,
    /// Optional set of language display names to include.
    pub languages: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            paths: vec![PathBuf::from(".")],
            hidden: false,
            no_ignore: false,
            files: false,
            no_ast: false,
            languages: Vec::new(),
        }
    }
}

/// Scans paths and returns file-level metrics and any read/parse setup errors.
pub fn scan(options: &ScanOptions) -> (Vec<FileReport>, Vec<String>) {
    let mut builder = ignore::WalkBuilder::new(&options.paths[0]);
    for path in options.paths.iter().skip(1) {
        builder.add(path);
    }
    builder
        .hidden(!options.hidden)
        .git_ignore(!options.no_ignore)
        .git_global(!options.no_ignore)
        .git_exclude(!options.no_ignore);
    let candidates: Vec<(PathBuf, SupportedLanguage)> = builder
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let language = entry
                .path()
                .extension()?
                .to_str()
                .and_then(SupportedLanguage::from_extension)?;
            let allowed = options.languages.is_empty()
                || options
                    .languages
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(language.display_name()));
            allowed.then(|| (entry.into_path(), language))
        })
        .collect();
    let processed: Vec<Result<FileReport, String>> = candidates
        .par_iter()
        .map(|(path, language)| process_file(path, *language, options.no_ast))
        .collect();
    let mut reports = Vec::new();
    let mut errors = Vec::new();
    for item in processed {
        match item {
            Ok(report) => reports.push(report),
            Err(error) => errors.push(error),
        }
    }
    reports.sort_by(|left, right| left.path.cmp(&right.path));
    errors.sort();
    (reports, errors)
}

fn process_file(path: &Path, lang: SupportedLanguage, no_ast: bool) -> Result<FileReport, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let source = String::from_utf8_lossy(&bytes);
    let lines = calculate_line_metrics(&source, lang);
    let (ast, functions) = if no_ast {
        (AstStats::default(), Vec::new())
    } else {
        let mut parser = Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|error| format!("{}: parser setup failed: {error}", path.display()))?;
        let tree = parser
            .parse(source.as_ref(), None)
            .ok_or_else(|| format!("{}: parser canceled", path.display()))?;
        (
            analyze_ast(&tree, lang),
            analyze_functions(&tree, lang, source.as_ref()),
        )
    };
    Ok(FileReport {
        path: path.to_path_buf(),
        lang,
        lines,
        ast,
        functions,
    })
}

fn calculate_line_metrics(source: &str, lang: SupportedLanguage) -> LineStats {
    let mut stats = LineStats::default();
    let mut block_comment = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            stats.blanks += 1;
            continue;
        }
        let hash_comment = lang == SupportedLanguage::Python && line.starts_with('#');
        if block_comment {
            stats.comments += 1;
            if line.contains("*/") {
                block_comment = false;
            }
        } else if hash_comment || line.starts_with("//") || line.starts_with("#!") {
            stats.comments += 1;
        } else if line.starts_with("/*") {
            stats.comments += 1;
            block_comment = !line.contains("*/");
        } else {
            stats.code += 1;
        }
    }
    stats
}

fn analyze_ast(tree: &tree_sitter::Tree, lang: SupportedLanguage) -> AstStats {
    let mut stats = AstStats::default();
    let mut cursor = tree.walk();
    let mut depth = 0usize;
    let mut nesting = 0usize;
    loop {
        let node = cursor.node();
        if node.is_named() {
            stats.total_nodes += 1;
        }
        stats.max_depth = stats.max_depth.max(depth);
        if node.is_error() || node.is_missing() {
            stats.syntax_errors += 1;
        }
        if lang.is_branching_node(node) {
            stats.cognitive_complexity += 1 + nesting;
        } else if lang.is_flat_jump_node(node) {
            stats.cognitive_complexity += 1;
        }
        if lang.is_nesting_node(node) {
            nesting += 1;
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return stats;
            }
            depth = depth.saturating_sub(1);
            if lang.is_nesting_node(cursor.node()) {
                nesting = nesting.saturating_sub(1);
            }
        }
    }
}

fn analyze_functions(
    tree: &tree_sitter::Tree,
    lang: SupportedLanguage,
    source: &str,
) -> Vec<FunctionReport> {
    let mut reports = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if lang.is_function_node(node) {
            let name = node
                .child_by_field_name("name")
                .map(|name| {
                    name.utf8_text(source.as_bytes())
                        .unwrap_or("<anonymous>")
                        .to_owned()
                })
                .unwrap_or_else(|| "<anonymous>".to_owned());
            reports.push(FunctionReport {
                name,
                line: node.start_position().row + 1,
                complexity: function_complexity(node, lang),
            });
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return reports;
            }
        }
    }
}

fn function_complexity(function: tree_sitter::Node<'_>, lang: SupportedLanguage) -> usize {
    let mut complexity = 0;
    let mut nesting = 0usize;
    let mut cursor = function.walk();
    loop {
        let node = cursor.node();
        let is_root = node.id() == function.id();
        let is_nested_function = !is_root && lang.is_function_node(node);
        if !is_root && !is_nested_function {
            if lang.is_branching_node(node) {
                complexity += 1 + nesting;
            } else if lang.is_flat_jump_node(node) {
                complexity += 1;
            }
            if lang.is_nesting_node(node) {
                nesting += 1;
            }
        }
        if !is_nested_function && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return complexity;
            }
            let parent = cursor.node();
            if parent.id() != function.id()
                && !lang.is_function_node(parent)
                && lang.is_nesting_node(parent)
            {
                nesting = nesting.saturating_sub(1);
            }
        }
    }
}

/// Aggregates file reports by language.
pub fn summarize(reports: &[FileReport]) -> BTreeMap<String, LangSummary> {
    let mut summaries = BTreeMap::new();
    for report in reports {
        let summary: &mut LangSummary = summaries
            .entry(report.lang.display_name().to_owned())
            .or_default();
        summary.files += 1;
        summary.code += report.lines.code;
        summary.comments += report.lines.comments;
        summary.blanks += report.lines.blanks;
        summary.nodes += report.ast.total_nodes;
        summary.peak_depth = summary.peak_depth.max(report.ast.max_depth);
        summary.complexity += report.ast.cognitive_complexity;
        summary.syntax_errors += report.ast.syntax_errors;
    }
    summaries
}

/// Renders scan results as a JSON document.
pub fn json_output(
    reports: &[FileReport],
    summaries: &BTreeMap<String, LangSummary>,
    complexity_threshold: Option<usize>,
) -> Result<String, serde_json::Error> {
    let violations: Vec<_> = reports
        .iter()
        .flat_map(|file| {
            file.functions
                .iter()
                .filter(move |function| {
                    complexity_threshold.is_some_and(|threshold| function.complexity > threshold)
                })
                .map(move |function| {
                    serde_json::json!({
                        "path": file.path,
                        "name": function.name,
                        "line": function.line,
                        "complexity": function.complexity,
                    })
                })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "files": reports,
        "languages": summaries,
        "complexity_threshold": complexity_threshold,
        "complexity_violations": violations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(source: &str, language: SupportedLanguage) -> AstStats {
        let mut parser = Parser::new();
        parser
            .set_language(&language.tree_sitter_language())
            .expect("supported grammar should load");
        let tree = parser
            .parse(source, None)
            .expect("parser should return a tree");
        analyze_ast(&tree, language)
    }

    #[test]
    fn detects_supported_extensions_case_insensitively() {
        assert_eq!(
            SupportedLanguage::from_extension("RS"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(
            SupportedLanguage::from_extension("pyi"),
            Some(SupportedLanguage::Python)
        );
        assert_eq!(
            SupportedLanguage::from_extension("tsx"),
            Some(SupportedLanguage::Tsx)
        );
        assert_eq!(
            SupportedLanguage::from_extension("jsx"),
            Some(SupportedLanguage::Jsx)
        );
        assert_eq!(SupportedLanguage::from_extension("unknown"), None);
    }

    #[test]
    fn counts_blank_code_line_and_comment_categories() {
        let source =
            "// leading comment\n\nfn f() {} // trailing comment\n/* block\n * continuation\n */\n";
        let stats = calculate_line_metrics(source, SupportedLanguage::Rust);
        assert_eq!(stats.comments, 4);
        assert_eq!(stats.blanks, 1);
        assert_eq!(stats.code, 1);
    }

    #[test]
    fn counts_python_hash_comments() {
        let stats = calculate_line_metrics("# comment\nx = 1\n", SupportedLanguage::Python);
        assert_eq!(stats.comments, 1);
        assert_eq!(stats.code, 1);
        assert_eq!(stats.blanks, 0);
    }

    #[test]
    fn all_supported_grammars_parse_representative_sources() {
        let examples = [
            (SupportedLanguage::Rust, "fn main() { let value = 1; }"),
            (SupportedLanguage::Python, "def main():\n    value = 1\n"),
            (SupportedLanguage::TypeScript, "const value: number = 1;"),
            (SupportedLanguage::Tsx, "const view = <div />;"),
            (SupportedLanguage::JavaScript, "const value = 1;"),
            (SupportedLanguage::Jsx, "const view = <div />;"),
        ];

        for (language, source) in examples {
            let stats = analyze(source, language);
            assert!(
                stats.total_nodes > 0,
                "{} should produce named nodes",
                language.display_name()
            );
            assert_eq!(
                stats.syntax_errors,
                0,
                "{} example should parse cleanly",
                language.display_name()
            );
        }
    }

    #[test]
    fn cognitive_complexity_counts_nesting_branches_and_flat_jumps() {
        let examples = [
            (
                SupportedLanguage::Rust,
                "fn f(x: bool) { if x { return; } }",
            ),
            (
                SupportedLanguage::Python,
                "def f(x):\n    if x:\n        return\n",
            ),
            (
                SupportedLanguage::TypeScript,
                "function f(x: boolean) { if (x) return; }",
            ),
            (
                SupportedLanguage::JavaScript,
                "function f(x) { if (x) return; }",
            ),
        ];

        for (language, source) in examples {
            let stats = analyze(source, language);
            assert_eq!(
                stats.syntax_errors,
                0,
                "{} example should parse cleanly",
                language.display_name()
            );
            assert_eq!(
                stats.cognitive_complexity,
                3,
                "{} branch should be nested in its function and include return",
                language.display_name()
            );
        }
    }

    #[test]
    fn reports_function_complexity_without_charging_nested_functions_to_parent() {
        let source =
            "function outer(x) { if (x) { function inner(y) { if (y) return; } return; } }";
        let mut parser = Parser::new();
        parser
            .set_language(&SupportedLanguage::JavaScript.tree_sitter_language())
            .expect("JavaScript grammar should load");
        let tree = parser
            .parse(source, None)
            .expect("parser should return a tree");
        let functions = analyze_functions(&tree, SupportedLanguage::JavaScript, source);

        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].name, "outer");
        assert_eq!(functions[0].line, 1);
        assert_eq!(functions[0].complexity, 2);
        assert_eq!(functions[1].name, "inner");
        assert_eq!(functions[1].complexity, 2);
    }

    #[test]
    fn reports_recoverable_syntax_errors() {
        let stats = analyze("fn broken( {", SupportedLanguage::Rust);
        assert!(stats.syntax_errors > 0);
    }
}
