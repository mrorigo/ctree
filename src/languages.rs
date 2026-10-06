// Rust guideline compliant 2026-10-06
//! Language definitions and grammar-specific syntax rules.

use serde::Serialize;
use tree_sitter::{Language, Node};

/// A source language with an available Tree-sitter grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportedLanguage {
    /// Rust source code.
    Rust,
    /// Python source code.
    Python,
    /// TypeScript source code.
    TypeScript,
    /// TSX source code.
    Tsx,
    /// JavaScript source code.
    JavaScript,
    /// JSX source code.
    Jsx,
}

impl SupportedLanguage {
    /// Detects a supported language from a file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "mjs" | "cjs" => Some(Self::JavaScript),
            "jsx" => Some(Self::Jsx),
            _ => None,
        }
    }

    /// Returns the grammar used to parse this language.
    pub fn tree_sitter_language(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust_orchard::LANGUAGE.into(),
            Self::Python => tree_sitter_python_orchard::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript | Self::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        }
    }

    /// Returns the human-readable language name.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TSX",
            Self::JavaScript => "JavaScript",
            Self::Jsx => "JSX",
        }
    }

    /// Reports whether a syntax node begins a function-like scope.
    pub fn is_function_node(self, node: Node<'_>) -> bool {
        match self {
            Self::Rust => matches!(node.kind(), "function_item" | "closure_expression"),
            Self::Python => matches!(
                node.kind(),
                "function_definition" | "async_function_definition" | "lambda"
            ),
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => matches!(
                node.kind(),
                "function_declaration"
                    | "function_expression"
                    | "arrow_function"
                    | "method_definition"
            ),
        }
    }

    /// Reports whether this node adds a level to cognitive nesting.
    pub fn is_nesting_node(self, node: Node<'_>) -> bool {
        let kind = node.kind();
        match self {
            Self::Rust => matches!(
                kind,
                "function_item"
                    | "closure_expression"
                    | "if_expression"
                    | "for_expression"
                    | "while_expression"
                    | "loop_expression"
                    | "match_expression"
            ),
            Self::Python => matches!(
                kind,
                "function_definition"
                    | "lambda"
                    | "if_statement"
                    | "for_statement"
                    | "while_statement"
                    | "match_statement"
            ),
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => matches!(
                kind,
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
        }
    }

    /// Reports whether this node contributes a branching complexity point.
    pub fn is_branching_node(self, node: Node<'_>) -> bool {
        let kind = node.kind();
        let direct = match self {
            Self::Rust => matches!(
                kind,
                "if_expression"
                    | "for_expression"
                    | "while_expression"
                    | "loop_expression"
                    | "match_arm"
            ),
            Self::Python => matches!(
                kind,
                "if_statement"
                    | "for_statement"
                    | "while_statement"
                    | "case_clause"
                    | "conditional_expression"
            ),
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => matches!(
                kind,
                "if_statement"
                    | "for_statement"
                    | "for_in_statement"
                    | "while_statement"
                    | "do_statement"
                    | "switch_case"
                    | "ternary_expression"
            ),
        };
        direct || self.is_logical_branch(node)
    }

    /// Reports whether this node is a non-nesting control-flow jump.
    pub fn is_flat_jump_node(self, node: Node<'_>) -> bool {
        match self {
            Self::Rust => matches!(
                node.kind(),
                "break_expression" | "continue_expression" | "return_expression"
            ),
            Self::Python => matches!(
                node.kind(),
                "break_statement" | "continue_statement" | "return_statement" | "raise_statement"
            ),
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => matches!(
                node.kind(),
                "break_statement" | "continue_statement" | "return_statement" | "throw_statement"
            ),
        }
    }

    fn is_logical_branch(self, node: Node<'_>) -> bool {
        if !matches!(node.kind(), "binary_expression" | "boolean_operator") {
            return false;
        }
        let operators: &[&str] = match self {
            Self::Python => &["and", "or"],
            Self::TypeScript | Self::Tsx | Self::JavaScript | Self::Jsx => &["&&", "||", "??"],
            Self::Rust => &["&&", "||"],
        };
        operators.iter().any(|operator| {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|child| child.kind() == *operator)
        })
    }
}
