#[path = "../src/main.rs"]
#[allow(dead_code, unused_imports)]
mod main_app;

use main_app::{calculate_rename_counts, parse_cli_args};
use regex::Regex;
use std::env;

#[test]
fn test_parse_cli_args_defaults() {
    let args: Vec<&str> = vec![];
    let parsed = parse_cli_args(args).expect("failed to parse empty args");
    assert!(!parsed.confirm);
    assert!(!parsed.prevent_delete);
    assert!(!parsed.help);
    assert_eq!(
        parsed.dir,
        env::current_dir().unwrap().canonicalize().unwrap()
    );
}

#[test]
fn test_parse_cli_args_confirm_flags() {
    for flag in &["-c", "--confirm", "-i", "--interactive"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse confirm flag");
        assert!(parsed.confirm, "flag {} should enable confirm", flag);
        assert!(
            !parsed.prevent_delete,
            "flag {} should not enable prevent_delete",
            flag
        );
    }
}

#[test]
fn test_parse_cli_args_prevent_delete_flags() {
    for flag in &["-p", "--prevent-delete", "-n", "--no-overwrite", "--prevent-overwrite"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse prevent-delete flag");
        assert!(
            parsed.prevent_delete,
            "flag {} should enable prevent_delete",
            flag
        );
        assert!(!parsed.confirm, "flag {} should not enable confirm", flag);
    }
}

#[test]
fn test_parse_cli_args_safe_flag_enables_both() {
    for flag in &["-s", "--safe"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse safe flag");
        assert!(parsed.confirm, "flag {} should enable confirm", flag);
        assert!(
            parsed.prevent_delete,
            "flag {} should enable prevent_delete",
            flag
        );
    }
}

#[test]
fn test_parse_cli_args_custom_dir_and_flags() {
    let temp_dir = env::temp_dir();
    let temp_path = temp_dir.canonicalize().unwrap();
    let temp_str = temp_path.to_str().unwrap();

    // Flag before dir
    let parsed = parse_cli_args(vec!["--confirm", temp_str]).expect("failed to parse");
    assert!(parsed.confirm);
    assert_eq!(parsed.dir, temp_path);

    // Dir before flag
    let parsed2 = parse_cli_args(vec![temp_str, "--prevent-delete"]).expect("failed to parse");
    assert!(parsed2.prevent_delete);
    assert_eq!(parsed2.dir, temp_path);
}

#[test]
fn test_parse_cli_args_unknown_flag() {
    let result = parse_cli_args(vec!["--unknown-flag"]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown option: --unknown-flag"));
}

#[test]
fn test_parse_cli_args_too_many_positional_args() {
    let result = parse_cli_args(vec!["dir1", "dir2"]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unexpected argument: dir2"));
}

#[test]
fn test_parse_cli_args_help() {
    let parsed = parse_cli_args(vec!["-h"]).expect("failed to parse help");
    assert!(parsed.help);
    let parsed = parse_cli_args(vec!["--help"]).expect("failed to parse help");
    assert!(parsed.help);
}

#[test]
fn test_calculate_rename_counts_normal_rename() {
    let items = vec![
        (true, "file1.txt".to_string()),
        (true, "file2.txt".to_string()),
        (true, "file3.txt".to_string()),
    ];
    let re = Regex::new(r"file(\d)\.txt").unwrap();
    let pattern = "doc$1.txt";
    let (before, after) = calculate_rename_counts(&items, &re, pattern);
    assert_eq!(before, 3);
    assert_eq!(after, 3);
}

#[test]
fn test_calculate_rename_counts_collision_causes_decrease() {
    let items = vec![
        (true, "file1.txt".to_string()),
        (true, "file2.txt".to_string()),
        (true, "file3.txt".to_string()),
    ];
    // Renaming all 3 files to the same static name
    let re = Regex::new(r"file\d\.txt").unwrap();
    let pattern = "same.txt";
    let (before, after) = calculate_rename_counts(&items, &re, pattern);
    assert_eq!(before, 3);
    assert_eq!(after, 1);
    assert!(
        after < before,
        "after count should be strictly less than before"
    );
}

#[test]
fn test_calculate_rename_counts_overwrite_unmatched_file() {
    let items = vec![
        (true, "a.txt".to_string()),
        (true, "b.txt".to_string()),
    ];
    // Renaming only a.txt to b.txt (which already exists and is not renamed)
    let re = Regex::new(r"^a\.txt$").unwrap();
    let pattern = "b.txt";
    let (before, after) = calculate_rename_counts(&items, &re, pattern);
    assert_eq!(before, 2);
    assert_eq!(after, 1);
    assert!(after < before);
}

#[test]
fn test_calculate_rename_counts_empty_pattern() {
    let items = vec![
        (true, "a.txt".to_string()),
        (true, "b.txt".to_string()),
    ];
    let re = Regex::new(r".*").unwrap();
    let pattern = "";
    let (before, after) = calculate_rename_counts(&items, &re, pattern);
    assert_eq!(before, 2);
    assert_eq!(after, 2);
}

#[test]
fn test_calculate_rename_counts_no_match() {
    let items = vec![
        (true, "a.txt".to_string()),
        (true, "b.txt".to_string()),
    ];
    let re = Regex::new(r"^xyz$").unwrap();
    let pattern = "something";
    let (before, after) = calculate_rename_counts(&items, &re, pattern);
    assert_eq!(before, 2);
    assert_eq!(after, 2);
}
