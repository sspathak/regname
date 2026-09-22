use std::path::{Path, PathBuf};
use iocraft::prelude::*;
use regex::Regex;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusedField {
    Match,
    Rename,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SafetyViolation {
    CountDecrease { before: usize, after: usize },
    ChainCollision { source: String, target: String },
}

#[derive(Clone, PartialEq, Eq)]
enum Modal {
    None,
    Confirm,
    Blocked(SafetyViolation),
    History { selected_idx: usize },
}

pub fn escape_history_field(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

pub fn unescape_history_field(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn get_history_path() -> Option<PathBuf> {
    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            return Some(PathBuf::from(data_home).join("regname").join("history"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home).join(".local").join("share").join("regname").join("history"));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.is_empty() {
            return Some(PathBuf::from(profile).join(".regname_history"));
        }
    }
    None
}

pub fn load_history_from_path(path: &Path) -> Vec<(String, String)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut list = Vec::new();
    for line in content.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some((m, r)) = line.split_once('\t') {
            list.push((unescape_history_field(m), unescape_history_field(r)));
        }
    }
    list
}

pub fn save_history_to_path(path: &Path, match_pattern: &str, rename_pattern: &str) {
    if match_pattern.is_empty() && rename_pattern.is_empty() {
        return;
    }
    let mut history = load_history_from_path(path);
    history.retain(|(m, r)| m != match_pattern || r != rename_pattern);
    history.insert(0, (match_pattern.to_string(), rename_pattern.to_string()));
    history.truncate(100);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut content = String::new();
    for (m, r) in history {
        content.push_str(&escape_history_field(&m));
        content.push('\t');
        content.push_str(&escape_history_field(&r));
        content.push('\n');
    }

    let _ = std::fs::write(path, content);
}

pub fn load_history() -> Vec<(String, String)> {
    if let Some(path) = get_history_path() {
        load_history_from_path(&path)
    } else {
        Vec::new()
    }
}

pub fn save_history(match_pattern: &str, rename_pattern: &str) {
    if let Some(path) = get_history_path() {
        save_history_to_path(&path, match_pattern, rename_pattern);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmChoice {
    Yes,
    No,
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut prev = 0;
    for (i, _) in s.char_indices() {
        if i >= idx {
            break;
        }
        prev = i;
    }
    prev
}

fn next_char_boundary(s: &str, idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    idx + s[idx..].chars().next().map_or(0, |c| c.len_utf8())
}

fn cursor_cell(value: &str, cursor: usize) -> String {
    let cursor = cursor.min(value.len());
    if cursor < value.len() {
        let next = next_char_boundary(value, cursor);
        value[cursor..next].to_string()
    } else {
        " ".to_string()
    }
}

fn cursor_column(value: &str, cursor: usize) -> i32 {
    let cursor = cursor.min(value.len());
    value[..cursor].chars().count() as i32
}

#[derive(Default, Props)]
struct AppProps {
    dir: std::path::PathBuf,
    exit_code: i32,
    out_buffer: String,
    err_buffer: String,
    confirm: bool,
    prevent_delete: bool,
}

#[allow(clippy::too_many_arguments)]
fn perform_rename(
    items: &[(bool, String)],
    match_re: &Regex,
    match_pattern: &str,
    pattern: &str,
    dir: &std::path::Path,
    oprintln: &mut dyn FnMut(&str),
    eprintln: &mut dyn FnMut(&str),
    should_exit: &mut State<Option<i32>>,
) {
    let mut any_renamed = false;
    for (_, filename) in items.iter() {
        if match_re.is_match(filename) && !pattern.is_empty() {
            let renamed = match_re.replace_all(filename, pattern).to_string();

            oprintln(&format!("{} -> {}", filename, renamed));

            let old_path = dir.join(filename);
            let new_path = dir.join(&renamed);

            match std::fs::rename(old_path, new_path) {
                Ok(_) => {
                    any_renamed = true;
                }
                Err(err) => {
                    eprintln(&format!("ERROR: Could not rename file: {}", err));
                    should_exit.set(Some(1));
                    return;
                }
            }
        }
    }

    if any_renamed {
        save_history(match_pattern, pattern);
    }

    should_exit.set(Some(0));
}

#[component]
fn App(props: &mut AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    let mut out_buffer = hooks.use_state(String::new);
    let mut err_buffer = hooks.use_state(String::new);
    let mut should_exit = hooks.use_state::<Option<i32>, _>(|| None);
    let mut scroll_offset = hooks.use_state(|| 0);
    let mut modal = hooks.use_state(|| Modal::None);
    let mut confirm_choice = hooks.use_state(|| ConfirmChoice::Yes);

    let mut eprintln = |msg: &str| {
        err_buffer.set(format!("{}{}\n", err_buffer.read().as_str(), msg));
    };

    let items = hooks.use_state(|| {
        let entries = match std::fs::read_dir(&props.dir) {
            Ok(entries) => entries.collect::<Vec<_>>(),
            Err(err) => {
                eprintln(&format!("ERROR: Could not read directory: {}", err));
                should_exit.set(Some(1));
                vec![]
            }
        };

        let mut items = vec![];

        for entry in entries {
            if let Err(err) = entry {
                eprintln(&format!("ERROR: Could not read entry: {}", err));
                should_exit.set(Some(1));
                continue;
            }

            let entry = entry.unwrap();

            let metadata = entry.metadata();
            if let Err(err) = metadata {
                eprintln(&format!("ERROR: Could not read entry metadata: {}", err));
                should_exit.set(Some(1));
                continue;
            }
            let metadata = metadata.unwrap();

            let is_file = metadata.is_file();
            let path = entry.path();

            let filename = path.file_name();
            if let Some(filename) = filename {
                let filename = filename.to_string_lossy().to_string();
                items.push((is_file, filename));
            }
        }

        items.sort_by(|a, b| {
            let a = a.1.to_lowercase();
            let b = b.1.to_lowercase();
            a.cmp(&b)
        });

        items
    });

    let mut history_items = hooks.use_state(Vec::<(String, String)>::new);
    let mut match_field = hooks.use_state(|| "(.*)".to_string());
    let mut rename_field = hooks.use_state(|| "${1}".to_string());
    let mut match_cursor = hooks.use_state(|| "(.*)".len());
    let mut rename_cursor = hooks.use_state(|| "${1}".len());
    let mut focused_field = hooks.use_state(|| FocusedField::Match);

    let item_count = items.read().len() as i32;

    let match_re = Regex::new(&match_field.read().to_string())
        .unwrap_or_else(|_| Regex::new("^$").unwrap());

    let item_elts = items
        .read()
        .iter()
        .map(|(is_file, filename)| {
            let label = if *is_file {
                format!("📄 {}", filename)
            } else {
                format!("📁 {}/", filename)
            };

            element! {
                Text(
                    color: if match_re.is_match(filename) {
                        Color::Green
                    } else {
                        Color::Grey
                    },
                    content: label,
                )
            }
        })
        .collect::<Vec<_>>();

    let renamed_elts = items
        .read()
        .iter()
        .map(|(is_file, filename)| {
            let pattern = rename_field.to_string();

            if match_re.is_match(filename) && !pattern.is_empty() {
                let renamed = match_re.replace_all(filename, &pattern);
                let label = if *is_file {
                    format!("📄 {}", renamed)
                } else {
                    format!("📁 {}/", renamed)
                };

                element! {
                    Text(
                        color: Color::Red,
                        content: label,
                    )
                }
            } else {
                let label = if *is_file {
                    format!("📄 {}", filename)
                } else {
                    format!("📁 {}/", filename)
                };

                element! {
                    Text(
                        color: Color::Grey,
                        content: label,
                    )
                }
            }
        })
        .collect::<Vec<_>>();

    hooks.use_terminal_events({
        let dir = props.dir.clone();
        let confirm_flag = props.confirm;
        let prevent_delete_flag = props.prevent_delete;

        move |event| {
            let mut oprintln = |msg: &str| {
                out_buffer.set(format!("{}{}\n", out_buffer.read().as_str(), msg));
            };
            let mut eprintln = |msg: &str| {
                err_buffer.set(format!("{}{}\n", err_buffer.read().as_str(), msg));
            };

            let match_re = Regex::new(&match_field.read().to_string())
                .unwrap_or_else(|_| Regex::new("^$").unwrap());

            match event {
                TerminalEvent::Key(KeyEvent {
                    code,
                    kind,
                    modifiers,
                    ..
                }) if kind != KeyEventKind::Release => {
                    let current_modal = modal.read().clone();
                    match current_modal {
                        Modal::Confirm => {
                            match code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    let match_pattern = match_field.read().to_string();
                                    let pattern = rename_field.to_string();
                                    if prevent_delete_flag {
                                        if let Err(violation) = check_preflight_safety(
                                            &items.read(),
                                            &match_re,
                                            &pattern,
                                        ) {
                                            modal.set(Modal::Blocked(violation));
                                            return;
                                        }
                                    }
                                    perform_rename(
                                        &items.read(),
                                        &match_re,
                                        &match_pattern,
                                        &pattern,
                                        &dir,
                                        &mut oprintln,
                                        &mut eprintln,
                                        &mut should_exit,
                                    );
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    // Dismiss confirmation modal and return to active editing
                                    modal.set(Modal::None);
                                }
                                KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                                    confirm_choice.set(match confirm_choice.get() {
                                        ConfirmChoice::Yes => ConfirmChoice::No,
                                        ConfirmChoice::No => ConfirmChoice::Yes,
                                    });
                                }
                                KeyCode::Enter => {
                                    match confirm_choice.get() {
                                        ConfirmChoice::Yes => {
                                            let match_pattern = match_field.read().to_string();
                                            let pattern = rename_field.to_string();
                                            if prevent_delete_flag {
                                                if let Err(violation) = check_preflight_safety(
                                                    &items.read(),
                                                    &match_re,
                                                    &pattern,
                                                ) {
                                                    modal.set(Modal::Blocked(violation));
                                                    return;
                                                }
                                            }
                                            perform_rename(
                                                &items.read(),
                                                &match_re,
                                                &match_pattern,
                                                &pattern,
                                                &dir,
                                                &mut oprintln,
                                                &mut eprintln,
                                                &mut should_exit,
                                            );
                                        }
                                        ConfirmChoice::No => {
                                            // Dismiss modal without executing
                                            modal.set(Modal::None);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Modal::Blocked { .. } => {
                            match code {
                                KeyCode::Enter
                                | KeyCode::Esc
                                | KeyCode::Char(' ')
                                | KeyCode::Char('o')
                                | KeyCode::Char('O') => {
                                    modal.set(Modal::None);
                                }
                                _ => {}
                            }
                        }
                        Modal::History { selected_idx } => {
                            let history = history_items.read();
                            let history_len = history.len();
                            match code {
                                KeyCode::Esc => {
                                    modal.set(Modal::None);
                                }
                                KeyCode::Up if selected_idx > 0 => {
                                    modal.set(Modal::History {
                                        selected_idx: selected_idx - 1,
                                    });
                                }
                                KeyCode::Down
                                    if history_len > 0 && selected_idx + 1 < history_len =>
                                {
                                    modal.set(Modal::History {
                                        selected_idx: selected_idx + 1,
                                    });
                                }
                                KeyCode::Char('r') | KeyCode::Char('R')
                                    if modifiers.contains(KeyModifiers::CONTROL)
                                        && history_len > 0 =>
                                {
                                    let next_idx = (selected_idx + 1) % history_len;
                                    modal.set(Modal::History {
                                        selected_idx: next_idx,
                                    });
                                }
                                KeyCode::Enter => {
                                    if history_len > 0 && selected_idx < history_len {
                                        let (m, r) = &history[selected_idx];
                                        match_field.set(m.clone());
                                        match_cursor.set(m.len());
                                        rename_field.set(r.clone());
                                        rename_cursor.set(r.len());
                                    }
                                    modal.set(Modal::None);
                                }
                                _ => {}
                            }
                        }
                        Modal::None => {
                            match code {
                                KeyCode::Char('r') | KeyCode::Char('R')
                                    if modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    let items = load_history();
                                    history_items.set(items);
                                    modal.set(Modal::History { selected_idx: 0 });
                                }
                                KeyCode::Esc => {
                                    should_exit.set(Some(0));
                                }
                                KeyCode::Up => scroll_offset.set((scroll_offset.get() - 1).max(0)),
                                KeyCode::Down => {
                                    scroll_offset.set((scroll_offset.get() + 1).min(item_count - 1))
                                }
                                KeyCode::Tab => match focused_field.get() {
                                    FocusedField::Match => {
                                        focused_field.set(FocusedField::Rename);
                                    }
                                    FocusedField::Rename => {
                                        focused_field.set(FocusedField::Match);
                                    }
                                },
                                KeyCode::Enter => {
                                    let match_pattern = match_field.read().to_string();
                                    let pattern = rename_field.to_string();

                                    if prevent_delete_flag {
                                        if let Err(violation) = check_preflight_safety(
                                            &items.read(),
                                            &match_re,
                                            &pattern,
                                        ) {
                                            modal.set(Modal::Blocked(violation));
                                            return;
                                        }
                                    }

                                    if confirm_flag {
                                        confirm_choice.set(ConfirmChoice::Yes);
                                        modal.set(Modal::Confirm);
                                        return;
                                    }

                                    perform_rename(
                                        &items.read(),
                                        &match_re,
                                        &match_pattern,
                                        &pattern,
                                        &dir,
                                        &mut oprintln,
                                        &mut eprintln,
                                        &mut should_exit,
                                    );
                                }
                                KeyCode::Left => match focused_field.get() {
                                    FocusedField::Match => {
                                        let value = match_field.read().to_string();
                                        let cursor = match_cursor.get();
                                        match_cursor.set(prev_char_boundary(&value, cursor));
                                    }
                                    FocusedField::Rename => {
                                        let value = rename_field.read().to_string();
                                        let cursor = rename_cursor.get();
                                        rename_cursor.set(prev_char_boundary(&value, cursor));
                                    }
                                },
                                KeyCode::Right => match focused_field.get() {
                                    FocusedField::Match => {
                                        let value = match_field.read().to_string();
                                        let cursor = match_cursor.get();
                                        match_cursor.set(next_char_boundary(&value, cursor));
                                    }
                                    FocusedField::Rename => {
                                        let value = rename_field.read().to_string();
                                        let cursor = rename_cursor.get();
                                        rename_cursor.set(next_char_boundary(&value, cursor));
                                    }
                                },
                                KeyCode::Home => match focused_field.get() {
                                    FocusedField::Match => match_cursor.set(0),
                                    FocusedField::Rename => rename_cursor.set(0),
                                },
                                KeyCode::End => match focused_field.get() {
                                    FocusedField::Match => {
                                        match_cursor.set(match_field.read().len());
                                    }
                                    FocusedField::Rename => {
                                        rename_cursor.set(rename_field.read().len());
                                    }
                                },
                                KeyCode::Backspace => match focused_field.get() {
                                    FocusedField::Match => {
                                        let mut value = match_field.read().to_string();
                                        let cursor = match_cursor.get();
                                        let prev = prev_char_boundary(&value, cursor);
                                        if prev < cursor {
                                            value.replace_range(prev..cursor, "");
                                            match_cursor.set(prev);
                                            match_field.set(value);
                                        }
                                    }
                                    FocusedField::Rename => {
                                        let mut value = rename_field.read().to_string();
                                        let cursor = rename_cursor.get();
                                        let prev = prev_char_boundary(&value, cursor);
                                        if prev < cursor {
                                            value.replace_range(prev..cursor, "");
                                            rename_cursor.set(prev);
                                            rename_field.set(value);
                                        }
                                    }
                                },
                                KeyCode::Delete => match focused_field.get() {
                                    FocusedField::Match => {
                                        let mut value = match_field.read().to_string();
                                        let cursor = match_cursor.get();
                                        let next = next_char_boundary(&value, cursor);
                                        if cursor < next {
                                            value.replace_range(cursor..next, "");
                                            match_field.set(value);
                                        }
                                    }
                                    FocusedField::Rename => {
                                        let mut value = rename_field.read().to_string();
                                        let cursor = rename_cursor.get();
                                        let next = next_char_boundary(&value, cursor);
                                        if cursor < next {
                                            value.replace_range(cursor..next, "");
                                            rename_field.set(value);
                                        }
                                    }
                                },
                                KeyCode::Char(c)
                                    if modifiers.is_empty()
                                        || modifiers == KeyModifiers::SHIFT =>
                                {
                                    match focused_field.get() {
                                        FocusedField::Match => {
                                            let mut value = match_field.read().to_string();
                                            let cursor = match_cursor.get().min(value.len());
                                            value.insert(cursor, c);
                                            match_cursor.set(cursor + c.len_utf8());
                                            match_field.set(value);
                                        }
                                        FocusedField::Rename => {
                                            let mut value = rename_field.read().to_string();
                                            let cursor = rename_cursor.get().min(value.len());
                                            value.insert(cursor, c);
                                            rename_cursor.set(cursor + c.len_utf8());
                                            rename_field.set(value);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    });

    if should_exit.get().is_some() {
        props.exit_code = should_exit.get().unwrap();
        props.out_buffer = out_buffer.read().to_string();
        props.err_buffer = err_buffer.read().to_string();
        system.exit();
    }

    element! {
        View(
            width,
            height,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Stretch,
        ) {
            View(
                width: Size::Percent(100f32),
                height: 1,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
            ) {
                View(
                    flex_shrink: 1f32,
                ) {
                    Text(content: "Directory: ")
                }
                View(
                    flex_grow: 1f32,
                    background_color: Color::Black,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    TextInput(value: format!("{}", props.dir.display()))
                }
                #({
                    let (safe_label, safe_bg, safe_fg) = if !props.prevent_delete && !props.confirm {
                        ("⚠️ UNSAFE", Color::Red, Color::White)
                    } else if !props.prevent_delete {
                        ("⚠️ OVERWRITE", Color::Red, Color::White)
                    } else {
                        ("🛡️ SAFE", Color::DarkGrey, Color::Cyan)
                    };

                    let (confirm_label, confirm_bg, confirm_fg) = if props.confirm {
                        ("✓ CONFIRM", Color::DarkGrey, Color::Green)
                    } else {
                        ("⚡ NO-CONFIRM", Color::DarkGrey, Color::Yellow)
                    };

                    element! {
                        View(
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            gap: 1,
                            padding_left: 1,
                            padding_right: 1,
                        ) {
                            View(
                                background_color: safe_bg,
                                padding_left: 1,
                                padding_right: 1,
                            ) {
                                Text(
                                    color: safe_fg,
                                    content: safe_label.to_string(),
                                )
                            }
                            View(
                                background_color: confirm_bg,
                                padding_left: 1,
                                padding_right: 1,
                            ) {
                                Text(
                                    color: confirm_fg,
                                    content: confirm_label.to_string(),
                                )
                            }
                            View(
                                background_color: Color::DarkGrey,
                                padding_left: 1,
                                padding_right: 1,
                            ) {
                                Text(
                                    color: Color::White,
                                    content: "Ctrl+R: HISTORY".to_string(),
                                )
                            }
                        }
                    }.into_any()
                })
            }
            View(
                width: Size::Percent(100f32),
                height: height - 2,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Stretch,
            ) {
                View(
                    flex_grow: 1f32,
                    border_style: BorderStyle::Double,
                    border_color: Color::Blue,
                    padding_left: 1,
                    padding_right: 1,
                    overflow: Overflow::Scroll,
                ) {
                    View(
                        width: Size::Percent(100f32),
                        height: item_count,
                    ) {
                        View(
                            width: Size::Percent(100f32),
                            position: Position::Absolute,
                            top: -scroll_offset.get(),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Start,
                        ) {
                            #(item_elts)
                        }
                    }
                }
                View(
                    flex_grow: 1f32,
                    border_style: BorderStyle::Double,
                    border_color: Color::Blue,
                    padding_left: 1,
                    padding_right: 1,
                    overflow: Overflow::Scroll,
                ) {
                    View(
                        width: Size::Percent(100f32),
                        height: item_count,
                    ) {
                        View(
                            width: Size::Percent(100f32),
                            position: Position::Absolute,
                            top: -scroll_offset.get(),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Start,
                        ) {
                            #(renamed_elts)
                        }
                    }
                }
            }
            View(
                width: Size::Percent(100f32),
                height: 1,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
                gap: 1,
            ) {
                View(
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Stretch,
                    flex_grow: 1f32,
                    gap: 1,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    View(
                        flex_shrink: 1f32,
                    ) {
                        Text(content: "Match:")
                    }
                    View(
                        flex_grow: 1f32,
                        background_color: if focused_field.get() == FocusedField::Match {
                            Color::DarkGrey
                        } else {
                            Color::Black
                        },
                        padding_left: 1,
                        padding_right: 1,
                    ) {
                        #({
                            let has_focus = focused_field.get() == FocusedField::Match;
                            let value = match_field.read().to_string();
                            let cursor = match_cursor.get();

                            element! {
                                View(
                                    width: Size::Percent(100f32),
                                    height: 1,
                                    overflow: Overflow::Hidden,
                                ) {
                                    Text(
                                        wrap: TextWrap::NoWrap,
                                        content: value.clone(),
                                    )
                                    #({
                                        if has_focus {
                                            element! {
                                                View(
                                                    position: Position::Absolute,
                                                    top: 0,
                                                    left: cursor_column(value.as_str(), cursor),
                                                    width: 1,
                                                    height: 1,
                                                    background_color: Color::White,
                                                    overflow: Overflow::Hidden,
                                                ) {
                                                    Text(
                                                        color: Color::Black,
                                                        wrap: TextWrap::NoWrap,
                                                        content: cursor_cell(value.as_str(), cursor),
                                                    )
                                                }
                                            }
                                            .into_any()
                                        } else {
                                            element! { View(width: 0, height: 0) }
                                                .into_any()
                                        }
                                    })
                                }
                            }
                            .into_any()
                        })
                    }
                }
                View(
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Stretch,
                    flex_grow: 1f32,
                    gap: 1,
                    padding_left: 1,
                    padding_right: 1,
                ) {
                    View(
                        flex_shrink: 1f32,
                    ) {
                        Text(content: "Rename:")
                    }
                    View(
                        flex_grow: 1f32,
                        background_color: if focused_field.get() == FocusedField::Rename {
                            Color::DarkGrey
                        } else {
                            Color::Black
                        },
                        padding_left: 1,
                        padding_right: 1,
                    ) {
                        #({
                            let has_focus = focused_field.get() == FocusedField::Rename;
                            let value = rename_field.read().to_string();
                            let cursor = rename_cursor.get();

                            element! {
                                View(
                                    width: Size::Percent(100f32),
                                    height: 1,
                                    overflow: Overflow::Hidden,
                                ) {
                                    Text(
                                        wrap: TextWrap::NoWrap,
                                        content: value.clone(),
                                    )
                                    #({
                                        if has_focus {
                                            element! {
                                                View(
                                                    position: Position::Absolute,
                                                    top: 0,
                                                    left: cursor_column(value.as_str(), cursor),
                                                    width: 1,
                                                    height: 1,
                                                    background_color: Color::White,
                                                    overflow: Overflow::Hidden,
                                                ) {
                                                    Text(
                                                        color: Color::Black,
                                                        wrap: TextWrap::NoWrap,
                                                        content: cursor_cell(value.as_str(), cursor),
                                                    )
                                                }
                                            }
                                            .into_any()
                                        } else {
                                            element! { View(width: 0, height: 0) }
                                                .into_any()
                                        }
                                    })
                                }
                            }
                            .into_any()
                        })
                    }
                }
            }
            #({
                match modal.read().clone() {
                    Modal::None => element! { View(width: 0, height: 0) }.into_any(),
                    Modal::Confirm => element! {
                        View(
                            position: Position::Absolute,
                            top: 0,
                            left: 0,
                            width,
                            height,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                        ) {
                            View(
                                width: (width - 4).clamp(36, 58),
                                height: 7,
                                border_style: BorderStyle::Round,
                                border_color: Color::Yellow,
                                background_color: Color::Black,
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceAround,
                                padding_left: 2,
                                padding_right: 2,
                            ) {
                                Text(
                                    color: Color::White,
                                    content: "Are you sure you want to execute this rename?".to_string(),
                                )
                                View(
                                    flex_direction: FlexDirection::Row,
                                    justify_content: JustifyContent::Center,
                                    gap: 3,
                                ) {
                                    View(
                                        background_color: if confirm_choice.get() == ConfirmChoice::Yes {
                                            Color::Cyan
                                        } else {
                                            Color::DarkGrey
                                        },
                                        padding_left: 1,
                                        padding_right: 1,
                                    ) {
                                        Text(
                                            color: if confirm_choice.get() == ConfirmChoice::Yes {
                                                Color::Black
                                            } else {
                                                Color::White
                                            },
                                            content: "Yes (y)".to_string(),
                                        )
                                    }
                                    View(
                                        background_color: if confirm_choice.get() == ConfirmChoice::No {
                                            Color::Cyan
                                        } else {
                                            Color::DarkGrey
                                        },
                                        padding_left: 1,
                                        padding_right: 1,
                                    ) {
                                        Text(
                                            color: if confirm_choice.get() == ConfirmChoice::No {
                                                Color::Black
                                            } else {
                                                Color::White
                                            },
                                            content: "No (n / Esc)".to_string(),
                                        )
                                    }
                                }
                                Text(
                                    color: Color::Grey,
                                    content: "(Press 'y' to confirm, 'n' or Esc to cancel)".to_string(),
                                )
                            }
                        }
                    }.into_any(),
                    Modal::Blocked(ref violation) => {
                        let (title, line1, line2) = match violation {
                            SafetyViolation::CountDecrease { before, after } => (
                                "CANNOT EXECUTE RENAME".to_string(),
                                format!("File count would decrease from {} to {}.", before, after),
                                "Files would be overwritten or deleted.".to_string(),
                            ),
                            SafetyViolation::ChainCollision { source, target } => (
                                "CANNOT EXECUTE RENAME: CHAIN COLLISION".to_string(),
                                format!("'{}' renames to existing '{}'.", source, target),
                                "Sequential rename would overwrite file before it is moved.".to_string(),
                            ),
                        };

                        element! {
                            View(
                                position: Position::Absolute,
                                top: 0,
                                left: 0,
                                width,
                                height,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                            ) {
                                View(
                                    width: (width - 4).clamp(36, 68),
                                    height: 8,
                                    border_style: BorderStyle::Round,
                                    border_color: Color::Red,
                                    background_color: Color::Black,
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::SpaceAround,
                                    padding_left: 2,
                                    padding_right: 2,
                                ) {
                                    Text(
                                        color: Color::Red,
                                        content: title,
                                    )
                                    Text(
                                        color: Color::White,
                                        content: line1,
                                    )
                                    Text(
                                        color: Color::Yellow,
                                        content: line2,
                                    )
                                    Text(
                                        color: Color::Grey,
                                        content: "(Press Enter, Esc, or Space to dismiss)".to_string(),
                                    )
                                }
                            }
                        }.into_any()
                    }
                    Modal::History { selected_idx } => {
                        let history = history_items.read();
                        let modal_width = (width - 4).clamp(42, 78);
                        let max_visible = 6;
                        let total = history.len();

                        let (start_idx, end_idx) = if total <= max_visible {
                            (0, total)
                        } else {
                            let half = max_visible / 2;
                            if selected_idx < half {
                                (0, max_visible)
                            } else if selected_idx + (max_visible - half) >= total {
                                (total - max_visible, total)
                            } else {
                                (selected_idx - half, selected_idx - half + max_visible)
                            }
                        };

                        let history_elts: Vec<AnyElement<'static>> = if history.is_empty() {
                            vec![
                                element! {
                                    View(
                                        padding_top: 1,
                                        padding_bottom: 1,
                                        align_items: AlignItems::Center,
                                    ) {
                                        Text(
                                            color: Color::Grey,
                                            content: "No history yet. Successful renames will be saved here.".to_string(),
                                        )
                                    }
                                }.into_any()
                            ]
                        } else {
                            (start_idx..end_idx).map(|i| {
                                let (m, r) = &history[i];
                                let is_selected = i == selected_idx;
                                let prefix = if is_selected { "▸ " } else { "  " };
                                let line_content = format!("{}{:<24} → {}", prefix, format!("/{}/", m), r);

                                element! {
                                    View(
                                        background_color: if is_selected {
                                            Color::Cyan
                                        } else {
                                            Color::Black
                                        },
                                        padding_left: 1,
                                        padding_right: 1,
                                        width: Size::Percent(100f32),
                                    ) {
                                        Text(
                                            color: if is_selected {
                                                Color::Black
                                            } else {
                                                Color::White
                                            },
                                            wrap: TextWrap::NoWrap,
                                            content: line_content,
                                        )
                                    }
                                }.into_any()
                            }).collect()
                        };

                        let footer_text = if history.is_empty() {
                            "(Press Esc to close)".to_string()
                        } else {
                            format!("(↑/↓: select, Enter: load, Esc: cancel | {} of {})", selected_idx + 1, total)
                        };

                        element! {
                            View(
                                position: Position::Absolute,
                                top: 0,
                                left: 0,
                                width,
                                height,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                            ) {
                                View(
                                    width: modal_width,
                                    border_style: BorderStyle::Round,
                                    border_color: Color::Cyan,
                                    background_color: Color::Black,
                                    flex_direction: FlexDirection::Column,
                                    align_items: AlignItems::Center,
                                    padding_left: 2,
                                    padding_right: 2,
                                    padding_top: 1,
                                    padding_bottom: 1,
                                    gap: 1,
                                ) {
                                    Text(
                                        color: Color::Cyan,
                                        content: "Command History (Ctrl+R)".to_string(),
                                    )
                                    View(
                                        flex_direction: FlexDirection::Column,
                                        width: Size::Percent(100f32),
                                    ) {
                                        #(history_elts)
                                    }
                                    Text(
                                        color: Color::Grey,
                                        content: footer_text,
                                    )
                                }
                            }
                        }.into_any()
                    }
                }
            })
        }
    }
}

pub fn calculate_rename_counts(
    items: &[(bool, String)],
    match_re: &Regex,
    pattern: &str,
) -> (usize, usize) {
    let before_count = items.len();
    if pattern.is_empty() {
        return (before_count, before_count);
    }
    let mut after_filenames = std::collections::HashSet::new();
    for (_, filename) in items {
        if match_re.is_match(filename) {
            let renamed = match_re.replace_all(filename, pattern).to_string();
            after_filenames.insert(renamed);
        } else {
            after_filenames.insert(filename.clone());
        }
    }
    (before_count, after_filenames.len())
}

pub fn check_preflight_safety(
    items: &[(bool, String)],
    match_re: &Regex,
    pattern: &str,
) -> Result<(), SafetyViolation> {
    let (before, after) = calculate_rename_counts(items, match_re, pattern);
    if after < before {
        return Err(SafetyViolation::CountDecrease { before, after });
    }

    if pattern.is_empty() {
        return Ok(());
    }

    let mut sources = std::collections::HashSet::new();
    let mut renames = Vec::new();

    for (_, filename) in items {
        if match_re.is_match(filename) {
            let renamed = match_re.replace_all(filename, pattern).to_string();
            if &renamed != filename {
                sources.insert(filename.clone());
                renames.push((filename.clone(), renamed));
            }
        }
    }

    for (source, target) in renames {
        if sources.contains(&target) {
            return Err(SafetyViolation::ChainCollision { source, target });
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct CliArgs {
    pub dir: std::path::PathBuf,
    pub confirm: bool,
    pub prevent_delete: bool,
    pub help: bool,
}

pub fn parse_cli_args<I, T>(args: I) -> Result<CliArgs, String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let mut dir: Option<std::path::PathBuf> = None;
    let mut confirm = true;
    let mut prevent_delete = true;
    let mut help = false;

    for arg in args {
        let arg = arg.as_ref();
        match arg {
            "-h" | "--help" => {
                help = true;
            }
            "--no-confirm" | "-y" | "--yes" => {
                confirm = false;
            }
            "--overwrite" | "--force" | "-f" => {
                prevent_delete = false;
            }
            "--unsafe" => {
                confirm = false;
                prevent_delete = false;
            }
            "-c" | "--confirm" | "-i" | "--interactive" => {
                confirm = true;
            }
            "-s" | "--safe" => {
                confirm = true;
                prevent_delete = true;
            }
            "-p" | "--prevent-delete" | "-n" | "--no-overwrite" | "--prevent-overwrite" => {
                prevent_delete = true;
            }
            s if s.starts_with('-') => {
                return Err(format!("Unknown option: {}", s));
            }
            s => {
                if dir.is_some() {
                    return Err(format!("Unexpected argument: {}", s));
                }
                dir = Some(std::path::PathBuf::from(s));
            }
        }
    }

    let dir = match dir {
        Some(path) => match path.canonicalize() {
            Ok(p) => p,
            Err(e) => return Err(format!("Could not access directory '{}': {}", path.display(), e)),
        },
        None => match std::env::current_dir() {
            Ok(p) => match p.canonicalize() {
                Ok(canon) => canon,
                Err(_) => p,
            },
            Err(e) => return Err(format!("Could not determine current directory: {}", e)),
        },
    };

    Ok(CliArgs {
        dir,
        confirm,
        prevent_delete,
        help,
    })
}

pub fn print_help() {
    println!("regname - Mass renamer TUI written in Rust");
    println!();
    println!("USAGE:");
    println!("    regname [OPTIONS] [DIR]");
    println!();
    println!("OPTIONS:");
    println!("    Safe mode is enabled by default (prompts for confirmation and blocks file overwrites).");
    println!();
    println!("    -y, --no-confirm, --yes");
    println!("            Skip confirmation pop-up dialog");
    println!("    -f, --force, --overwrite");
    println!("            Allow renaming even if files would be overwritten or deleted");
    println!("    --unsafe");
    println!("            Bypass all safety checks (disables both confirmation and overwrite guard)");
    println!("    -c, --confirm, -i, --interactive");
    println!("            Require confirmation pop-up (enabled by default)");
    println!("    -p, --prevent-delete, -n, --no-overwrite, -s, --safe");
    println!("            Block overwriting/deletion (enabled by default)");
    println!("    -h, --help");
    println!("            Print help information");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli_args = match parse_cli_args(&args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("ERROR: {}", err);
            std::process::exit(1);
        }
    };

    if cli_args.help {
        print_help();
        std::process::exit(0);
    }

    let mut elt = element! {
        App(
            dir: cli_args.dir,
            confirm: cli_args.confirm,
            prevent_delete: cli_args.prevent_delete,
        )
    };
    smol::block_on(elt.fullscreen()).unwrap();
    print!("{}", elt.props.out_buffer);
    eprint!("{}", elt.props.err_buffer);
    std::process::exit(elt.props.exit_code);
}
