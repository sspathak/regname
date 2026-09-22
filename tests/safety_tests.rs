#[path = "../src/main.rs"]
#[allow(dead_code, unused_imports)]
mod main_app;

use main_app::{
    calculate_rename_counts, check_preflight_safety, escape_history_field, load_history_from_path,
    parse_cli_args, save_history_to_path, unescape_history_field, SafetyViolation,
};
use regex::Regex;
use std::env;

#[test]
fn test_parse_cli_args_defaults() {
    let args: Vec<&str> = vec![];
    let parsed = parse_cli_args(args).expect("failed to parse empty args");
    assert!(parsed.confirm, "confirm should be enabled by default");
    assert!(parsed.prevent_delete, "prevent_delete should be enabled by default");
    assert!(!parsed.help);
    assert_eq!(
        parsed.dir,
        env::current_dir().unwrap().canonicalize().unwrap()
    );
}

#[test]
fn test_parse_cli_args_no_confirm_flags() {
    for flag in &["--no-confirm", "-y", "--yes"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse no-confirm flag");
        assert!(!parsed.confirm, "flag {} should disable confirm", flag);
        assert!(parsed.prevent_delete, "flag {} should leave prevent_delete enabled", flag);
    }
}

#[test]
fn test_parse_cli_args_force_overwrite_flags() {
    for flag in &["--overwrite", "--force", "-f"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse force flag");
        assert!(!parsed.prevent_delete, "flag {} should disable prevent_delete", flag);
        assert!(parsed.confirm, "flag {} should leave confirm enabled", flag);
    }
}

#[test]
fn test_parse_cli_args_unsafe_flag() {
    let parsed = parse_cli_args(vec!["--unsafe"]).expect("failed to parse --unsafe");
    assert!(!parsed.confirm, "--unsafe should disable confirm");
    assert!(!parsed.prevent_delete, "--unsafe should disable prevent_delete");
}

#[test]
fn test_parse_cli_args_explicit_safe_flags() {
    for flag in &["-s", "--safe", "-c", "--confirm", "-p", "--prevent-delete"] {
        let parsed = parse_cli_args(vec![*flag]).expect("failed to parse safe flag");
        assert!(parsed.confirm);
        assert!(parsed.prevent_delete);
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

#[test]
fn test_preflight_safety_normal() {
    let items = vec![
        (true, "file1.txt".to_string()),
        (true, "file2.txt".to_string()),
    ];
    let re = Regex::new(r"file(\d)\.txt").unwrap();
    let pattern = "doc$1.txt";
    assert_eq!(check_preflight_safety(&items, &re, pattern), Ok(()));
}

#[test]
fn test_preflight_safety_chain_collision_blocked() {
    let items = vec![
        (true, "test1.txt".to_string()),
        (true, "test2.txt".to_string()),
    ];
    let re = Regex::new(r"test1\.txt").unwrap();
    let pattern = "test2.txt";
    let res = check_preflight_safety(&items, &re, pattern);
    assert!(matches!(res, Err(SafetyViolation::CountDecrease { .. } | SafetyViolation::ChainCollision { .. })));
}

#[test]
fn test_preflight_safety_swap_collision_blocked() {
    let items = vec![
        (true, "x.txt".to_string()),
        (true, "y.txt".to_string()),
    ];
    let re = Regex::new(r"^x\.txt$").unwrap();
    let pattern = "y.txt";
    assert!(check_preflight_safety(&items, &re, pattern).is_err());
}

#[test]
fn test_preflight_safety_no_op_allowed() {
    let items = vec![
        (true, "x.txt".to_string()),
        (true, "y.txt".to_string()),
    ];
    let re = Regex::new(r".*").unwrap();
    let pattern = "$0";
    assert_eq!(check_preflight_safety(&items, &re, pattern), Ok(()));
}

#[test]
fn test_history_escape_unescape_roundtrip() {
    let cases = vec![
        "normal text",
        "regex with (.*) and $1",
        "tab\tseparated\tvalues",
        "newline\nand\r\nreturns",
        r"regex\d+\s+\w+",
        r"c:\path\to\file",
        r"${1}_${2}.bak",
    ];

    for case in cases {
        let escaped = escape_history_field(case);
        assert!(!escaped.contains('\t'), "escaped text must not contain raw tabs");
        assert!(!escaped.contains('\n'), "escaped text must not contain raw newlines");
        let unescaped = unescape_history_field(&escaped);
        assert_eq!(unescaped, case);
    }
}

#[test]
fn test_history_save_load_order_and_deduplication() {
    let temp_dir = env::temp_dir().join(format!(
        "regname_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);
    let hist_file = temp_dir.join("history");

    // Initially empty
    let list = load_history_from_path(&hist_file);
    assert!(list.is_empty());

    // Save first entry
    save_history_to_path(&hist_file, r"(.*)\.txt", r"${1}.md");
    let list = load_history_from_path(&hist_file);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], (r"(.*)\.txt".to_string(), r"${1}.md".to_string()));

    // Save second entry
    save_history_to_path(&hist_file, r"^img_(\d+)", r"photo_${1}");
    let list = load_history_from_path(&hist_file);
    assert_eq!(list.len(), 2);
    // Most recent is at index 0
    assert_eq!(list[0], (r"^img_(\d+)".to_string(), r"photo_${1}".to_string()));
    assert_eq!(list[1], (r"(.*)\.txt".to_string(), r"${1}.md".to_string()));

    // Save first entry again -> moves to index 0, no duplicates!
    save_history_to_path(&hist_file, r"(.*)\.txt", r"${1}.md");
    let list = load_history_from_path(&hist_file);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], (r"(.*)\.txt".to_string(), r"${1}.md".to_string()));
    assert_eq!(list[1], (r"^img_(\d+)".to_string(), r"photo_${1}".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_history_max_capacity_truncation() {
    let temp_dir = env::temp_dir().join(format!(
        "regname_cap_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);
    let hist_file = temp_dir.join("history");

    for i in 0..120 {
        save_history_to_path(&hist_file, &format!("match_{}", i), &format!("replace_{}", i));
    }

    let list = load_history_from_path(&hist_file);
    assert_eq!(list.len(), 100, "history should be capped at 100 entries");
    assert_eq!(list[0], ("match_119".to_string(), "replace_119".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
