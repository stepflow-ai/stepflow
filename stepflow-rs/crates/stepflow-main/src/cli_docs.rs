// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

//! CLI documentation generation using clap-markdown.
//!
//! This module provides functionality to generate markdown documentation
//! from the clap CLI definition, similar to how schemas are generated
//! and validated in the codebase.

use std::collections::HashMap;
use std::path::Path;

use crate::cli::Cli;

/// Ordered list of commands with their sidebar positions
/// This ensures consistent ordering and fails if new commands are added without explicit positioning
const COMMAND_ORDER: &[(&str, u32)] = &[
    ("run", 2),
    ("serve", 3),
    ("submit", 4),
    ("test", 5),
    ("list-components", 6),
    ("repl", 7),
    ("validate", 8),
];

/// Generate custom CLI index documentation with command list and links
pub fn generate_cli_docs() -> String {
    use clap::CommandFactory as _;

    let frontmatter = "---\nsidebar_position: 1\n---\n\n";

    let mut content = String::new();
    content.push_str("# CLI Reference\n\n");
    content.push_str("Stepflow provides several commands for executing workflows in different ways. This reference covers all available commands and their options.\n\n");
    content.push_str("## Commands\n\n");

    let app = Cli::command();
    let command_positions: HashMap<&str, u32> = COMMAND_ORDER.iter().cloned().collect();

    // Create a sorted list of commands by their sidebar position
    let mut commands_with_positions: Vec<(&str, u32, &clap::Command)> = Vec::new();

    for subcommand in app.get_subcommands() {
        let name = subcommand.get_name();

        // Skip the built-in "help" command
        if name == "help" {
            continue;
        }

        if let Some(&position) = command_positions.get(name) {
            commands_with_positions.push((name, position, subcommand));
        }
    }

    // Sort by position
    commands_with_positions.sort_by_key(|(_, position, _)| *position);

    // Generate the command list
    for (name, _, subcommand) in commands_with_positions {
        let about = subcommand
            .get_about()
            .map(|s| s.to_string())
            .unwrap_or_default();
        content.push_str(&format!("- **[`{name}`](./{name}.md)** - {about}\n"));
    }

    content.push_str("\n## Global Options\n\n");
    content.push_str("All commands support these global options:\n\n");

    // Extract global options from the main CLI
    for arg in app.get_arguments() {
        // Check if this is a global argument by looking at the global field
        if arg.is_global_set() {
            let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();

            // Format the argument name
            let arg_display = if let Some(long) = arg.get_long() {
                format!("--{long}")
            } else if let Some(short) = arg.get_short() {
                format!("-{short}")
            } else {
                arg.get_id().as_str().to_string()
            };

            // Add value name if present
            let value_name = arg
                .get_value_names()
                .and_then(|names| names.first())
                .map(|name| format!(" <{}>", name.as_str()))
                .unwrap_or_default();

            content.push_str(&format!("- `{arg_display}{value_name}` - {help}\n"));
        }
    }

    format!("{frontmatter}{content}")
}

/// Generate documentation for a specific subcommand with frontmatter
pub fn generate_subcommand_docs() -> HashMap<String, String> {
    use clap::CommandFactory as _;

    let mut docs = HashMap::new();
    let app = Cli::command();
    let command_positions: HashMap<&str, u32> = COMMAND_ORDER.iter().cloned().collect();
    let options = clap_markdown::MarkdownOptions::new().show_footer(false);

    // Generate docs for each subcommand
    for subcommand in app.get_subcommands() {
        let name = subcommand.get_name().to_string();

        // Skip the built-in "help" command
        if name == "help" {
            continue;
        }

        let position = command_positions.get(name.as_str())
            .unwrap_or_else(|| panic!("Command '{name}' not found in COMMAND_ORDER. Please add it with an appropriate sidebar_position."));

        let frontmatter = format!("---\nsidebar_position: {position}\n---\n\n");
        let content = clap_markdown::help_markdown_command_custom(subcommand, &options);
        let full_content = format!("{frontmatter}{content}");

        docs.insert(name, full_content);
    }

    docs
}

/// Get the expected list of commands in order
pub fn get_expected_commands() -> Vec<String> {
    COMMAND_ORDER
        .iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Write CLI documentation to files
pub fn write_cli_docs_to_files(base_path: &Path) -> std::io::Result<()> {
    use std::fs;

    // Ensure the directory exists
    fs::create_dir_all(base_path)?;

    // Write main CLI documentation as index.md
    let main_docs = generate_cli_docs();
    fs::write(base_path.join("index.md"), main_docs)?;

    // Write subcommand documentation (without cli- prefix)
    let subcommand_docs = generate_subcommand_docs();
    for (command_name, docs) in subcommand_docs {
        let filename = format!("{command_name}.md");
        fs::write(base_path.join(filename), docs)?;
    }

    Ok(())
}

/// Read existing CLI documentation from files
pub fn read_existing_cli_docs(base_path: &Path) -> std::io::Result<HashMap<String, String>> {
    use std::fs;

    let mut docs = HashMap::new();

    // Read main CLI documentation from index.md
    if let Ok(content) = fs::read_to_string(base_path.join("index.md")) {
        docs.insert("cli".to_string(), content);
    }

    // Read subcommand documentation (without cli- prefix)
    for (command_name, _) in COMMAND_ORDER {
        let filename = format!("{command_name}.md");
        if let Ok(content) = fs::read_to_string(base_path.join(&filename)) {
            docs.insert(command_name.to_string(), content);
        }
    }

    Ok(docs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Test that verifies generated CLI documentation matches checked-in files,
    /// similar to the schema comparison tests in the codebase.
    #[test]
    fn test_cli_docs_comparison() {
        let docs_path = std::path::Path::new("../../../docs/docs/cli");
        let should_overwrite = env::var("STEPFLOW_OVERWRITE_CLI_DOCS").is_ok();

        // Generate current documentation
        let main_docs = generate_cli_docs();
        let subcommand_docs = generate_subcommand_docs();

        if should_overwrite {
            // Write generated documentation to files
            write_cli_docs_to_files(docs_path).expect("Failed to write CLI docs");
            return;
        }

        // Read existing documentation
        let existing_docs =
            read_existing_cli_docs(docs_path).expect("Failed to read existing CLI docs");

        // Compare main CLI documentation
        if let Some(existing_main) = existing_docs.get("cli") {
            if existing_main.trim() != main_docs.trim() {
                panic!(
                    "Main CLI documentation does not match generated version.\n\
                     Run with STEPFLOW_OVERWRITE_CLI_DOCS=1 to update.\n\
                     \n\
                     Expected:\n{}\n\
                     \n\
                     Generated:\n{}",
                    existing_main.trim(),
                    main_docs.trim()
                );
            }
        } else if should_overwrite {
            // Write the main docs file if it doesn't exist and we're overwriting
            std::fs::write(docs_path.join("cli.md"), &main_docs)
                .expect("Failed to write main CLI docs");
        }

        // Compare subcommand documentation
        for (command_name, generated_docs) in subcommand_docs {
            if let Some(existing_docs) = existing_docs.get(&command_name) {
                if existing_docs.trim() != generated_docs.trim() {
                    panic!(
                        "CLI documentation for '{}' does not match generated version.\n\
                         Run with STEPFLOW_OVERWRITE_CLI_DOCS=1 to update.\n\
                         \n\
                         Expected:\n{}\n\
                         \n\
                         Generated:\n{}",
                        command_name,
                        existing_docs.trim(),
                        generated_docs.trim()
                    );
                }
            }
        }
    }

    #[test]
    fn test_generate_main_cli_docs() {
        let docs = generate_cli_docs();
        assert!(docs.contains("# CLI Reference"));
        assert!(docs.contains("## Commands"));
        assert!(docs.contains("## Global Options"));

        // Verify it contains links to command pages
        assert!(docs.contains("[`run`](./run.md)"));
        assert!(docs.contains("[`serve`](./serve.md)"));
        assert!(docs.contains("[`validate`](./validate.md)"));

        // Verify it doesn't contain detailed command usage (that's for individual pages)
        assert!(!docs.contains("Usage: stepflow-main run"));
        assert!(!docs.contains("**Command Overview:**"));
    }

    #[test]
    fn test_generate_subcommand_docs() {
        let docs = generate_subcommand_docs();
        let expected_commands = get_expected_commands();

        // Should have documentation for all expected commands
        for command in &expected_commands {
            assert!(
                docs.contains_key(command),
                "Missing documentation for command '{command}'"
            );
        }

        // Each should contain usage information and frontmatter
        for (command, doc) in &docs {
            assert!(
                doc.contains("---\nsidebar_position:"),
                "Command '{command}' missing frontmatter"
            );
            assert!(doc.contains("Usage:"), "Command '{command}' missing usage");
        }

        // Verify we have exactly the expected commands (no more, no less)
        let mut actual_commands: Vec<String> = docs.keys().cloned().collect();
        actual_commands.sort();
        let mut expected_sorted = expected_commands.clone();
        expected_sorted.sort();

        assert_eq!(
            actual_commands, expected_sorted,
            "Generated commands don't match expected commands. If you added a new command, please add it to COMMAND_ORDER."
        );
    }

    #[test]
    fn test_command_order_validation() {
        use clap::CommandFactory as _;

        let app = Cli::command();
        let mut actual_commands: Vec<String> = app
            .get_subcommands()
            .filter(|cmd| cmd.get_name() != "help") // Skip built-in help command
            .map(|cmd| cmd.get_name().to_string())
            .collect();
        actual_commands.sort();

        let mut expected_commands = get_expected_commands();
        expected_commands.sort();

        assert_eq!(
            actual_commands, expected_commands,
            "CLI commands don't match COMMAND_ORDER. Please update COMMAND_ORDER to include all commands with appropriate sidebar positions."
        );
    }

    #[test]
    fn test_frontmatter_generation() {
        let main_docs = generate_cli_docs();
        assert!(main_docs.starts_with("---\nsidebar_position: 1\n---\n\n"));

        let subcommand_docs = generate_subcommand_docs();
        for (command, doc) in subcommand_docs {
            assert!(
                doc.starts_with("---\nsidebar_position:"),
                "Command '{command}' doesn't start with frontmatter"
            );
        }
    }
}
