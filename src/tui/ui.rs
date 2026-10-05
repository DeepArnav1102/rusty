use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use crate::tui::app::{
    ActiveModal, App, AuthField, HitAction, NAV_ITEMS, NavCategory, NavSection, OutputKind,
    RemoteField,
};
use crate::tui::theme::Theme;

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hitboxes.clear();
    let size = f.area();

    // Background base
    let base_block = Block::default().style(Style::default().bg(Theme::BG_ROOT));
    f.render_widget(base_block, size);

    if size.width < 60 || size.height < 18 {
        draw_too_small(f, size);
        return;
    }

    // Root layout: Header, Main Area (Sidebar + Body), Footer
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(12),   // Body
            Constraint::Length(1), // Footer
        ])
        .split(size);

    draw_header(f, app, root_chunks[0]);
    draw_main(f, app, root_chunks[1]);
    draw_footer(f, app, root_chunks[2]);

    // Modals
    match app.active_modal {
        ActiveModal::AuthLogin => draw_auth_modal(f, app),
        ActiveModal::PromptAdd => draw_add_modal(f, app),
        ActiveModal::PromptCommit => draw_commit_modal(f, app),
        ActiveModal::PromptBranch => draw_branch_modal(f, app),
        ActiveModal::PromptCheckout => draw_checkout_modal(f, app),
        ActiveModal::PromptMerge => draw_merge_modal(f, app),
        ActiveModal::PromptRemoteAdd => draw_remote_add_modal(f, app),
        ActiveModal::PromptRm => draw_rm_modal(f, app),
        ActiveModal::ConfirmAbortMerge => draw_confirm_abort_modal(f, app),
        ActiveModal::ConfirmLogout => draw_confirm_logout_modal(f, app),
        ActiveModal::ConfirmInit => draw_confirm_init_modal(f, app),
        ActiveModal::Help => draw_help_modal(f, app),
        ActiveModal::None => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HEADER
// ─────────────────────────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, app: &mut App, area: Rect) {
    let header_block = Block::default()
        .style(Style::default().bg(Theme::BG_HEADER))
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Theme::BORDER_DEFAULT));
    f.render_widget(header_block, area);

    let inner = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1);

    // Left Title
    let title_spans = vec![
        Span::styled("⚡ RUSTY", Theme::title()),
        Span::styled("  │  ", Style::default().fg(Theme::BORDER_DEFAULT)),
        Span::styled(
            "Version Control System",
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ];
    let left_width = 46.min(inner.width);
    let left_rect = Rect::new(inner.x, inner.y, left_width, 1);
    f.render_widget(Paragraph::new(Line::from(title_spans)), left_rect);

    // Right Badges & Path
    let right_width = inner.width.saturating_sub(left_width);
    if right_width > 20 {
        let right_rect = Rect::new(inner.x + left_width, inner.y, right_width, 1);
        let mut right_spans = Vec::new();

        // Repo path
        if let Some(ref path) = app.repo.work_dir {
            let path_str = path.to_string_lossy();
            let display_path = if path_str.len() > 30 {
                format!("…{}", &path_str[path_str.len() - 29..])
            } else {
                path_str.to_string()
            };
            right_spans.push(Span::styled("@ ", Style::default().fg(Theme::ACCENT_BLUE)));
            right_spans.push(Span::styled(
                format!("{}  ", display_path),
                Style::default().fg(Theme::TEXT_SECONDARY),
            ));
        }

        // Repo Status Pill
        if app.repo.is_initialized {
            right_spans.push(Span::styled(
                " + Repository ",
                Style::default()
                    .fg(Theme::ACCENT_GREEN)
                    .bg(Color::Rgb(6, 40, 30))
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            right_spans.push(Span::styled(
                " x No Repo ",
                Style::default()
                    .fg(Theme::ACCENT_RED)
                    .bg(Color::Rgb(40, 10, 15))
                    .add_modifier(Modifier::BOLD),
            ));
        }
        right_spans.push(Span::raw(" "));

        // Branch Pill
        right_spans.push(Span::styled(
            format!(" Y {} ", app.repo.current_branch),
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .bg(Color::Rgb(35, 15, 55))
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::raw(" "));

        // Merging Pill if in progress
        if app.repo.working_tree.is_merging {
            right_spans.push(Span::styled(
                " % MERGING ",
                Style::default()
                    .fg(Theme::ACCENT_YELLOW)
                    .bg(Color::Rgb(50, 35, 10))
                    .add_modifier(Modifier::BOLD),
            ));
            right_spans.push(Span::raw(" "));
        }

        // User Pill
        let user_label = if let Some(ref c) = app.creds {
            c.username.as_deref().unwrap_or(&c.email)
        } else {
            "Login"
        };
        right_spans.push(Span::styled(
            format!(" & {} v ", user_label),
            Style::default()
                .fg(Theme::TEXT_PRIMARY)
                .bg(Color::Rgb(30, 41, 59)),
        ));

        let right_p = Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right);
        f.render_widget(right_p, right_rect);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MAIN (SIDEBAR + CONTENT)
// ─────────────────────────────────────────────────────────────────────────────

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Left sidebar
            Constraint::Min(40),    // Main workspace
        ])
        .split(area);

    draw_sidebar(f, app, main_cols[0]);

    match app.current_view {
        NavSection::Dashboard => draw_dashboard(f, app, main_cols[1]),
        NavSection::Status => draw_status_screen(f, app, main_cols[1]),
        NavSection::Log => draw_log_screen(f, app, main_cols[1]),
        NavSection::Branches => draw_branches_screen(f, app, main_cols[1]),
        NavSection::Merge => draw_merge_screen(f, app, main_cols[1]),
        NavSection::Remote => draw_remote_screen(f, app, main_cols[1]),
        _ => draw_dashboard(f, app, main_cols[1]),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SIDEBAR
// ─────────────────────────────────────────────────────────────────────────────

fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(Theme::BG_SIDEBAR))
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Theme::BORDER_DEFAULT));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    let mut current_y = inner.y;
    let max_y = inner.y + inner.height;

    // Fixed layout constants (all icons are now 1-char ASCII):
    //  col 0      : 1-char icon
    //  col 1      : space
    //  col 2..end : name, right-padded; shortcut/badge flush to right edge
    let w = inner.width as usize;

    let mut current_cat: Option<NavCategory> = None;

    for (idx, item) in NAV_ITEMS.iter().enumerate() {
        if current_y >= max_y {
            break;
        }

        // Section header
        if current_cat != Some(item.category) {
            current_cat = Some(item.category);
            let cat_label = match item.category {
                NavCategory::Repository => "REPOSITORY",
                NavCategory::Branching => "BRANCHING",
                NavCategory::Sync => "SYNC",
                NavCategory::Account => "ACCOUNT",
                NavCategory::Tools => "TOOLS",
            };

            if current_y + 1 < max_y {
                let cat_line = Line::from(Span::styled(
                    cat_label,
                    Style::default()
                        .fg(Theme::TEXT_MUTED)
                        .add_modifier(Modifier::BOLD),
                ));
                f.render_widget(
                    Paragraph::new(cat_line),
                    Rect::new(inner.x, current_y, inner.width, 1),
                );
                current_y += 1;
            }
        }

        if current_y >= max_y {
            break;
        }

        let is_selected = idx == app.selected_nav;
        let row_rect = Rect::new(inner.x, current_y, inner.width, 1);
        app.hitboxes.push((row_rect, HitAction::Nav(idx)));

        let name_style = if is_selected {
            Theme::nav_active()
        } else {
            Theme::nav_inactive()
        };

        let badge = match item.section {
            NavSection::Dashboard => "1".to_string(),
            NavSection::Status => {
                let cnt = app.repo.working_tree.total_changes_count();
                if cnt > 0 {
                    cnt.to_string()
                } else {
                    String::new()
                }
            }
            NavSection::Merge if app.repo.working_tree.is_merging => "!".to_string(),
            _ => String::new(),
        };

        // Right-hand annotation: badge or "[shortcut]"
        let right_ann: String = if !badge.is_empty() {
            format!("{} ", badge)
        } else {
            format!("[{}]", item.shortcut)
        };

        // Build the row: " I name ........... [x]"
        // icon is 1 char, then space, then name, then padding, then right_ann
        let prefix = format!(" {} ", item.icon); // " I " = 3 chars
        let right_len = right_ann.chars().count();
        // name area = total width - prefix(3) - right_ann length
        let name_area = w.saturating_sub(3 + right_len);
        let name_str = item.name;
        let name_len = name_str.chars().count();
        let pad_len = name_area.saturating_sub(name_len);

        let mut spans = Vec::new();
        spans.push(Span::styled(prefix, name_style));
        spans.push(Span::styled(name_str, name_style));
        spans.push(Span::styled(" ".repeat(pad_len), name_style));

        if !badge.is_empty() {
            let badge_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Theme::BG_NAV_ACTIVE)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Theme::TEXT_SECONDARY)
                    .bg(Theme::BG_BADGE)
                    .add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(right_ann, badge_style));
        } else {
            spans.push(Span::styled(
                right_ann,
                Style::default().fg(Theme::TEXT_MUTED),
            ));
        }

        f.render_widget(Paragraph::new(Line::from(spans)), row_rect);
        current_y += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DASHBOARD VIEW (MATCHING THE REFERENCE IMAGE)
// ─────────────────────────────────────────────────────────────────────────────

fn draw_dashboard(f: &mut Frame, app: &mut App, area: Rect) {
    let dashboard_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(58), // Left center (Welcome + Quick Actions + Output)
            Constraint::Percentage(42), // Right sidebar (Repo Info + Working Directory + Recent Commits)
        ])
        .split(area);

    draw_dashboard_center(f, app, dashboard_cols[0]);
    draw_dashboard_right(f, app, dashboard_cols[1]);
}

fn draw_dashboard_center(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Welcome Banner
            Constraint::Min(8),     // Command Output (expanded)
        ])
        .split(area);

    draw_welcome_banner(f, rows[0]);
    draw_command_output(f, app, rows[1]);
}

fn draw_welcome_banner(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    let available_h = inner.height as usize;
    let available_w = inner.width as usize;

    let mut lines: Vec<Line> = Vec::new();

    // Responsive design based on available width and height
    if available_w >= 36 && available_h >= 7 {
        // Top subtitle / welcome tag
        lines.push(Line::from(vec![
            Span::styled("◈  ", Style::default().fg(Theme::ACCENT_CYAN)),
            Span::styled(
                "W E L C O M E   T O",
                Style::default()
                    .fg(Theme::TEXT_SECONDARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ◈", Style::default().fg(Theme::ACCENT_CYAN)),
        ]));

        // Slant font for RUSTY with smooth warm orange gradient
        let slant_art = [
            (
                r"    ____  __  _______ ______  __  ",
                Color::Rgb(255, 140, 30),
            ),
            (
                r"   / __ \/ / / / ___//_  __/\ \/ /",
                Color::Rgb(252, 120, 25),
            ),
            (
                r"  / /_/ / / / /\__ \  / /    \  / ",
                Color::Rgb(248, 100, 20),
            ),
            (
                r" / _, _/ /_/ /___/ / / /     / /  ",
                Color::Rgb(242, 80, 20),
            ),
            (
                r"/_/ |_|\____//____/ /_/     /_/   ",
                Color::Rgb(235, 60, 25),
            ),
        ];

        for (art_line, color) in slant_art {
            lines.push(Line::from(Span::styled(
                art_line,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }

        // Subtitle pill
        if available_h >= 8 {
            lines.push(Line::from(vec![
                Span::styled(
                    "⚡  V E R S I O N   C O N T R O L   S Y S T E M  ⚡",
                    Style::default()
                        .fg(Theme::TEXT_MUTED)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    } else {
        // Compact modern block font for smaller sizes
        lines.push(Line::from(Span::styled(
            "— WELCOME TO —",
            Style::default()
                .fg(Theme::TEXT_MUTED)
                .add_modifier(Modifier::BOLD),
        )));

        let block_art = [
            (" █▀▀█  █  █  █▀▀▀█  ▀█▀  █   █ ", Color::Rgb(255, 140, 30)),
            (" █▄▄▀  █  █  ▀▀▀▄▄   █    █▄█  ", Color::Rgb(248, 100, 20)),
            (" █  █  ▀▄▄▀  █▄▄▄█   █     █   ", Color::Rgb(235, 65, 25)),
        ];

        for (art_line, color) in block_art {
            lines.push(Line::from(Span::styled(
                art_line,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
        }

        lines.push(Line::from(Span::styled(
            "Version Control System",
            Style::default()
                .fg(Theme::TEXT_SECONDARY)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        inner,
    );
}

fn draw_command_output(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " >_ Command Output ",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_CARD));
    f.render_widget(block, area);

    // Clear (x) button on the top right
    let clear_rect = Rect::new(area.x + area.width.saturating_sub(13), area.y, 11, 1);
    app.hitboxes.push((clear_rect, HitAction::ClearOutput));
    let clear_btn = Paragraph::new(Span::styled(
        "Clear (x)",
        Style::default().fg(Theme::TEXT_MUTED),
    ));
    f.render_widget(clear_btn, clear_rect);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let visible_lines = inner.height as usize;
    let total_lines = app.output_lines.len();

    let start_idx = if total_lines > visible_lines {
        (total_lines.saturating_sub(visible_lines)).saturating_sub(app.output_scroll)
    } else {
        0
    };

    let display_slice = if start_idx < total_lines {
        &app.output_lines[start_idx..]
    } else {
        &[]
    };

    let mut rendered_lines: Vec<Line> = display_slice
        .iter()
        .map(|entry| {
            let style = match entry.kind {
                OutputKind::Command => Style::default()
                    .fg(Theme::ACCENT_CYAN)
                    .add_modifier(Modifier::BOLD),
                OutputKind::Success => Style::default().fg(Theme::ACCENT_GREEN),
                OutputKind::Warning => Style::default().fg(Theme::ACCENT_YELLOW),
                OutputKind::Error => Style::default()
                    .fg(Theme::ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
                OutputKind::Info => Style::default().fg(Theme::TEXT_SECONDARY),
                OutputKind::Plain => Style::default().fg(Theme::TEXT_PRIMARY),
            };
            Line::from(Span::styled(&entry.text, style))
        })
        .collect();

    rendered_lines.push(Line::from(vec![
        Span::styled("$ ", Style::default().fg(Theme::ACCENT_CYAN)),
        Span::styled("█", Style::default().fg(Theme::ACCENT_GREEN)),
    ]));

    let p = Paragraph::new(rendered_lines).wrap(Wrap { trim: false });
    f.render_widget(p, inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// DASHBOARD RIGHT COLUMN (REPO INFO + WORKING TREE + RECENT COMMITS)
// ─────────────────────────────────────────────────────────────────────────────

fn draw_dashboard_right(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Repo Information
            Constraint::Min(8),     // Working Tree Status
            Constraint::Length(9),  // Recent Commits
        ])
        .split(area);

    draw_repo_info_card(f, app, rows[0]);
    draw_working_tree_card(f, app, rows[1]);
    draw_recent_commits_card(f, app, rows[2]);
}

fn draw_repo_info_card(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " > Repository Information ",
            Style::default()
                .fg(Theme::ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let path_str = app
        .repo
        .work_dir
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "Not found".to_string());

    let remote_str = app
        .repo
        .remote_origin
        .clone()
        .map(|u| format!("origin ({})", u))
        .unwrap_or_else(|| "None configured".to_string());

    let user_str = if let Some(ref c) = app.creds {
        format!("{} ({})", c.username.as_deref().unwrap_or(""), c.email)
    } else {
        "Not logged in".to_string()
    };

    let last_commit_str = if let Some(ref c) = app.repo.last_commit {
        format!("{} \"{}\"", c.short_hash, c.message)
    } else {
        "None".to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Path:         ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(path_str, Style::default().fg(Theme::TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("Branch:       ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                if app.repo.is_detached {
                    format!("Y {} (detached)", app.repo.current_branch)
                } else {
                    format!("Y {}", app.repo.current_branch)
                },
                Style::default().fg(Theme::ACCENT_PURPLE),
            ),
        ]),
        Line::from(vec![
            Span::styled("Repository:   ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                if app.repo.is_initialized {
                    "Initialised [ok]"
                } else {
                    "Uninitialised [--]"
                },
                Style::default().fg(if app.repo.is_initialized {
                    Theme::ACCENT_GREEN
                } else {
                    Theme::ACCENT_RED
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("Remote:       ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(remote_str, Style::default().fg(Theme::TEXT_SECONDARY)),
        ]),
        Line::from(vec![
            Span::styled("User:         ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(user_str, Style::default().fg(Theme::TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("Commits:      ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                format!("{}", app.repo.total_commits),
                Style::default().fg(Theme::TEXT_PRIMARY),
            ),
            Span::styled("   Unpushed: ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                &app.repo.unpushed_status,
                Style::default().fg(Theme::ACCENT_YELLOW),
            ),
        ]),
        Line::from(vec![
            Span::styled("Last Commit:  ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(last_commit_str, Style::default().fg(Theme::ACCENT_CYAN)),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_working_tree_card(f: &mut Frame, app: &App, area: Rect) {
    let total_changes = app.repo.working_tree.total_changes_count();
    let badge_text = format!(" {} ", total_changes);

    let title_line = Line::from(vec![
        Span::styled(
            " ~ Working Directory Status ",
            Style::default()
                .fg(Theme::ACCENT_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            badge_text,
            Style::default()
                .fg(Theme::TEXT_PRIMARY)
                .bg(Theme::BG_BADGE)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .title(title_line)
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();

    if app.repo.working_tree.is_clean() {
        lines.push(Line::from(Span::styled(
            "* Working directory clean, nothing modified.",
            Style::default().fg(Theme::ACCENT_GREEN),
        )));
    } else {
        // Merge conflicts if any
        if !app.repo.working_tree.conflicts.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(
                    "! Unresolved conflicts ({})",
                    app.repo.working_tree.conflicts.len()
                ),
                Style::default()
                    .fg(Theme::ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
            )));
            for c in &app.repo.working_tree.conflicts {
                lines.push(Line::from(vec![
                    Span::styled("   L- ! ", Style::default().fg(Theme::ACCENT_RED)),
                    Span::styled(
                        format!("{}: {}", c.kind, c.path),
                        Style::default().fg(Theme::TEXT_PRIMARY),
                    ),
                ]));
            }
        }

        // Modified files
        let mod_count = app.repo.working_tree.unstaged_modified.len();
        if mod_count > 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "~ Modified files ",
                    Style::default().fg(Theme::ACCENT_YELLOW),
                ),
                Span::styled(
                    format!("({})", mod_count),
                    Style::default().fg(Theme::TEXT_MUTED),
                ),
            ]));
            for f in app.repo.working_tree.unstaged_modified.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::styled("   L- ", Style::default().fg(Theme::ACCENT_YELLOW)),
                    Span::styled(f, Style::default().fg(Theme::TEXT_PRIMARY)),
                ]));
            }
            if mod_count > 4 {
                lines.push(Line::from(Span::styled(
                    format!("      … and {} more", mod_count - 4),
                    Style::default().fg(Theme::TEXT_MUTED),
                )));
            }
        }

        // Staged files
        let staged_count = app.repo.working_tree.staged_count();
        if staged_count > 0 {
            lines.push(Line::from(vec![
                Span::styled("+  Staged files ", Style::default().fg(Theme::ACCENT_GREEN)),
                Span::styled(
                    format!("({})", staged_count),
                    Style::default().fg(Theme::TEXT_MUTED),
                ),
            ]));
            for f in app.repo.working_tree.staged_new.iter().take(3) {
                lines.push(Line::from(vec![
                    Span::styled("   L- [new] ", Style::default().fg(Theme::ACCENT_GREEN)),
                    Span::styled(f, Style::default().fg(Theme::TEXT_PRIMARY)),
                ]));
            }
            for f in app.repo.working_tree.staged_modified.iter().take(3) {
                lines.push(Line::from(vec![
                    Span::styled("   L- [mod] ", Style::default().fg(Theme::ACCENT_GREEN)),
                    Span::styled(f, Style::default().fg(Theme::TEXT_PRIMARY)),
                ]));
            }
        }

        // Untracked files
        let untracked_count = app.repo.working_tree.untracked.len();
        if untracked_count > 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    "? Untracked files ",
                    Style::default().fg(Theme::ACCENT_CYAN),
                ),
                Span::styled(
                    format!("({})", untracked_count),
                    Style::default().fg(Theme::TEXT_MUTED),
                ),
            ]));
            for f in app.repo.working_tree.untracked.iter().take(4) {
                lines.push(Line::from(vec![
                    Span::styled("   L- ", Style::default().fg(Theme::ACCENT_CYAN)),
                    Span::styled(f, Style::default().fg(Theme::TEXT_PRIMARY)),
                ]));
            }
            if untracked_count > 4 {
                lines.push(Line::from(Span::styled(
                    format!("      … and {} more", untracked_count - 4),
                    Style::default().fg(Theme::TEXT_MUTED),
                )));
            }
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_recent_commits_card(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " # Recent Commits ",
            Style::default()
                .fg(Theme::ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_CARD));
    f.render_widget(block, area);

    // "View All (L)" button on the right
    let view_all_rect = Rect::new(area.x + area.width.saturating_sub(15), area.y, 13, 1);
    app.hitboxes
        .push((view_all_rect, HitAction::ViewAllCommits));
    let view_all = Paragraph::new(Span::styled(
        "View All (L)",
        Style::default()
            .fg(Theme::ACCENT_BLUE)
            .add_modifier(Modifier::UNDERLINED),
    ));
    f.render_widget(view_all, view_all_rect);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    if app.repo.recent_commits.is_empty() {
        let p = Paragraph::new(Span::styled(
            "(no commits recorded yet)",
            Style::default().fg(Theme::TEXT_MUTED),
        ));
        f.render_widget(p, inner);
        return;
    }

    let mut lines = Vec::new();
    for commit in app.repo.recent_commits.iter().take(inner.height as usize) {
        let msg = if commit.message.len() > 22 {
            format!("{}…", &commit.message[..21])
        } else {
            commit.message.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", commit.short_hash),
                Style::default()
                    .fg(Theme::ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<24} ", msg),
                Style::default().fg(Theme::TEXT_PRIMARY),
            ),
            Span::styled(
                &commit.relative_time,
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// DEDICATED SCREENS: STATUS, LOG, BRANCHES, MERGE, REMOTE
// ─────────────────────────────────────────────────────────────────────────────

fn draw_status_screen(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " ~ Repository Working Tree Status ",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_FOCUSED));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Branch: ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled(
            &app.repo.current_branch,
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    if app.repo.working_tree.is_clean() {
        lines.push(Line::from(Span::styled(
            "* Working tree clean. Nothing to commit.",
            Style::default().fg(Theme::ACCENT_GREEN),
        )));
    } else {
        if app.repo.working_tree.staged_count() > 0 {
            lines.push(Line::from(Span::styled(
                "Changes to be committed (staged):",
                Style::default()
                    .fg(Theme::ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            )));
            for f in &app.repo.working_tree.staged_new {
                lines.push(Line::from(Span::styled(
                    format!("  + new file:   {}", f),
                    Style::default().fg(Theme::ACCENT_GREEN),
                )));
            }
            for f in &app.repo.working_tree.staged_modified {
                lines.push(Line::from(Span::styled(
                    format!("  ~ modified:   {}", f),
                    Style::default().fg(Theme::ACCENT_GREEN),
                )));
            }
            for f in &app.repo.working_tree.staged_deleted {
                lines.push(Line::from(Span::styled(
                    format!("  - deleted:    {}", f),
                    Style::default().fg(Theme::ACCENT_GREEN),
                )));
            }
            lines.push(Line::from(""));
        }

        if app.repo.working_tree.unstaged_count() > 0 {
            lines.push(Line::from(Span::styled(
                "Changes not staged for commit (working tree):",
                Style::default()
                    .fg(Theme::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            )));
            for f in &app.repo.working_tree.unstaged_modified {
                lines.push(Line::from(Span::styled(
                    format!("  M modified:   {}", f),
                    Style::default().fg(Theme::ACCENT_YELLOW),
                )));
            }
            for f in &app.repo.working_tree.unstaged_deleted {
                lines.push(Line::from(Span::styled(
                    format!("  D deleted:    {}", f),
                    Style::default().fg(Theme::ACCENT_RED),
                )));
            }
            lines.push(Line::from(""));
        }

        if !app.repo.working_tree.untracked.is_empty() {
            lines.push(Line::from(Span::styled(
                "Untracked files (use 'a' to stage):",
                Style::default()
                    .fg(Theme::ACCENT_CYAN)
                    .add_modifier(Modifier::BOLD),
            )));
            for f in &app.repo.working_tree.untracked {
                lines.push(Line::from(Span::styled(
                    format!("  ? {}", f),
                    Style::default().fg(Theme::TEXT_MUTED),
                )));
            }
        }
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_log_screen(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " # Commit History Graph & DAG ",
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_FOCUSED));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    if app.repo.recent_commits.is_empty() {
        let p = Paragraph::new(Span::styled(
            "No commits found on active branch.",
            Style::default().fg(Theme::TEXT_MUTED),
        ));
        f.render_widget(p, inner);
        return;
    }

    let mut lines = Vec::new();
    for commit in &app.repo.recent_commits {
        let graph_symbol = if commit.is_merge {
            "●─┐"
        } else {
            "●  "
        };

        let badge = if commit.is_merge {
            Span::styled(
                " [MERGE] ",
                Style::default()
                    .fg(Theme::ACCENT_PURPLE)
                    .bg(Color::Rgb(40, 20, 60)),
            )
        } else {
            Span::raw(" ")
        };

        lines.push(Line::from(vec![
            Span::styled(
                graph_symbol,
                Style::default()
                    .fg(Theme::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                &commit.short_hash,
                Style::default()
                    .fg(Theme::ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ),
            badge,
            Span::styled(&commit.message, Style::default().fg(Theme::TEXT_PRIMARY)),
            Span::raw("  "),
            Span::styled(
                format!("(parents: {})", commit.parents.len()),
                Style::default().fg(Theme::TEXT_MUTED),
            ),
        ]));

        if commit.is_merge {
            lines.push(Line::from(vec![
                Span::styled("│ └─ parents: ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(
                    commit
                        .parents
                        .iter()
                        .map(|p| &p[..7.min(p.len())])
                        .collect::<Vec<&str>>()
                        .join(", "),
                    Style::default().fg(Theme::ACCENT_PURPLE),
                ),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("│    tree: ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                &commit.tree[..7.min(commit.tree.len())],
                Style::default().fg(Theme::ACCENT_CYAN),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            "│",
            Style::default().fg(Theme::BORDER_DEFAULT),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_branches_screen(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " Y Branches (Local & Remote) ",
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_FOCUSED));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Active Branch: ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled(
            &app.repo.current_branch,
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("    "),
        Span::styled("[B] New Branch", Style::default().fg(Theme::ACCENT_BLUE)),
        Span::raw("   "),
        Span::styled("[O] Checkout", Style::default().fg(Theme::ACCENT_GREEN)),
    ]));
    lines.push(Line::from(""));

    for (idx, b) in app.repo.branches.iter().enumerate() {
        let prefix = if b.is_current { " * " } else { "   " };

        let style = if b.is_current {
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD)
        } else if b.is_remote {
            Style::default().fg(Theme::ACCENT_CYAN)
        } else {
            Style::default().fg(Theme::TEXT_PRIMARY)
        };

        let hash_str = b
            .commit_hash
            .as_deref()
            .map(|h| &h[..7.min(h.len())])
            .unwrap_or("");

        let row_line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(Theme::ACCENT_GREEN)),
            Span::styled(format!("{:<24}", b.name), style),
            Span::styled(hash_str, Style::default().fg(Theme::TEXT_MUTED)),
        ]);

        lines.push(row_line);
        let click_rect = Rect::new(inner.x, inner.y + 2 + idx as u16, inner.width, 1);
        app.hitboxes
            .push((click_rect, HitAction::SelectBranch(idx)));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_merge_screen(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " % Merge & Conflict Resolution Center ",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_FOCUSED));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();

    if app.repo.working_tree.is_merging {
        lines.push(Line::from(Span::styled(
            "! MERGE IN PROGRESS",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        )));
        if let Some(ref m_head) = app.repo.working_tree.merge_head {
            lines.push(Line::from(vec![
                Span::styled("MERGE_HEAD: ", Style::default().fg(Theme::TEXT_MUTED)),
                Span::styled(
                    m_head,
                    Style::default()
                        .fg(Theme::ACCENT_PURPLE)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        lines.push(Line::from(""));

        if app.repo.working_tree.conflicts.is_empty() {
            lines.push(Line::from(Span::styled(
                "+ All merge conflicts resolved!",
                Style::default()
                    .fg(Theme::ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "Run 'rusty commit' [C] to conclude the merge.",
                Style::default().fg(Theme::TEXT_PRIMARY),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "Unmerged conflicting paths (resolve in editor and run 'rusty add'):",
                Style::default()
                    .fg(Theme::ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
            )));
            for (idx, conflict) in app.repo.working_tree.conflicts.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {}. [", idx + 1),
                        Style::default().fg(Theme::TEXT_MUTED),
                    ),
                    Span::styled(
                        format!("{}", conflict.kind),
                        Style::default().fg(Theme::ACCENT_RED),
                    ),
                    Span::styled("] ", Style::default().fg(Theme::TEXT_MUTED)),
                    Span::styled(
                        &conflict.path,
                        Style::default()
                            .fg(Theme::TEXT_PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Actions: ", Style::default().fg(Theme::TEXT_MUTED)),
            Span::styled(
                "[C] Complete Merge",
                Style::default().fg(Theme::ACCENT_GREEN),
            ),
            Span::raw("    "),
            Span::styled(
                "[X] Abort Merge",
                Style::default()
                    .fg(Theme::ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "No merge is currently in progress.",
            Style::default().fg(Theme::ACCENT_GREEN),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press [M] to start merging a branch into the current branch.",
            Style::default().fg(Theme::TEXT_PRIMARY),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_remote_screen(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " $ Remote Repositories ",
            Style::default()
                .fg(Theme::ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(Theme::BG_CARD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::BORDER_FOCUSED));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "Configured Upstream: ",
            Style::default().fg(Theme::TEXT_MUTED),
        ),
        Span::styled(
            app.repo
                .remote_origin
                .as_deref()
                .unwrap_or("No origin configured"),
            Style::default()
                .fg(Theme::ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "[R] Add/Update Remote",
            Style::default().fg(Theme::ACCENT_BLUE),
        ),
        Span::raw("   "),
        Span::styled("[F] Fetch", Style::default().fg(Theme::ACCENT_GREEN)),
        Span::raw("   "),
        Span::styled("[P] Push", Style::default().fg(Theme::ACCENT_PURPLE)),
        Span::raw("   "),
        Span::styled("[U] Pull", Style::default().fg(Theme::ACCENT_YELLOW)),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

// ─────────────────────────────────────────────────────────────────────────────
// FOOTER
// ─────────────────────────────────────────────────────────────────────────────

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let footer_splits = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),    // Key shortcuts
            Constraint::Length(32), // Status & Clock
        ])
        .split(area);

    let shortcuts = Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Theme::ACCENT_BLUE)),
        Span::styled(" Navigate  ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled("Enter", Style::default().fg(Theme::ACCENT_GREEN)),
        Span::styled(" Select  ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled("Esc", Style::default().fg(Theme::ACCENT_RED)),
        Span::styled(" Back  ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled("1-9", Style::default().fg(Theme::ACCENT_PURPLE)),
        Span::styled(" Shortcuts  ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled("x", Style::default().fg(Theme::ACCENT_YELLOW)),
        Span::styled(" Clear Output  ", Style::default().fg(Theme::TEXT_MUTED)),
        Span::styled("?", Style::default().fg(Theme::ACCENT_CYAN)),
        Span::styled(" Help", Style::default().fg(Theme::TEXT_MUTED)),
    ]);
    f.render_widget(Paragraph::new(shortcuts), footer_splits[0]);

    let right_status = Line::from(vec![
        Span::styled("● ", Style::default().fg(Theme::ACCENT_GREEN)),
        Span::styled(
            &app.status_message,
            Style::default().fg(Theme::TEXT_SECONDARY),
        ),
    ]);
    f.render_widget(
        Paragraph::new(right_status).alignment(Alignment::Right),
        footer_splits[1],
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// MODALS
// ─────────────────────────────────────────────────────────────────────────────

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let actual_width = width.min(r.width.saturating_sub(4)).max(30);
    let actual_height = height.min(r.height.saturating_sub(4)).max(7);
    let x = r.x + (r.width.saturating_sub(actual_width)) / 2;
    let y = r.y + (r.height.saturating_sub(actual_height)) / 2;
    Rect::new(x, y, actual_width, actual_height)
}

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
        Theme::ACCENT_BLUE
    } else {
        Theme::BORDER_DEFAULT
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
            spans.push(Span::styled(
                chars[..cursor_idx].iter().collect::<String>(),
                Style::default().fg(Color::White),
            ));
        }
        if cursor_idx < chars.len() {
            spans.push(Span::styled(
                chars[cursor_idx].to_string(),
                Style::default()
                    .fg(Color::Black)
                    .bg(Theme::ACCENT_BLUE)
                    .add_modifier(Modifier::BOLD),
            ));
            if cursor_idx + 1 < chars.len() {
                spans.push(Span::styled(
                    chars[cursor_idx + 1..].iter().collect::<String>(),
                    Style::default().fg(Color::White),
                ));
            }
        } else {
            spans.push(Span::styled("█", Style::default().fg(Theme::ACCENT_BLUE)));
        }
        spans
    } else {
        vec![Span::styled(
            chars.into_iter().collect::<String>(),
            Style::default().fg(Theme::TEXT_PRIMARY),
        )]
    };

    let p = Paragraph::new(Line::from(display_spans))
        .style(Style::default().bg(Theme::BG_INPUT))
        .block(
            Block::default()
                .title(Span::styled(
                    format!(" {} ", title),
                    Style::default().fg(if is_focused {
                        Theme::ACCENT_BLUE
                    } else {
                        Theme::TEXT_MUTED
                    }),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color)),
        );

    f.render_widget(p, area);

    if is_focused {
        let cursor_col =
            (area.x + 1 + cursor_pos as u16).min(area.x + area.width.saturating_sub(2));
        let cursor_row = area.y + 1;
        f.set_cursor_position((cursor_col, cursor_row));
    }
}

fn draw_commit_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(64, 12, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " @ Create Commit (rusty commit) ",
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_PURPLE))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2), // Staged files summary
            Constraint::Length(3), // Input
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Actions
        ])
        .split(area);

    let summary = Line::from(vec![
        Span::styled(
            format!("Staged files: {} ", app.repo.working_tree.staged_count()),
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "(uncommitted staged changes will be saved to HEAD)",
            Style::default().fg(Theme::TEXT_MUTED),
        ),
    ]);
    f.render_widget(Paragraph::new(summary), inner[0]);

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
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Commit   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_add_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 11, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " + Stage Files (rusty add) ",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_GREEN))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Instruction
            Constraint::Length(3), // Input
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Actions
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Enter file/dir path (use '.' to stage all changes):",
        Style::default().fg(Theme::TEXT_SECONDARY),
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
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Stage Path   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_branch_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 11, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " Y Create Branch (rusty branch) ",
            Style::default()
                .fg(Theme::ACCENT_PURPLE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_PURPLE))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Enter new branch name:",
        Style::default().fg(Theme::TEXT_SECONDARY),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Branch Name",
        &app.input_branch,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Create Branch   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_checkout_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 11, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            "> Switch Branch (rusty checkout) ",
            Style::default()
                .fg(Theme::ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_BLUE))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Enter branch name to checkout:",
        Style::default().fg(Theme::TEXT_SECONDARY),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Branch Name",
        &app.input_checkout,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Checkout   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_merge_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 11, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " % Merge Branch (rusty merge) ",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_YELLOW))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        format!("Enter branch to merge into '{}':", app.repo.current_branch),
        Style::default().fg(Theme::TEXT_SECONDARY),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Target Branch",
        &app.input_merge,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Merge   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_remote_add_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(64, 14, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " $ Configure Remote (rusty remote add) ",
            Style::default()
                .fg(Theme::ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_CYAN))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // URL
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // Actions
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
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Next Field   "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Save Remote   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_rm_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 11, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " - Rm Cached (rusty rm --cached) ",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_RED))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    let prompt = Paragraph::new(Span::styled(
        "Enter path to unstage from index (file on disk is preserved):",
        Style::default().fg(Theme::TEXT_SECONDARY),
    ));
    f.render_widget(prompt, inner[0]);

    render_input_box(
        f,
        inner[1],
        "Path",
        &app.input_rm_path,
        app.cursor_pos,
        true,
        false,
    );

    let actions = Line::from(vec![
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Remove from Index   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[3],
    );
}

fn draw_confirm_abort_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(58, 9, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " ! Confirm Abort Merge ",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_RED))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 2,
        area.width.saturating_sub(4),
        area.height.saturating_sub(4),
    );

    let text = vec![
        Line::from(Span::styled(
            "Are you sure you want to abort the current merge?",
            Style::default()
                .fg(Theme::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "All conflict resolutions will be reset to HEAD state.",
            Style::default().fg(Theme::TEXT_MUTED),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y / Enter] ",
                Style::default()
                    .fg(Theme::ACCENT_RED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Abort Merge    "),
            Span::styled("[N / Esc] ", Style::default().fg(Theme::TEXT_SECONDARY)),
            Span::raw("Keep Merging"),
        ]),
    ];

    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn draw_confirm_logout_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(54, 8, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " X Confirm Logout ",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_YELLOW))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 2,
        area.width.saturating_sub(4),
        area.height.saturating_sub(4),
    );

    let text = vec![
        Line::from(Span::styled(
            "Clear cached credentials from ~/.rusty/credentials.json?",
            Style::default().fg(Theme::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y / Enter] ",
                Style::default()
                    .fg(Theme::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Logout    "),
            Span::styled("[N / Esc] ", Style::default().fg(Theme::TEXT_SECONDARY)),
            Span::raw("Cancel"),
        ]),
    ];

    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn draw_confirm_init_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(54, 8, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " ! Reinitialize Repository? ",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_YELLOW))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 2,
        area.width.saturating_sub(4),
        area.height.saturating_sub(4),
    );

    let text = vec![
        Line::from(Span::styled(
            "A .rusty repository already exists here. Reinitialize?",
            Style::default().fg(Theme::TEXT_PRIMARY),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Y / Enter] ",
                Style::default()
                    .fg(Theme::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Reinit    "),
            Span::styled("[N / Esc] ", Style::default().fg(Theme::TEXT_SECONDARY)),
            Span::raw("Cancel"),
        ]),
    ];

    f.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn draw_auth_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(68, 17, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " # Rusty VCS Authentication Layer ",
            Style::default()
                .fg(Theme::ACCENT_ORANGE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_ORANGE))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Subtitle
            Constraint::Length(3), // Email
            Constraint::Length(3), // Token
            Constraint::Length(3), // Server
            Constraint::Length(1), // Error
            Constraint::Length(1), // Actions
        ])
        .split(area);

    let note = Paragraph::new(Span::styled(
        "Authenticate using Personal Access Token (PAT) for push, pull and fetch.",
        Style::default().fg(Theme::TEXT_MUTED),
    ));
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
        "Personal Access Token / PAT",
        &app.auth_token,
        app.cursor_pos,
        app.auth_field == AuthField::Token,
        true,
    );

    render_input_box(
        f,
        inner[3],
        "Server URL",
        &app.auth_server,
        app.cursor_pos,
        app.auth_field == AuthField::Server,
        false,
    );

    if let Some(ref err) = app.auth_error {
        let err_p = Paragraph::new(Span::styled(
            format!("x {}", err),
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ));
        f.render_widget(err_p, inner[4]);
    }

    let actions = Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Next Field   "),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Verify & Login   "),
        Span::styled(
            "[Esc]",
            Style::default()
                .fg(Theme::ACCENT_RED)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Cancel"),
    ]);
    f.render_widget(
        Paragraph::new(actions).alignment(Alignment::Center),
        inner[5],
    );
}

fn draw_help_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(72, 19, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " ? Rusty VCS Dashboard Help & Cheatsheet ",
            Style::default()
                .fg(Theme::ACCENT_CYAN)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Theme::ACCENT_CYAN))
        .style(Style::default().bg(Theme::BG_CARD));
    f.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let help_lines = vec![
        Line::from(Span::styled(
            "Navigation & Global Keys:",
            Style::default()
                .fg(Theme::ACCENT_BLUE)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • ↑ / ↓ or k / j     : Move selection in left sidebar"),
        Line::from("  • Enter              : Activate selected navigation section"),
        Line::from(
            "  • 1 - 9              : Instant jump to sections (1: Dashboard, 2: Status, etc.)",
        ),
        Line::from("  • x                  : Clear Command Output terminal"),
        Line::from("  • Tab                : Open Authentication / Login modal"),
        Line::from("  • Esc                : Return to Dashboard / Close any open modal"),
        Line::from("  • ? or h             : Open this Help cheatsheet"),
        Line::from("  • q / Ctrl+C         : Quit Rusty TUI"),
        Line::from(""),
        Line::from(Span::styled(
            "Quick Action Hotkeys (matching Dashboard grid):",
            Style::default()
                .fg(Theme::ACCENT_YELLOW)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • s: Status    a: Add/Stage   c: Commit   l: Log"),
        Line::from("  • m: Merge     b: Branch      o: Checkout p: Push"),
        Line::from("  • f: Fetch     u: Pull        r: Remote   i: Init"),
        Line::from(""),
        Line::from(Span::styled(
            "Mouse Support:",
            Style::default()
                .fg(Theme::ACCENT_GREEN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("  • Click any Sidebar item, Quick Action card, or Clear button directly"),
        Line::from("  • Mouse scroll wheel scrolls the Command Output terminal history"),
    ];

    f.render_widget(Paragraph::new(help_lines).wrap(Wrap { trim: true }), inner);
}

fn draw_too_small(f: &mut Frame, area: Rect) {
    let p = Paragraph::new(vec![
        Line::from(Span::styled("⚡ RUSTY", Theme::title())),
        Line::from(""),
        Line::from(Span::styled(
            "Terminal window is too small for the dashboard.",
            Style::default().fg(Theme::ACCENT_YELLOW),
        )),
        Line::from(Span::styled(
            "Please resize your terminal to at least 60x18.",
            Style::default().fg(Theme::TEXT_MUTED),
        )),
    ])
    .alignment(Alignment::Center);

    f.render_widget(p, area);
}
