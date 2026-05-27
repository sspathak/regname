use iocraft::prelude::*;
use regex::Regex;
use smol;

#[derive(Clone, Copy, PartialEq)]
enum FocusedField {
    Match,
    Rename,
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
}

#[component]
fn App(props: &mut AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let (width, height) = hooks.use_terminal_size();
    let mut system = hooks.use_context_mut::<SystemContext>();

    let mut out_buffer = hooks.use_state(|| String::new());
    let mut err_buffer = hooks.use_state(|| String::new());
    let mut should_exit = hooks.use_state::<Option<i32>, _>(|| None);
    let mut scroll_offset = hooks.use_state(|| 0);

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
            }
            else {
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
                })
                    if { kind != KeyEventKind::Release } =>
                {
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

                            for (_, filename) in items.read().iter() {
                                if match_re.is_match(filename) && !pattern.is_empty() {
                                    let renamed = match_re.replace_all(
                                        filename,
                                        &pattern,
                                    ).to_string();

                                    oprintln(&format!("{} -> {}", filename, renamed));

                                    let old_path = dir.join(filename);
                                    let new_path = dir.join(&renamed);

                                    match std::fs::rename(old_path, new_path) {
                                        Ok(_) => {}
                                        Err(err) => {
                                            eprintln(&format!(
                                                "ERROR: Could not rename file: {}",
                                                err,
                                            ));
                                            should_exit.set(Some(1));
                                            break;
                                        }
                                    }
                                }
                            }

                            should_exit.set(Some(0));
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
                        KeyCode::Char(c) if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT => {
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
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .map(|s| match std::path::PathBuf::from(s).canonicalize() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("ERROR: {}", err);
                std::process::exit(1);
            },
        })
        .or_else(|| match std::env::current_dir() {
            Ok(path) => Some(path),
            Err(err) => {
                eprintln!("ERROR: {}", err);
                std::process::exit(1);
            },
        })
        .unwrap();

    let mut elt = element! {App(dir: dir)};
    smol::block_on(elt.fullscreen()).unwrap();
    print!("{}", elt.props.out_buffer);
    eprint!("{}", elt.props.err_buffer);
    std::process::exit(elt.props.exit_code);
}
