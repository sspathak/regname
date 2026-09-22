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

fn perform_rename(
    items: &[(bool, String)],
    match_re: &Regex,
    pattern: &str,
    dir: &std::path::Path,
    oprintln: &mut dyn FnMut(&str),
    eprintln: &mut dyn FnMut(&str),
    should_exit: &mut State<Option<i32>>,
) {
    for (_, filename) in items.iter() {
        if match_re.is_match(filename) && !pattern.is_empty() {
            let renamed = match_re.replace_all(filename, pattern).to_string();

            oprintln(&format!("{} -> {}", filename, renamed));

            let old_path = dir.join(filename);
            let new_path = dir.join(&renamed);

            match std::fs::rename(old_path, new_path) {
                Ok(_) => {}
                Err(err) => {
                    eprintln(&format!("ERROR: Could not rename file: {}", err));
                    should_exit.set(Some(1));
                    return;
                }
            }
        }
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

    let mut match_field = hooks.use_state(|| "(.*)".to_string());
    let mut rename_field = hooks.use_state(|| "$1".to_string());
    let mut match_cursor = hooks.use_state(|| "(.*)".len());
    let mut rename_cursor = hooks.use_state(|| "$1".len());
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
                        Modal::None => {
                            match code {
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
    let mut confirm = false;
    let mut prevent_delete = false;
    let mut help = false;

    for arg in args {
        let arg = arg.as_ref();
        match arg {
            "-h" | "--help" => {
                help = true;
            }
            "-c" | "--confirm" | "-i" | "--interactive" => {
                confirm = true;
            }
            "-s" | "--safe" => {
                // --safe and -s also enable the confirm flag
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
    println!("    -c, --confirm, -i, --interactive");
    println!("            Prompt for confirmation with a pop-up before executing rename");
    println!("    -p, --prevent-delete, -n, --no-overwrite");
    println!("            Prevent rename if before and after file count decreases");
    println!("    -s, --safe");
    println!("            Safe mode: enables both --prevent-delete and --confirm");
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
