use slint::{Model, ModelRc, SharedString, StyledText, VecModel};
use std::cell::RefCell;
use std::convert::From;
use std::ffi::OsStr;
use std::{path::Path, rc::Rc};

use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

use crate::git_utils::{GitDiffLine, LineType};

slint::include_modules!();

mod git_utils;

impl From<&LineType> for LineStatus {
    fn from(value: &LineType) -> Self {
        match value {
            LineType::Added => LineStatus::Added,
            LineType::Removed => LineStatus::Removed,
            LineType::Unchanged => LineStatus::Unchanged,
        }
    }
}

fn color_to_hex(c: syntect::highlighting::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

struct HighLighterConfig {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme: String,
}

impl HighLighterConfig {
    fn new(theme: &str) -> Self {
        HighLighterConfig {
            syntax_set: SyntaxSet::load_defaults_nonewlines(),
            theme_set: ThemeSet::load_defaults(),
            theme: theme.to_owned(),
        }
    }
}

struct FileHighlighter<'a> {
    config: &'a HighLighterConfig,
    hightlight_lines: HighlightLines<'a>,
}

impl<'a> FileHighlighter<'a> {
    fn new(config: &'a HighLighterConfig, file_extension: &str) -> Self {
        let syntax = config
            .syntax_set
            .find_syntax_by_extension(file_extension)
            .expect("Could not find syntax for rust (rs)!");
        FileHighlighter {
            config,
            hightlight_lines: HighlightLines::new(syntax, &config.theme_set.themes[&config.theme]),
        }
    }
    fn highlight_line(&mut self, line: &str) -> StyledText {
        let regions = self
            .hightlight_lines
            .highlight_line(line, &self.config.syntax_set)
            .unwrap();

        let html_line = regions
            .into_iter()
            .map(|(style, text)| {
                let escaped_text = html_escape::encode_text(text);
                format!(
                    r#"<font color="{}">{}</font>"#,
                    color_to_hex(style.foreground),
                    escaped_text,
                )
            })
            .collect::<Vec<_>>()
            .join("");

        slint::StyledText::from_markdown(&html_line).unwrap()
    }
}

fn create_diff_model(
    changed_lines: &[GitDiffLine],
    highlighter: &mut FileHighlighter,
    diff_model: Rc<VecModel<DiffLine>>,
) {
    let mut old_line_counter = 0;
    let mut new_line_counter = 0;

    let v = changed_lines
        .iter()
        .map(|diff| {
            let old_no = if diff.status == LineType::Added {
                -1
            } else {
                old_line_counter += 1;
                old_line_counter
            };
            let new_no = if diff.status == LineType::Removed {
                -1
            } else {
                new_line_counter += 1;
                new_line_counter
            };
            DiffLine {
                source_line: SharedString::from(&diff.line),
                styled_line: highlighter.highlight_line(&diff.line),
                status: LineStatus::from(&diff.status),
                note: SharedString::new(),
                is_done: false,
                old_line_no: old_no,
                new_line_no: new_no,
            }
        })
        .collect::<Vec<_>>();
    diff_model.set_vec(v);
}

fn update_diff_line_colors(highlighter: &mut FileHighlighter, diff_model: Rc<VecModel<DiffLine>>) {
    diff_model
        .iter()
        .enumerate()
        .for_each(|(row, mut diff_line)| {
            diff_line.styled_line = highlighter.highlight_line(&diff_line.source_line);
            diff_model.set_row_data(row, diff_line);
        });
}

fn extension_from_filename(filename: &str) -> Option<&str> {
    Path::new(filename).extension().and_then(OsStr::to_str)
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ui_weak = ui.as_weak();
    let hightlighter_config = Rc::new(RefCell::new(HighLighterConfig::new("base16-eighties.dark")));

    let themes = hightlighter_config
        .borrow()
        .theme_set
        .themes
        .keys()
        .map(SharedString::from)
        .collect::<Vec<_>>();

    let theme_index = themes
        .iter()
        .position(|t| t == &hightlighter_config.borrow().theme)
        .map(|i| i as i32)
        .unwrap_or(-1);

    let themes_model: ModelRc<SharedString> = Rc::new(VecModel::from(themes)).into();

    ui.set_themes(themes_model);
    ui.set_current_theme_index(theme_index);

    let diff_model = Rc::new(VecModel::default());
    ui.set_diff_model(diff_model.clone().into());

    ui.on_load_diff({
        let config = hightlighter_config.clone();
        let diff_model = diff_model.clone();
        move |paranmeter: DiffParameter| {
            let diff_model = diff_model.clone();
            let config = config.borrow();
            let extension =
                extension_from_filename(&paranmeter.filename).expect("File extension not found!");

            let mut highlighter = FileHighlighter::new(&config, extension);

            let repository_path = Path::new(&paranmeter.repository);

            let git_diff =
                git_utils::GitDiff::create(repository_path).expect("GitDiff::create failed!");
            let changed_lines = git_diff
                .diff(&paranmeter.from, &paranmeter.to, &paranmeter.filename)
                .expect("Could not create diff!");

            create_diff_model(&changed_lines, &mut highlighter, diff_model);
        }
    });
    ui.on_set_theme({
        let config = hightlighter_config.clone();
        let diff_model = diff_model.clone();
        move |theme| {
            {
                let mut config = config.borrow_mut();
                config.theme = String::from(&theme);
            }
            let config = config.borrow();
            let diff_model = diff_model.clone();
            // let extension =
            //     extension_from_filename(&paranmeter.filename).expect("File extension not found!");

            let mut highlighter = FileHighlighter::new(&config, "rs");
            update_diff_line_colors(&mut highlighter, diff_model);
        }
    });

    ui.run()
}
