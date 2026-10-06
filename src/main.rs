// Rust guideline compliant 2026-10-06
//! Command-line interface for ctree.

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use ctree::{ScanOptions, json_output, scan, summarize};
use std::path::PathBuf;
use std::time::Instant;

/// A syntax-aware source code metrics command-line tool.
#[derive(Debug, Parser)]
#[command(
    version,
    about,
    long_about = "Count source lines and analyze syntax tree structure and cognitive complexity."
)]
struct Cli {
    /// Paths to scan.
    #[arg(value_name = "PATH", default_value = ".")]
    paths: Vec<PathBuf>,
    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Table)]
    format: Format,
    /// Include a row for each source file.
    #[arg(long)]
    files: bool,
    /// Include complexity metrics for each function-like scope.
    #[arg(long, conflicts_with = "no_ast")]
    functions: bool,
    /// Fail with status 1 if any function complexity is greater than this value.
    #[arg(long, value_name = "N", conflicts_with = "no_ast")]
    complexity_threshold: Option<usize>,
    /// Include hidden files and directories.
    #[arg(long)]
    hidden: bool,
    /// Scan files ignored by gitignore.
    #[arg(long)]
    no_ignore: bool,
    /// Count lines without parsing syntax trees.
    #[arg(long)]
    no_ast: bool,
    /// Comma-separated language names to include.
    #[arg(long, value_delimiter = ',')]
    language: Vec<String>,
    /// Sort language rows by a metric.
    #[arg(long, value_enum, default_value_t = SortBy::Code)]
    sort: SortBy,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Table,
    Json,
    Csv,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SortBy {
    Code,
    Files,
    Complexity,
    Nodes,
    Name,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let started = Instant::now();
    let options = ScanOptions {
        paths: cli.paths,
        hidden: cli.hidden,
        no_ignore: cli.no_ignore,
        files: cli.files,
        no_ast: cli.no_ast,
        languages: cli.language,
    };
    let (reports, errors) = scan(&options);
    let summaries = summarize(&reports);
    match cli.format {
        Format::Json => println!(
            "{}",
            json_output(&reports, &summaries, cli.complexity_threshold)
                .context("could not serialize JSON output")?
        ),
        Format::Csv => render_csv(&reports, &summaries, cli.complexity_threshold),
        Format::Table => render_table(
            &reports,
            &summaries,
            cli.files,
            cli.functions,
            cli.complexity_threshold,
            cli.sort,
            started.elapsed(),
        ),
    }
    for error in &errors {
        eprintln!("ctree: {error}");
    }
    if cli.complexity_threshold.is_some_and(|threshold| {
        reports
            .iter()
            .flat_map(|file| &file.functions)
            .any(|function| function.complexity > threshold)
    }) {
        std::process::exit(1);
    }
    if reports.is_empty() && !errors.is_empty() {
        std::process::exit(2);
    }
    Ok(())
}

fn render_table(
    reports: &[ctree::FileReport],
    summaries: &std::collections::BTreeMap<String, ctree::LangSummary>,
    files: bool,
    functions: bool,
    complexity_threshold: Option<usize>,
    sort: SortBy,
    elapsed: std::time::Duration,
) {
    println!(
        "{:<16} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6}",
        "LANGUAGE", "FILES", "CODE", "COMMENTS", "BLANKS", "NODES", "COMPLEX", "ERRORS"
    );
    println!("{}", "-".repeat(78));
    let mut entries: Vec<_> = summaries.iter().collect();
    entries.sort_by(|(name_a, a), (name_b, b)| match sort {
        SortBy::Code => b.code.cmp(&a.code).then_with(|| name_a.cmp(name_b)),
        SortBy::Files => b.files.cmp(&a.files).then_with(|| name_a.cmp(name_b)),
        SortBy::Complexity => b
            .complexity
            .cmp(&a.complexity)
            .then_with(|| name_a.cmp(name_b)),
        SortBy::Nodes => b.nodes.cmp(&a.nodes).then_with(|| name_a.cmp(name_b)),
        SortBy::Name => name_a.cmp(name_b),
    });
    for (name, summary) in &entries {
        print_summary(name, summary);
    }
    let total = entries
        .iter()
        .fold(ctree::LangSummary::default(), |mut total, (_, s)| {
            total.files += s.files;
            total.code += s.code;
            total.comments += s.comments;
            total.blanks += s.blanks;
            total.nodes += s.nodes;
            total.peak_depth = total.peak_depth.max(s.peak_depth);
            total.complexity += s.complexity;
            total.syntax_errors += s.syntax_errors;
            total
        });
    println!("{}", "=".repeat(78));
    print_summary(&format!("TOTAL ({elapsed:.2?})"), &total);
    if files {
        println!("\nFILES");
        for file in reports {
            println!(
                "{}\t{} code, {} comments, {} blanks, {} nodes, depth {}, complexity {}, {} syntax errors",
                file.path.display(),
                file.lines.code,
                file.lines.comments,
                file.lines.blanks,
                file.ast.total_nodes,
                file.ast.max_depth,
                file.ast.cognitive_complexity,
                file.ast.syntax_errors
            );
        }
    }
    if functions || complexity_threshold.is_some() {
        let title = complexity_threshold
            .map(|limit| format!("FUNCTIONS (complexity > {limit})"))
            .unwrap_or_else(|| "FUNCTIONS".to_owned());
        println!("\n{title}");
        for file in reports {
            for function in &file.functions {
                if complexity_threshold.is_some_and(|limit| function.complexity <= limit) {
                    continue;
                }
                println!(
                    "{}:{}\t{}\tcomplexity {}",
                    file.path.display(),
                    function.line,
                    function.name,
                    function.complexity
                );
            }
        }
    }
}

fn print_summary(name: &str, s: &ctree::LangSummary) {
    println!(
        "{name:<16} {:>6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>6}",
        s.files, s.code, s.comments, s.blanks, s.nodes, s.complexity, s.syntax_errors
    );
}

fn render_csv(
    reports: &[ctree::FileReport],
    summaries: &std::collections::BTreeMap<String, ctree::LangSummary>,
    complexity_threshold: Option<usize>,
) {
    println!(
        "record,language,path,files,code,comments,blanks,nodes,depth,complexity,syntax_errors,function,line"
    );
    for (language, s) in summaries {
        println!(
            "summary,{language},,{},{},{},{},{},{},{},{},,",
            s.files,
            s.code,
            s.comments,
            s.blanks,
            s.nodes,
            s.peak_depth,
            s.complexity,
            s.syntax_errors
        );
    }
    for file in reports {
        let path = file.path.to_string_lossy().replace('"', "\"\"");
        println!(
            "file,{},\"{}\",1,{},{},{},{},{},{},{},,",
            file.lang.display_name(),
            path,
            file.lines.code,
            file.lines.comments,
            file.lines.blanks,
            file.ast.total_nodes,
            file.ast.max_depth,
            file.ast.cognitive_complexity,
            file.ast.syntax_errors
        );
        for function in &file.functions {
            if complexity_threshold.is_some_and(|limit| function.complexity <= limit) {
                continue;
            }
            println!(
                "function,{},\"{}\",,,,,,,{},\"{}\",{}",
                file.lang.display_name(),
                path,
                function.complexity,
                function.name.replace('"', "\"\""),
                function.line
            );
        }
    }
}
