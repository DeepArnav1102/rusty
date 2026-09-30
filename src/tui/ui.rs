use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::tui::app::{ActiveModal, App, AuthField, COMMANDS, RemoteField};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Background base
    let base_block = Block::default().style(Style::default().bg(Color::Rgb(15, 17, 26)));
    f.render_widget(base_block, size);

    // Root vertical layout: Header, Main Body (Command List + Info/Console), Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Header Bar
            Constraint::Min(12),   // Middle Main Work Area
            Constraint::Length(3), // Bottom Footer / Keybindings
        ])
        .split(size);

    draw_header(f, app, chunks[0]);
    draw_body(f, app, chunks[1]);
    draw_footer(f, chunks[2]);

    // Modals
    match app.active_modal {
        ActiveModal::AuthLogin => draw_auth_modal(f, app),
        ActiveModal::PromptAdd => draw_add_modal(f, app),
        ActiveModal::PromptCommit => draw_commit_modal(f, app),
        ActiveModal::PromptRemoteAdd => draw_remote_add_modal(f, app),
        ActiveModal::Help => draw_help_modal(f),
        ActiveModal::None => {}
    }
}

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28), // Title
            Constraint::Min(20),    // Repo info & branch
            Constraint::Length(36), // User auth badge
        ])
        .split(area);

    // Title Block
    let title_line = Line::from(vec![
        Span::styled(
            "  ⚡ RUSTY ",
            Style::default()
                .fg(Color::Rgb(255, 140, 0))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "VCS DASHBOARD",
            Style::default()
                .fg(Color::Rgb(80, 250, 123))
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let title_block = Paragraph::new(title_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(255, 140, 0))),
    );
    f.render_widget(title_block, header_chunks[0]);

    // Repository status & branch
    let repo_status_span = if app.repo_detected {
        Span::styled(
            "● Repo OK",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "○ No Repo (.rusty)",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    };

    let branch_span = Span::styled(
        format!(" ⎇ branch: [{}]", app.current_branch),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let status_line = Line::from(vec![
        Span::raw(" "),
        repo_status_span,
        Span::raw("  | "),
        branch_span,
    ]);

    let status_block = Paragraph::new(status_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(100, 110, 145))),
    );
    f.render_widget(status_block, header_chunks[1]);

    // Authentication Badge
    let auth_line = if let Some(ref creds) = app.creds {
        let name = creds.username.as_deref().unwrap_or(&creds.email);
        Line::from(vec![
            Span::styled(" 🔒 ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{name}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" [Tab: Auth]", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " ⚠ Not Logged In ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[Tab: Login]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ])
    };

    let auth_border_color = if app.creds.is_some() {
        Color::Rgb(80, 250, 123)
    } else {
        Color::Rgb(255, 184, 108)
    };

    let auth_block = Paragraph::new(auth_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(auth_border_color)),
    );
    f.render_widget(auth_block, header_chunks[2]);
}

fn draw_body(f: &mut Frame, app: &mut App, area: Rect) {
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(32), // Command Palette (Navigation)
            Constraint::Min(40),    // Right side (Command Info Box + Activity Console)
        ])
        .split(area);

    draw_command_list(f, app, main_cols[0]);
    draw_right_panel(f, app, main_cols[1]);
}

fn draw_command_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = COMMANDS
        .iter()
        .enumerate()
        .map(|(idx, cmd)| {
            let is_selected = idx == app.selected_index;
            let (prefix, text_style, border_bracket) = if is_selected {
                (
                    " ❯ ",
                    Style::default()
                        .fg(Color::Rgb(255, 255, 255))
                        .bg(Color::Rgb(75, 40, 130))
                        .add_modifier(Modifier::BOLD),
                    Color::Rgb(255, 121, 198),
                )
            } else {
                (
                    "   ",
                    Style::default().fg(Color::Rgb(200, 200, 220)),
                    Color::Rgb(100, 110, 145),
                )
            };

            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Rgb(255, 121, 198))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{}] ", cmd.shortcut),
                    Style::default()
                        .fg(border_bracket)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(cmd.name, text_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list_widget = List::new(items).block(
        Block::default()
            .title(Span::styled(
                " ⚡ COMMAND PALETTE [↑/↓] ",
                Style::default()
                    .fg(Color::Rgb(189, 147, 249))
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(139, 92, 246))),
    );

    f.render_widget(list_widget, area);
}

fn draw_right_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let right_splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // Command Information Box
            Constraint::Min(8),    // Output & Log Console
        ])
        .split(area);

    draw_command_info(f, app, right_splits[0]);
    draw_console(f, app, right_splits[1]);
}

fn draw_command_info(f: &mut Frame, app: &App, area: Rect) {
    let current_cmd = &COMMANDS[app.selected_index];

    let info_text = vec![
        Line::from(vec![
            Span::styled(
                "Command: ",
                Style::default()
                    .fg(Color::Rgb(139, 233, 253))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("rusty {}", current_cmd.name.to_lowercase()),
                Style::default()
                    .fg(Color::Rgb(80, 250, 123))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Shortcut: ", Style::default().fg(Color::Rgb(139, 233, 253))),
            Span::styled(
                format!("'{}'", current_cmd.shortcut),
                Style::default()
                    .fg(Color::Rgb(255, 184, 108))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" or [Enter]", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Details: ",
                Style::default()
                    .fg(Color::Rgb(241, 250, 140))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(current_cmd.description, Style::default().fg(Color::White)),
        ]),
    ];

    let info_block = Paragraph::new(info_text)
        .block(
            Block::default()
                .title(Span::styled(
                    " 💡 COMMAND INFO ",
                    Style::default()
                        .fg(Color::Rgb(241, 250, 140))
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(255, 184, 108))),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(info_block, area);
}

fn draw_console(f: &mut Frame, app: &mut App, area: Rect) {
    // Split console area: top = log view, bottom = clear button bar (height 3)
    let console_splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(4),    // Log output area
            Constraint::Length(3), // Clear button bar
        ])
        .split(area);

    let log_area = console_splits[0];
    let btn_area = console_splits[1];

    // ── Log Output Area ──────────────────────────────────────────────────────
    let max_lines = log_area.height.saturating_sub(2) as usize;
    let total_logs = app.logs.len();
    let start_idx = total_logs.saturating_sub(max_lines);

    let log_lines: Vec<Line> = if app.logs.is_empty() {
        vec![Line::from(Span::styled(
            "  (no recent execution logs — select a command or press 's' for status)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.logs[start_idx..]
            .iter()
            .map(|entry| {
                if entry.contains("[SUCCESS]") {
                    Line::from(Span::styled(
                        entry,
                        Style::default().fg(Color::Rgb(80, 250, 123)),
                    ))
                } else if entry.contains("[ERROR]") {
                    Line::from(Span::styled(
                        entry,
                        Style::default()
                            .fg(Color::Rgb(255, 85, 85))
                            .add_modifier(Modifier::BOLD),
                    ))
                } else if entry.contains("[WARN]") {
                    Line::from(Span::styled(
                        entry,
                        Style::default().fg(Color::Rgb(255, 184, 108)),
                    ))
                } else if entry.contains("[SYSTEM]") {
                    Line::from(Span::styled(
                        entry,
                        Style::default().fg(Color::Rgb(189, 147, 249)),
                    ))
                } else {
                    Line::from(Span::styled(
                        entry,
                        Style::default().fg(Color::Rgb(248, 248, 242)),
                    ))
                }
            })
            .collect()
    };

    let title_line = Line::from(vec![
        Span::styled(
            " 📟 EXECUTION LOG & ACTIVITY ",
            Style::default()
                .fg(Color::Rgb(80, 250, 123))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if total_logs > 0 {
                format!(" {} entries ", total_logs)
            } else {
                "".to_string()
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let console_block = Paragraph::new(log_lines)
        .block(
            Block::default()
                .title(title_line)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(68, 71, 90))),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(console_block, log_area);

    // ── Clear Button Bar ─────────────────────────────────────────────────────
    // Build the clear button (centred inside btn_area).
    // We compute a tight rect around the button text so clicks land accurately.
    let btn_label = "  🗑  Clear Log & Activity  [x]  ";
    let btn_label_len = btn_label.chars().count() as u16;
    let btn_x = btn_area.x + btn_area.width.saturating_sub(btn_label_len + 2) / 2;
    let btn_y = btn_area.y + 1; // vertically centred inside the 3-line row
    let btn_width = btn_label_len.min(btn_area.width.saturating_sub(2));
    let btn_rect = Rect::new(btn_x, btn_y, btn_width, 1);

    // Store for mouse-click hit-test (add 1 col padding each side)
    app.clear_btn_rect = Some(Rect::new(
        btn_rect.x.saturating_sub(1),
        btn_rect.y,
        btn_rect.width + 2,
        btn_rect.height,
    ));

    // Outer bar block
    let bar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(68, 71, 90)));
    f.render_widget(bar_block, btn_area);

    // The button itself — rendered as a styled paragraph inside btn_rect
    let btn_para = Paragraph::new(Line::from(vec![Span::styled(
        btn_label,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Rgb(255, 120, 60))
            .add_modifier(Modifier::BOLD),
    )]))
    .alignment(Alignment::Center);

    f.render_widget(btn_para, btn_rect);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let keys = Line::from(vec![
        Span::styled(
            " [↑/↓] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(189, 147, 249))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Navigate   "),
        Span::styled(
            " [Enter] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(80, 250, 123))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Run   "),
        Span::styled(
            " [x] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(255, 184, 108))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Clear Log   "),
        Span::styled(
            " [Tab] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(255, 140, 0))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Auth   "),
        Span::styled(
            " [?] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(139, 233, 253))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Help   "),
        Span::styled(
            " [q] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(255, 85, 85))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Quit "),
    ]);

    let footer_block = Paragraph::new(keys).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(98, 114, 164))),
    );

    f.render_widget(footer_block, area);
}

// Modal Centering Helper with robust clamping
fn centered_rect_fixed(width: u16, height: u16, r: Rect) -> Rect {
    let actual_width = width.min(r.width.saturating_sub(2)).max(20);
    let actual_height = height.min(r.height.saturating_sub(2)).max(6);

    let x = r.x + (r.width.saturating_sub(actual_width)) / 2;
    let y = r.y + (r.height.saturating_sub(actual_height)) / 2;

    Rect::new(x, y, actual_width, actual_height)
}

/// Helper to render an interactive text input box with borders, cursor, and text
fn render_input_box(
    f: &mut Frame,
    area: Rect,
    title: &str,
    text: &str,
    cursor_pos: usize,
    is_focused: bool,
    is_masked: bool,
) {
    let border_color = if is_focused {
        Color::Rgb(80, 250, 123)
    } else {
        Color::Rgb(98, 114, 164)
    };

    let title_color = if is_focused {
        Color::Rgb(80, 250, 123)
    } else {
        Color::Rgb(180, 180, 200)
    };

    let chars: Vec<char> = if is_masked {
        text.chars().map(|_| '*').collect()
    } else {
        text.chars().collect()
    };

    let display_spans = if is_focused {
        let cursor_idx = cursor_pos.min(chars.len());
        let mut spans = Vec::new();

        if cursor_idx > 0 {
            let left: String = chars[..cursor_idx].iter().collect();
            spans.push(Span::styled(left, Style::default().fg(Color::White)));
        }

        if cursor_idx < chars.len() {
            let cur_char = chars[cursor_idx].to_string();
            let right: String = chars[cursor_idx + 1..].iter().collect();

            // Highlight char under cursor
            spans.push(Span::styled(
                cur_char,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Rgb(80, 250, 123))
                    .add_modifier(Modifier::BOLD),
            ));

            if !right.is_empty() {
                spans.push(Span::styled(right, Style::default().fg(Color::White)));
            }
        } else {
            // Cursor at the end of the text
            spans.push(Span::styled(
                "█",
                Style::default().fg(Color::Rgb(80, 250, 123)),
            ));
        }

        spans
    } else {
        let display_str: String = chars.into_iter().collect();
        vec![Span::styled(
            display_str,
            Style::default().fg(Color::Rgb(200, 200, 220)),
        )]
    };

    let p = Paragraph::new(Line::from(display_spans))
        .style(Style::default().bg(Color::Rgb(24, 28, 42)))
        .block(
            Block::default()
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default()
                        .fg(title_color)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(p, area);

    // Position the native hardware terminal cursor
    if is_focused {
        let cursor_col =
            (area.x + 1 + cursor_pos as u16).min(area.x + area.width.saturating_sub(2));
        let cursor_row = area.y + 1;
        f.set_cursor_position((cursor_col, cursor_row));
    }
}

fn draw_auth_modal(f: &mut Frame, app: &App) {
    let area = centered_rect_fixed(72, 19, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " 🔑 Rusty VCS Authentication Layer ",
            Style::default()
                .fg(Color::Rgb(255, 215, 0))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Rgb(255, 140, 0)))
        .style(Style::default().bg(Color::Rgb(20, 24, 38)));

    f.render_widget(modal_block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2), // Subtitle / helper note
            Constraint::Length(3), // Email Field
            Constraint::Length(3), // PAT / Token Field
            Constraint::Length(3), // Server URL Field
            Constraint::Length(1), // Error message if any
            Constraint::Length(1), // Controls
        ])
        .split(area);

    let note = Paragraph::new(vec![
        Line::from(Span::styled(
            "Connect CLI to your Rusty VCS account using Personal Access Token (PAT).",
            Style::default().fg(Color::Rgb(200, 200, 220)),
        )),
        Line::from(Span::styled(
            "You can generate tokens in your web user profile settings.",
            Style::default().fg(Color::DarkGray),
        )),
    ]);
    f.render_widget(note, inner[0]);

    render_input_box(
        f,
        inner[1],
        "User Email",
        &app.auth_email,
        app.cursor_pos,
        app.auth_field == AuthField::Email,
        false,
    );

    render_input_box(
        f,
        inner[2],
        "Security Token / PAT",
        &app.auth_token,
        app.cursor_pos,
        app.auth_field == AuthField::Token,
        true,
    );

    render_input_box(
        f,
        inner[3],
        "Server URL (default: http://localhost:3000)",
        &app.auth_server,
        app.cursor_pos,
        app.auth_field == AuthField::Server,
        false,
    );

    // Error or status
    if let Some(ref err) = app.auth_error {
        let err_p = Paragraph::new(Span::styled(
            format!("✖ {}", err),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        f.render_widget(err_p, inner[4]);
    }

    // Modal Actions
    let actions = Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Next Field   "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Verify & Login   "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel / Close"),
    ]);
    let actions_p = Paragraph::new(actions).alignment(Alignment::Center);
    f.render_widget(actions_p, inner[5]);
}

fn draw_add_modal(f: &mut Frame, app: &App) {
    let area = centered_rect_fixed(62, 11, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " ➕ Stage Files (rusty add) ",
            Style::default()
                .fg(Color::Rgb(80, 250, 123))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Rgb(80, 250, 123)))
        .style(Style::default().bg(Color::Rgb(20, 24, 38)));

    f.render_widget(modal_block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Prompt instruction
            Constraint::Length(3), // Input box
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Action keys
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Enter file or directory path (use '.' to stage all changes):",
        Style::default().fg(Color::Rgb(200, 200, 220)),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Target Path",
        &app.input_path,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Stage Path   "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_commit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect_fixed(66, 11, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " 💾 Create Commit (rusty commit) ",
            Style::default()
                .fg(Color::Rgb(189, 147, 249))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Rgb(189, 147, 249)))
        .style(Style::default().bg(Color::Rgb(20, 24, 38)));

    f.render_widget(modal_block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Prompt instruction
            Constraint::Length(3), // Input box
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Action keys
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Type your commit message and press Enter:",
        Style::default().fg(Color::Rgb(200, 200, 220)),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Commit Message",
        &app.input_message,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Create Commit   "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_remote_add_modal(f: &mut Frame, app: &App) {
    let area = centered_rect_fixed(68, 14, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " 🌐 Configure Remote (rusty remote add) ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(20, 24, 38)));

    f.render_widget(modal_block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Name input
            Constraint::Length(3), // URL input
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Action keys
        ])
        .split(area);

    render_input_box(
        f,
        inner[0],
        "Remote Name (e.g. origin)",
        &app.remote_name,
        app.cursor_pos,
        app.remote_field == RemoteField::Name,
        false,
    );

    render_input_box(
        f,
        inner[1],
        "Remote URL (e.g. http://localhost:3000)",
        &app.remote_url,
        app.cursor_pos,
        app.remote_field == RemoteField::Url,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Switch Field   "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Save Remote   "),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_help_modal(f: &mut Frame) {
    let area = centered_rect_fixed(68, 18, f.area());
    f.render_widget(Clear, area);

    let modal_block = Block::default()
        .title(Span::styled(
            " ❓ Rusty VCS TUI Help & Cheatsheet ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Rgb(20, 24, 38)));

    f.render_widget(modal_block, area);

    let help_lines = vec![
        Line::from(Span::styled(
            "Keyboard Shortcuts:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • ↑ / ↓ or k / j   : Move selection in Command Palette"),
        Line::from("  • Enter            : Run selected command"),
        Line::from("  • x                : Clear Execution Log & Activity"),
        Line::from("  • Tab              : Open / toggle Authentication modal"),
        Line::from(
            "  • s, a, c, l, p, w : Quick trigger for Status, Add, Commit, Log, Push, WriteTree",
        ),
        Line::from("  • i, r, u, o       : Quick trigger for Init, Remote, Whoami, Logout"),
        Line::from("  • ? / h            : Open this help screen"),
        Line::from("  • Esc              : Close active popup / modal"),
        Line::from("  • q / Ctrl+C       : Exit Rusty TUI"),
        Line::from(""),
        Line::from(Span::styled(
            "Text Editing in Modals:",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • Type characters freely; cursor navigates with ← / →, Home / End"),
        Line::from("  • Backspace / Delete: Erase characters; Ctrl+w / Ctrl+u: Erase word / line"),
        Line::from("  • Pasting from clipboard is supported directly into input fields"),
    ];

    let p = Paragraph::new(help_lines)
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(ratatui::widgets::Padding::uniform(1)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}
