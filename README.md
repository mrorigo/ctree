# ctree

`ctree` is a syntax-aware source metrics CLI. It reports code, comment-only, and blank lines alongside Tree-sitter node counts, maximum syntax tree depth, syntax errors, and a cognitive complexity estimate.

## Supported languages

| Language | Extensions |
| --- | --- |
| Rust | `.rs` |
| Python | `.py`, `.pyi` |
| TypeScript | `.ts`, `.mts`, `.cts` |
| TSX | `.tsx` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| JSX | `.jsx` |

Tree-sitter 0.27 grammars parse files independently on Rayon workers. Rust and Python use Orchard grammars; TypeScript and JavaScript use the tree-sitter organization grammars. Scans skip hidden files, gitignored paths, and common ignored build output by default. Symlinks are not followed by the walker.

## Usage

```text
ctree [OPTIONS] [PATH]...
```

With no path, ctree scans the current directory. Multiple files and directories can be provided.

```sh
ctree .
ctree src Cargo.toml
ctree --files --sort complexity src
ctree --functions src
ctree --complexity-threshold 15 src
ctree --format markdown --files --functions src
ctree --format json --language rust,python .
ctree --format csv --no-ignore .
ctree --no-ast .
```

`--no-ast` skips parsing, so node, depth, complexity, and syntax error fields are zero. `--hidden` includes hidden directory entries. `--no-ignore` disables gitignore filtering. Output formats are `table`, `json`, `csv`, and `markdown` (`md` is an alias).

`--functions` lists function-level cognitive complexity. `--complexity-threshold N` lists only functions whose score is greater than `N` and exits with status 1 if any exceed it, making it suitable for CI. JSON includes per-function metrics and a `complexity_violations` array; CSV includes function rows and filters them to violations when a threshold is set. Nested functions are scored independently and excluded from their parent's score.

## Metric definitions

- **Code lines:** Nonblank lines that are not classified as comment-only. A line containing both code and a trailing comment counts as code.
- **Comment lines:** Lines beginning with a supported line comment marker, or lines inside a block comment. Python `#` comments and C-style `//` and `/* ... */` comments are recognized. Comment delimiters inside strings may affect line-only counts.
- **Blank lines:** Empty or whitespace-only lines. A final newline does not add an extra line.
- **Nodes:** Named Tree-sitter syntax nodes.
- **Depth:** Maximum syntax tree depth with the root at depth zero.
- **Cognitive complexity:** Branching constructs add one plus the number of enclosing function/control-flow nesting constructs. Return, break, continue, raise, and throw nodes add one. This is a useful comparative estimate, not a formal language-independent standard.
- **Syntax errors:** Tree-sitter error and missing nodes. Tree-sitter recovers from many invalid fragments, so a report is still produced for files with syntax errors.

Read errors are printed to stderr while successfully analyzed files remain in the output. If every discovered file failed to read or parse, ctree exits with status 2.

## Development

```sh
cargo fmt
cargo clippy -- -D warnings
cargo test
```
