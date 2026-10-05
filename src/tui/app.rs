use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

use crate::auth::{self, Credentials};
use crate::tui::capture::capture_stdout;
use crate::tui::state::{RepoDetails, load_repo_details};
use crate::tui::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavCategory {
    Repository,
    Branching,
    Sync,
    Account,
    Tools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    // Repository
    Dashboard,
    Status,
    Stage,
    Commit,
    Log,
    // Branching
    Branches,
    Checkout,
    Merge,
    // Sync
    Fetch,
    Pull,
    Push,
    Remote,
    // Account
    Whoami,
    Logout,
    // Tools
    Rm,
    Help,
    Quit,
}

#[derive(Debug, Clone)]
pub struct NavItem {
    pub section: NavSection,
    pub category: NavCategory,
    pub name: &'static str,
    pub icon: &'static str,
    pub shortcut: &'static str,
}

pub const NAV_ITEMS: &[NavItem] = &[
    // Repository
    NavItem {
        section: NavSection::Dashboard,
        category: NavCategory::Repository,
        name: "Dashboard",
        icon: "*",
        shortcut: "1",
    },
    NavItem {
        section: NavSection::Status,
        category: NavCategory::Repository,
        name: "Status",
        icon: "~",
        shortcut: "s",
    },
    NavItem {
        section: NavSection::Stage,
        category: NavCategory::Repository,
        name: "Add / Stage",
        icon: "+",
        shortcut: "a",
    },
    NavItem {
        section: NavSection::Commit,
        category: NavCategory::Repository,
        name: "Commit",
        icon: "@",
        shortcut: "c",
    },
    NavItem {
        section: NavSection::Log,
        category: NavCategory::Repository,
        name: "Log",
        icon: "#",
        shortcut: "l",
    },
    // Branching
    NavItem {
        section: NavSection::Branches,
        category: NavCategory::Branching,
        name: "Branches",
        icon: "Y",
        shortcut: "b",
    },
    NavItem {
        section: NavSection::Checkout,
        category: NavCategory::Branching,
        name: "Checkout",
        icon: ">",
        shortcut: "o",
    },
    NavItem {
        section: NavSection::Merge,
        category: NavCategory::Branching,
        name: "Merge",
        icon: "%",
        shortcut: "m",
    },
    // Sync
    NavItem {
        section: NavSection::Fetch,
        category: NavCategory::Sync,
        name: "Fetch",
        icon: "v",
        shortcut: "f",
    },
    NavItem {
        section: NavSection::Pull,
        category: NavCategory::Sync,
        name: "Pull",
        icon: "V",
        shortcut: "u",
    },
    NavItem {
        section: NavSection::Push,
        category: NavCategory::Sync,
        name: "Push",
        icon: "^",
        shortcut: "p",
    },
    NavItem {
        section: NavSection::Remote,
        category: NavCategory::Sync,
        name: "Remote",
        icon: "$",
        shortcut: "r",
    },
    // Account
    NavItem {
        section: NavSection::Whoami,
        category: NavCategory::Account,
        name: "Whoami",
        icon: "&",
        shortcut: "w",
    },
    NavItem {
        section: NavSection::Logout,
        category: NavCategory::Account,
        name: "Logout",
        icon: "X",
        shortcut: "O",
    },
    // Tools
    NavItem {
        section: NavSection::Rm,
        category: NavCategory::Tools,
        name: "Rm (Cached)",
        icon: "-",
        shortcut: "R",
    },
    NavItem {
        section: NavSection::Help,
        category: NavCategory::Tools,
        name: "Help",
        icon: "?",
        shortcut: "?",
    },
    NavItem {
        section: NavSection::Quit,
        category: NavCategory::Tools,
        name: "Quit",
        icon: "Q",
        shortcut: "q",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    AuthLogin,
    PromptAdd,
    PromptCommit,
    PromptBranch,
    PromptCheckout,
    PromptMerge,
    PromptRemoteAdd,
    PromptRm,
    ConfirmAbortMerge,
    ConfirmLogout,
    ConfirmInit,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthField {
    Email,
    Token,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteField {
    Name,
    Url,
}

#[derive(Debug, Clone)]
pub struct OutputLine {
    pub text: String,
    pub kind: OutputKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Command,
    Info,
    Success,
    Warning,
    Error,
    Plain,
}

pub struct App {
    pub repo: RepoDetails,
    pub creds: Option<Credentials>,

    pub selected_nav: usize,
    pub current_view: NavSection,

    // Command output history
    pub output_lines: Vec<OutputLine>,
    pub output_scroll: usize,

    // Modal state
    pub active_modal: ActiveModal,
    pub cursor_pos: usize,

    // Text inputs
    pub auth_email: String,
    pub auth_token: String,
    pub auth_server: String,
    pub auth_field: AuthField,
    pub auth_error: Option<String>,

    pub input_path: String,
    pub input_message: String,
    pub input_branch: String,
    pub input_checkout: String,
    pub input_merge: String,
    pub input_rm_path: String,

    pub remote_name: String,
    pub remote_url: String,
    pub remote_field: RemoteField,

    // Selections in sub-views
    pub selected_branch_idx: usize,
    pub selected_commit_idx: usize,
    pub selected_conflict_idx: usize,

    // Mouse click hitboxes (populated dynamically during draw)
    pub hitboxes: Vec<(Rect, HitAction)>,

    pub should_quit: bool,
    pub status_message: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum HitAction {
    Nav(usize),
    QuickAction(NavSection),
    ClearOutput,
    ViewAllCommits,
    SelectBranch(usize),
    SelectCommit(usize),
    SelectConflict(usize),
    ModalSubmit,
    ModalCancel,
}

impl App {
    pub fn new() -> Self {
        let creds = auth::load_credentials().unwrap_or(None);
        let repo = load_repo_details();

        let mut app = Self {
            repo,
            creds,
            selected_nav: 0,
            current_view: NavSection::Dashboard,
            output_lines: Vec::new(),
            output_scroll: 0,
            active_modal: ActiveModal::None,
            cursor_pos: 0,

            auth_email: String::new(),
            auth_token: String::new(),
            auth_server: "http://localhost:3000".to_string(),
            auth_field: AuthField::Email,
            auth_error: None,

            input_path: ".".to_string(),
            input_message: String::new(),
            input_branch: String::new(),
            input_checkout: String::new(),
            input_merge: String::new(),
            input_rm_path: String::new(),

            remote_name: "origin".to_string(),
            remote_url: "http://localhost:3000".to_string(),
            remote_field: RemoteField::Name,

            selected_branch_idx: 0,
            selected_commit_idx: 0,
            selected_conflict_idx: 0,

            hitboxes: Vec::new(),
            should_quit: false,
            status_message: "All systems ready".to_string(),
        };

        // Initial greeting and status output matching the reference dashboard
        app.log_cmd("rusty status");
        if app.repo.is_initialized {
            app.log_plain(&format!("On branch {}", app.repo.current_branch));
            if app.repo.working_tree.is_clean() {
                app.log_success("nothing to commit, working tree clean");
            } else {
                if app.repo.working_tree.staged_count() > 0 {
                    app.log_info("Changes to be committed:");
                    let staged_new = app.repo.working_tree.staged_new.clone();
                    for f in staged_new {
                        app.log_success(&format!("  new file:   {}", f));
                    }
                    let staged_modified = app.repo.working_tree.staged_modified.clone();
                    for f in staged_modified {
                        app.log_success(&format!("  modified:   {}", f));
                    }
                    let staged_deleted = app.repo.working_tree.staged_deleted.clone();
                    for f in staged_deleted {
                        app.log_success(&format!("  deleted:    {}", f));
                    }
                }
                if app.repo.working_tree.unstaged_count() > 0 {
                    app.log_info("Changes not staged for commit:");
                    let unstaged_modified = app.repo.working_tree.unstaged_modified.clone();
                    for f in unstaged_modified {
                        app.log_warn(&format!("  modified:   {}", f));
                    }
                    let unstaged_deleted = app.repo.working_tree.unstaged_deleted.clone();
                    for f in unstaged_deleted {
                        app.log_warn(&format!("  deleted:    {}", f));
                    }
                }
                if !app.repo.working_tree.untracked.is_empty() {
                    app.log_info("Untracked files:");
                    let untracked = app.repo.working_tree.untracked.clone();
                    for f in untracked {
                        app.log_warn(&format!("  {}", f));
                    }
                }
            }
        } else {
            app.log_warn("No .rusty repository detected. Press [I] to initialize.");
        }

        app
    }

    pub fn refresh_state(&mut self) {
        self.repo = load_repo_details();
        self.creds = auth::load_credentials().unwrap_or(None);
    }

    // Output logging methods
    pub fn log_cmd(&mut self, cmd: &str) {
        self.output_lines.push(OutputLine {
            text: format!("$ {}", cmd),
            kind: OutputKind::Command,
        });
        self.trim_output();
    }

    pub fn log_plain(&mut self, msg: &str) {
        for line in msg.lines() {
            self.output_lines.push(OutputLine {
                text: line.to_string(),
                kind: OutputKind::Plain,
            });
        }
        self.trim_output();
    }

    pub fn log_info(&mut self, msg: &str) {
        for line in msg.lines() {
            self.output_lines.push(OutputLine {
                text: line.to_string(),
                kind: OutputKind::Info,
            });
        }
        self.trim_output();
    }

    pub fn log_success(&mut self, msg: &str) {
        for line in msg.lines() {
            self.output_lines.push(OutputLine {
                text: line.to_string(),
                kind: OutputKind::Success,
            });
        }
        self.trim_output();
    }

    pub fn log_warn(&mut self, msg: &str) {
        for line in msg.lines() {
            self.output_lines.push(OutputLine {
                text: line.to_string(),
                kind: OutputKind::Warning,
            });
        }
        self.trim_output();
    }

    pub fn log_error(&mut self, msg: &str) {
        for line in msg.lines() {
            self.output_lines.push(OutputLine {
                text: line.to_string(),
                kind: OutputKind::Error,
            });
        }
        self.trim_output();
    }

    pub fn clear_output(&mut self) {
        self.output_lines.clear();
        self.output_scroll = 0;
        self.log_plain("$");
    }

    fn trim_output(&mut self) {
        if self.output_lines.len() > 500 {
            let drain = self.output_lines.len() - 500;
            self.output_lines.drain(0..drain);
        }
    }

    // Navigation
    pub fn next_nav(&mut self) {
        if self.selected_nav + 1 < NAV_ITEMS.len() {
            self.selected_nav += 1;
        } else {
            self.selected_nav = 0;
        }
        self.current_view = NAV_ITEMS[self.selected_nav].section;
    }

    pub fn prev_nav(&mut self) {
        if self.selected_nav > 0 {
            self.selected_nav -= 1;
        } else {
            self.selected_nav = NAV_ITEMS.len() - 1;
        }
        self.current_view = NAV_ITEMS[self.selected_nav].section;
    }

    pub fn select_nav_section(&mut self, section: NavSection) {
        if let Some(idx) = NAV_ITEMS.iter().position(|item| item.section == section) {
            self.selected_nav = idx;
            self.current_view = section;
        }
    }

    pub fn trigger_action(&mut self, section: NavSection) {
        match section {
            NavSection::Dashboard => {
                self.select_nav_section(NavSection::Dashboard);
            }
            NavSection::Status => {
                self.run_status();
                self.select_nav_section(NavSection::Status);
            }
            NavSection::Stage => {
                self.input_path = ".".to_string();
                self.cursor_pos = self.input_path.chars().count();
                self.active_modal = ActiveModal::PromptAdd;
            }
            NavSection::Commit => {
                self.input_message.clear();
                self.cursor_pos = 0;
                self.active_modal = ActiveModal::PromptCommit;
            }
            NavSection::Log => {
                self.refresh_state();
                self.select_nav_section(NavSection::Log);
            }
            NavSection::Branches => {
                self.refresh_state();
                self.select_nav_section(NavSection::Branches);
            }
            NavSection::Checkout => {
                self.input_checkout.clear();
                self.cursor_pos = 0;
                self.active_modal = ActiveModal::PromptCheckout;
            }
            NavSection::Merge => {
                if self.repo.working_tree.is_merging {
                    self.select_nav_section(NavSection::Merge);
                } else {
                    self.input_merge.clear();
                    self.cursor_pos = 0;
                    self.active_modal = ActiveModal::PromptMerge;
                }
            }
            NavSection::Fetch => {
                self.run_fetch();
            }
            NavSection::Pull => {
                self.run_pull();
            }
            NavSection::Push => {
                self.run_push();
            }
            NavSection::Remote => {
                self.remote_name = "origin".to_string();
                self.remote_url = "http://localhost:3000".to_string();
                self.remote_field = RemoteField::Name;
                self.cursor_pos = self.remote_name.chars().count();
                self.active_modal = ActiveModal::PromptRemoteAdd;
            }
            NavSection::Whoami => {
                self.run_whoami();
            }
            NavSection::Logout => {
                self.active_modal = ActiveModal::ConfirmLogout;
            }
            NavSection::Rm => {
                self.input_rm_path.clear();
                self.cursor_pos = 0;
                self.active_modal = ActiveModal::PromptRm;
            }
            NavSection::Help => {
                self.active_modal = ActiveModal::Help;
            }
            NavSection::Quit => {
                self.should_quit = true;
            }
        }
    }

    // Backend Execution Handlers
    pub fn run_init(&mut self) {
        self.log_cmd("rusty init");
        let (res, out) = capture_stdout(|| crate::repository::init());
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success("Initialized empty Rusty repository in .rusty/");
                self.status_message = "Repository initialized".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Init failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_status(&mut self) {
        self.refresh_state();
        self.log_cmd("rusty status");
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'rusty init' first.");
            return;
        };

        let (res, out) = capture_stdout(|| crate::status::check_status(&repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        if let Err(e) = res {
            self.log_error(&format!("Status error: {}", e));
        }
    }

    pub fn run_add(&mut self, path: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let target = if path.trim().is_empty() {
            "."
        } else {
            path.trim()
        };
        self.log_cmd(&format!("rusty add {}", target));

        let (res, out) = capture_stdout(|| crate::add::add_file(Path::new(target), &repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Staged '{}' into index.", target));
                self.status_message = format!("Staged {}", target);
            }
            Err(e) => {
                self.log_error(&format!("Add failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_commit(&mut self, message: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        if message.trim().is_empty() {
            self.log_error("Commit message cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty commit -m \"{}\"", message.trim()));

        let msg = message.trim().to_string();
        let (res, out) = capture_stdout(|| crate::commit::create_commit(&repo_path, msg));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(hash) => {
                let short = if hash.len() >= 8 { &hash[..8] } else { &hash };
                self.log_success(&format!(
                    "[{} {}] Created commit.",
                    self.repo.current_branch, short
                ));
                self.status_message = format!("Committed {}", short);
            }
            Err(e) => {
                self.log_error(&format!("Commit failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_create_branch(&mut self, name: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let name = name.trim();
        if name.is_empty() {
            self.log_error("Branch name cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty branch {}", name));
        let (res, out) = capture_stdout(|| crate::branch::create_branch(&repo_path, name));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Created branch '{}'", name));
                self.status_message = format!("Created branch {}", name);
            }
            Err(e) => {
                self.log_error(&format!("Branch creation failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_checkout(&mut self, branch: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let branch = branch.trim();
        if branch.is_empty() {
            self.log_error("Branch name cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty checkout {}", branch));
        let (res, out) = capture_stdout(|| crate::checkout::checkout(&repo_path, branch));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Switched to branch '{}'", branch));
                self.status_message = format!("Switched to {}", branch);
            }
            Err(e) => {
                self.log_error(&format!("Checkout failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_merge(&mut self, target: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let target = target.trim();
        if target.is_empty() {
            self.log_error("Target branch cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty merge {}", target));
        let (res, out) = capture_stdout(|| crate::merge::merge(&repo_path, target));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Merged '{}' successfully.", target));
                self.status_message = format!("Merged {}", target);
            }
            Err(e) => {
                self.log_error(&format!("Merge: {}", e));
            }
        }
        self.refresh_state();
        if self.repo.working_tree.is_merging {
            self.select_nav_section(NavSection::Merge);
        }
    }

    pub fn run_merge_abort(&mut self) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        self.log_cmd("rusty merge --abort");
        let (res, out) = capture_stdout(|| crate::merge::merge_abort(&repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success("Merge aborted. Working tree restored to HEAD.");
                self.status_message = "Merge aborted".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Merge abort failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_fetch(&mut self) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        if self.creds.is_none() {
            self.log_warn("Fetch requires authentication. Please log in first.");
            self.open_login_modal();
            return;
        }

        self.log_cmd("rusty fetch");
        let (res, out) = capture_stdout(|| crate::fetch::fetch(&repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success("Fetch completed successfully.");
                self.status_message = "Fetch complete".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Fetch error: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_pull(&mut self) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        if self.creds.is_none() {
            self.log_warn("Pull requires authentication. Please log in first.");
            self.open_login_modal();
            return;
        }

        self.log_cmd("rusty pull");
        let (res, out) = capture_stdout(|| crate::pull::pull(&repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success("Pull completed successfully.");
                self.status_message = "Pull complete".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Pull error: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_push(&mut self) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        if self.creds.is_none() {
            self.log_warn("Push requires authentication. Opening login dialog...");
            self.open_login_modal();
            return;
        }

        self.log_cmd("rusty push");
        let (res, out) = capture_stdout(|| crate::push::push(&repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success("Push completed successfully.");
                self.status_message = "Push complete".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Push error: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_remote_add(&mut self, name: &str, url: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let name = name.trim();
        let url = url.trim();
        if name.is_empty() || url.is_empty() {
            self.log_error("Remote name and URL cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty remote add {} {}", name, url));
        let (res, out) = capture_stdout(|| crate::remote::add_remote(&repo_path, name, url));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Added remote '{}' -> {}", name, url));
                self.status_message = format!("Remote added: {}", name);
            }
            Err(e) => {
                self.log_error(&format!("Remote error: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_rm_cached(&mut self, path: &str) {
        self.refresh_state();
        let Some(repo_path) = self.repo.repo_path.clone() else {
            self.log_error("Not a rusty repository.");
            return;
        };

        let path = path.trim();
        if path.is_empty() {
            self.log_error("Path cannot be empty.");
            return;
        }

        self.log_cmd(&format!("rusty rm --cached {}", path));
        let (res, out) = capture_stdout(|| crate::rm::rm_cached(path, &repo_path));
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.log_success(&format!("Removed '{}' from index.", path));
                self.status_message = format!("Removed from index: {}", path);
            }
            Err(e) => {
                self.log_error(&format!("Rm failed: {}", e));
            }
        }
        self.refresh_state();
    }

    pub fn run_whoami(&mut self) {
        self.log_cmd("rusty whoami");
        match auth::load_credentials() {
            Ok(Some(creds)) => {
                self.creds = Some(creds.clone());
                self.log_info(&format!("Logged in as: {}", creds.email));
                if let Some(ref u) = creds.username {
                    self.log_info(&format!("Username:     {}", u));
                }
                self.log_info(&format!("Server:       {}", creds.server));
            }
            Ok(None) => {
                self.creds = None;
                self.log_warn("Not logged in. Press [Tab] to authenticate.");
            }
            Err(e) => {
                self.log_error(&format!("Credentials error: {}", e));
            }
        }
    }

    pub fn run_logout(&mut self) {
        self.log_cmd("rusty logout");
        // auth::logout() calls println!() internally. We must capture its stdout
        // output so it never writes raw bytes directly to the TUI's alternate screen,
        // which would corrupt the terminal display ("UI disappears" bug).
        let (res, out) = capture_stdout(|| auth::logout());
        if !out.trim().is_empty() {
            self.log_plain(out.trim());
        }
        match res {
            Ok(_) => {
                self.creds = None;
                self.log_success("Logged out successfully. Credentials removed.");
                self.status_message = "Logged out".to_string();
            }
            Err(e) => {
                self.log_error(&format!("Logout error: {}", e));
            }
        }
        // Return to Dashboard so the nav doesn't stay stuck on the Logout item.
        self.select_nav_section(NavSection::Dashboard);
    }

    pub fn open_login_modal(&mut self) {
        self.auth_field = AuthField::Email;
        self.cursor_pos = self.auth_email.chars().count();
        self.auth_error = None;
        self.active_modal = ActiveModal::AuthLogin;
    }

    pub fn submit_auth_login(&mut self) {
        let email = self.auth_email.trim().to_string();
        let token = self.auth_token.trim().to_string();
        let server = if self.auth_server.trim().is_empty() {
            "http://localhost:3000".to_string()
        } else {
            self.auth_server.trim().trim_end_matches('/').to_string()
        };

        if email.is_empty() {
            self.auth_error = Some("Email cannot be empty.".to_string());
            return;
        }

        if token.is_empty() {
            self.auth_error = Some("Token/PAT cannot be empty.".to_string());
            return;
        }

        self.log_info(&format!("Authenticating with {}...", server));

        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                self.auth_error = Some(format!("HTTP error: {}", e));
                return;
            }
        };

        let login_url = format!("{}/api/v1/auth/cli/login", server);
        let mut resp = client
            .post(&login_url)
            .json(&serde_json::json!({
                "email": email,
                "pat": token,
            }))
            .send();

        if let Ok(ref res) = resp {
            if res.status() == reqwest::StatusCode::NOT_FOUND {
                let alt_cli_url = format!("{}/api/v1/auth/cli-login", server);
                if let Ok(alt_res) = client
                    .post(&alt_cli_url)
                    .json(&serde_json::json!({
                        "email": email,
                        "pat": token,
                    }))
                    .send()
                {
                    resp = Ok(alt_res);
                }
            }
        }

        let (is_success, username) = match resp {
            Ok(res) => {
                let status = res.status();
                let text = res.text().unwrap_or_default();
                if status.is_success() {
                    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    let u = parsed["data"]["user"]["username"]
                        .as_str()
                        .map(|s| s.to_string());
                    (true, u)
                } else {
                    let err_msg =
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            parsed["message"].as_str().unwrap_or(&text).to_string()
                        } else {
                            text
                        };
                    self.auth_error = Some(format!("Rejected ({}): {}", status, err_msg));
                    (false, None)
                }
            }
            Err(err) => {
                let alt_url = format!("{}/auth/verify-token", server);
                match client
                    .post(&alt_url)
                    .json(&serde_json::json!({ "email": email, "token": token }))
                    .send()
                {
                    Ok(res) if res.status().is_success() => (true, None),
                    _ => {
                        self.auth_error = Some(format!("Connection error: {}", err));
                        (false, None)
                    }
                }
            }
        };

        if is_success {
            let creds = Credentials {
                email: email.clone(),
                token,
                server: server.clone(),
                username,
            };

            if let Ok(home) = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Home missing")) {
                let rusty_home = home.join(".rusty");
                let _ = std::fs::create_dir_all(&rusty_home);
                let _ = std::fs::write(
                    rusty_home.join("credentials.json"),
                    serde_json::to_string_pretty(&creds).unwrap_or_default(),
                );
            }

            self.creds = Some(creds);
            self.auth_error = None;
            self.active_modal = ActiveModal::None;
            self.log_success(&format!("Successfully logged in as {}", email));
            self.status_message = format!("Logged in as {}", email);
        }
    }

    pub fn get_active_input_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.active_modal {
            ActiveModal::PromptAdd => Some((&mut self.input_path, &mut self.cursor_pos)),
            ActiveModal::PromptCommit => Some((&mut self.input_message, &mut self.cursor_pos)),
            ActiveModal::PromptBranch => Some((&mut self.input_branch, &mut self.cursor_pos)),
            ActiveModal::PromptCheckout => Some((&mut self.input_checkout, &mut self.cursor_pos)),
            ActiveModal::PromptMerge => Some((&mut self.input_merge, &mut self.cursor_pos)),
            ActiveModal::PromptRm => Some((&mut self.input_rm_path, &mut self.cursor_pos)),
            ActiveModal::PromptRemoteAdd => match self.remote_field {
                RemoteField::Name => Some((&mut self.remote_name, &mut self.cursor_pos)),
                RemoteField::Url => Some((&mut self.remote_url, &mut self.cursor_pos)),
            },
            ActiveModal::AuthLogin => match self.auth_field {
                AuthField::Email => Some((&mut self.auth_email, &mut self.cursor_pos)),
                AuthField::Token => Some((&mut self.auth_token, &mut self.cursor_pos)),
                AuthField::Server => Some((&mut self.auth_server, &mut self.cursor_pos)),
            },
            _ => None,
        }
    }
}

pub fn run_tui() -> Result<()> {
    // Set up a panic hook that restores the terminal before printing the panic message.
    // This prevents the terminal from being stuck in raw/alternate-screen mode on a crash.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort terminal restore on panic
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let res = run_loop(&mut terminal, &mut app);

    // Always restore terminal, even if run_loop returned an error.
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    // Restore the original panic hook now that we're done.
    let _ = std::panic::take_hook();

    if let Err(err) = res {
        eprintln!("TUI encountered an error: {:?}", err);
    }

    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            return Ok(());
        }

        if event::poll(Duration::from_millis(50))? {
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat {
                            handle_key(app, key.code, key.modifiers);
                        }
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            handle_mouse_click(app, mouse.column, mouse.row);
                        }
                        MouseEventKind::ScrollUp => {
                            if app.output_scroll > 0 {
                                app.output_scroll = app.output_scroll.saturating_sub(1);
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            app.output_scroll = app.output_scroll.saturating_add(1);
                        }
                        _ => {}
                    },
                    Event::Paste(text) => {
                        handle_paste(app, &text);
                    }
                    _ => {}
                }
            }
        }
    }
}

fn handle_mouse_click(app: &mut App, col: u16, row: u16) {
    // Check against registered hitboxes
    for (rect, action) in app.hitboxes.clone() {
        if col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
        {
            match action {
                HitAction::Nav(idx) => {
                    if idx < NAV_ITEMS.len() {
                        app.selected_nav = idx;
                        app.current_view = NAV_ITEMS[idx].section;
                    }
                }
                HitAction::QuickAction(section) => {
                    app.trigger_action(section);
                }
                HitAction::ClearOutput => {
                    app.clear_output();
                }
                HitAction::ViewAllCommits => {
                    app.select_nav_section(NavSection::Log);
                }
                HitAction::SelectBranch(idx) => {
                    app.selected_branch_idx = idx;
                }
                HitAction::SelectCommit(idx) => {
                    app.selected_commit_idx = idx;
                }
                HitAction::SelectConflict(idx) => {
                    app.selected_conflict_idx = idx;
                }
                HitAction::ModalSubmit => {
                    submit_active_modal(app);
                }
                HitAction::ModalCancel => {
                    app.active_modal = ActiveModal::None;
                }
            }
            return;
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if modifiers.contains(KeyModifiers::CONTROL)
        && (code == KeyCode::Char('c') || code == KeyCode::Char('C'))
    {
        app.should_quit = true;
        return;
    }

    match app.active_modal {
        ActiveModal::None => handle_main_keys(app, code, modifiers),
        _ => handle_modal_keys(app, code, modifiers),
    }
}

fn handle_main_keys(app: &mut App, code: KeyCode, _modifiers: KeyModifiers) {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
        KeyCode::Up | KeyCode::Char('k') => app.prev_nav(),
        KeyCode::Down | KeyCode::Char('j') => app.next_nav(),
        KeyCode::Enter => {
            let section = NAV_ITEMS[app.selected_nav].section;
            app.trigger_action(section);
        }
        KeyCode::Tab => {
            app.open_login_modal();
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            if app.current_view == NavSection::Merge && app.repo.working_tree.is_merging {
                app.active_modal = ActiveModal::ConfirmAbortMerge;
            } else {
                app.clear_output();
            }
        }
        KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => {
            app.active_modal = ActiveModal::Help;
        }

        // Direct Quick Action Shortcuts (matching the dashboard grid)
        KeyCode::Char('s') | KeyCode::Char('S') => app.trigger_action(NavSection::Status),
        KeyCode::Char('a') | KeyCode::Char('A') => app.trigger_action(NavSection::Stage),
        KeyCode::Char('c') | KeyCode::Char('C') => app.trigger_action(NavSection::Commit),
        KeyCode::Char('l') | KeyCode::Char('L') => app.trigger_action(NavSection::Log),
        KeyCode::Char('m') | KeyCode::Char('M') => app.trigger_action(NavSection::Merge),
        KeyCode::Char('b') | KeyCode::Char('B') => {
            // If in dashboard, prompt branch modal or go to branch view
            app.input_branch.clear();
            app.cursor_pos = 0;
            app.active_modal = ActiveModal::PromptBranch;
        }
        KeyCode::Char('o') => app.trigger_action(NavSection::Checkout),
        KeyCode::Char('p') | KeyCode::Char('P') => app.trigger_action(NavSection::Push),
        KeyCode::Char('f') | KeyCode::Char('F') => app.trigger_action(NavSection::Fetch),
        KeyCode::Char('u') | KeyCode::Char('U') => app.trigger_action(NavSection::Pull),
        KeyCode::Char('r') => app.trigger_action(NavSection::Remote),
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if !app.repo.is_initialized {
                app.run_init();
            } else {
                app.active_modal = ActiveModal::ConfirmInit;
            }
        }
        KeyCode::Char('w') => app.trigger_action(NavSection::Whoami),
        KeyCode::Char('O') => app.trigger_action(NavSection::Logout),
        KeyCode::Char('R') => app.trigger_action(NavSection::Rm),

        // Number shortcuts 1-9 for navigation
        KeyCode::Char('1') => app.select_nav_section(NavSection::Dashboard),
        KeyCode::Char('2') => app.trigger_action(NavSection::Status),
        KeyCode::Char('3') => app.trigger_action(NavSection::Stage),
        KeyCode::Char('4') => app.trigger_action(NavSection::Commit),
        KeyCode::Char('5') => app.trigger_action(NavSection::Log),
        KeyCode::Char('6') => app.select_nav_section(NavSection::Branches),
        KeyCode::Char('7') => app.trigger_action(NavSection::Checkout),
        KeyCode::Char('8') => app.select_nav_section(NavSection::Merge),
        KeyCode::Char('9') => app.trigger_action(NavSection::Push),

        // Sub-view specific controls
        KeyCode::PageUp => {
            app.output_scroll = app.output_scroll.saturating_add(5);
        }
        KeyCode::PageDown => {
            app.output_scroll = app.output_scroll.saturating_sub(5);
        }
        KeyCode::Esc => {
            if app.current_view != NavSection::Dashboard {
                app.select_nav_section(NavSection::Dashboard);
            }
        }
        _ => {}
    }
}

fn handle_modal_keys(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match app.active_modal {
        ActiveModal::None => {}

        ActiveModal::Help => {
            if matches!(
                code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?')
            ) {
                app.active_modal = ActiveModal::None;
            }
        }

        ActiveModal::ConfirmAbortMerge => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                app.active_modal = ActiveModal::None;
                app.run_merge_abort();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.active_modal = ActiveModal::None;
            }
            _ => {}
        },

        ActiveModal::ConfirmLogout => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                app.active_modal = ActiveModal::None;
                app.run_logout();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.active_modal = ActiveModal::None;
            }
            _ => {}
        },

        ActiveModal::ConfirmInit => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                app.active_modal = ActiveModal::None;
                app.run_init();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                app.active_modal = ActiveModal::None;
            }
            _ => {}
        },

        ActiveModal::AuthLogin => match code {
            KeyCode::Esc => {
                app.active_modal = ActiveModal::None;
                app.auth_error = None;
            }
            KeyCode::Tab | KeyCode::Down => {
                app.auth_field = match app.auth_field {
                    AuthField::Email => AuthField::Token,
                    AuthField::Token => AuthField::Server,
                    AuthField::Server => AuthField::Email,
                };
                app.cursor_pos = match app.auth_field {
                    AuthField::Email => app.auth_email.chars().count(),
                    AuthField::Token => app.auth_token.chars().count(),
                    AuthField::Server => app.auth_server.chars().count(),
                };
            }
            KeyCode::BackTab | KeyCode::Up => {
                app.auth_field = match app.auth_field {
                    AuthField::Email => AuthField::Server,
                    AuthField::Token => AuthField::Email,
                    AuthField::Server => AuthField::Token,
                };
                app.cursor_pos = match app.auth_field {
                    AuthField::Email => app.auth_email.chars().count(),
                    AuthField::Token => app.auth_token.chars().count(),
                    AuthField::Server => app.auth_server.chars().count(),
                };
            }
            KeyCode::Enter => {
                app.submit_auth_login();
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptRemoteAdd => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Tab | KeyCode::Down => {
                app.remote_field = match app.remote_field {
                    RemoteField::Name => RemoteField::Url,
                    RemoteField::Url => RemoteField::Name,
                };
                app.cursor_pos = match app.remote_field {
                    RemoteField::Name => app.remote_name.chars().count(),
                    RemoteField::Url => app.remote_url.chars().count(),
                };
            }
            KeyCode::BackTab | KeyCode::Up => {
                app.remote_field = match app.remote_field {
                    RemoteField::Name => RemoteField::Url,
                    RemoteField::Url => RemoteField::Name,
                };
                app.cursor_pos = match app.remote_field {
                    RemoteField::Name => app.remote_name.chars().count(),
                    RemoteField::Url => app.remote_url.chars().count(),
                };
            }
            KeyCode::Enter => {
                let name = app.remote_name.clone();
                let url = app.remote_url.clone();
                app.active_modal = ActiveModal::None;
                app.run_remote_add(&name, &url);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptAdd => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let target = app.input_path.clone();
                app.active_modal = ActiveModal::None;
                app.run_add(&target);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptCommit => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let msg = app.input_message.clone();
                app.active_modal = ActiveModal::None;
                app.input_message.clear();
                app.cursor_pos = 0;
                app.run_commit(&msg);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptBranch => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let name = app.input_branch.clone();
                app.active_modal = ActiveModal::None;
                app.run_create_branch(&name);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptCheckout => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let branch = app.input_checkout.clone();
                app.active_modal = ActiveModal::None;
                app.run_checkout(&branch);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptMerge => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let branch = app.input_merge.clone();
                app.active_modal = ActiveModal::None;
                app.run_merge(&branch);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::PromptRm => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let path = app.input_rm_path.clone();
                app.active_modal = ActiveModal::None;
                app.run_rm_cached(&path);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },
    }
}

fn submit_active_modal(app: &mut App) {
    match app.active_modal {
        ActiveModal::AuthLogin => app.submit_auth_login(),
        ActiveModal::PromptAdd => {
            let p = app.input_path.clone();
            app.active_modal = ActiveModal::None;
            app.run_add(&p);
        }
        ActiveModal::PromptCommit => {
            let m = app.input_message.clone();
            app.active_modal = ActiveModal::None;
            app.run_commit(&m);
        }
        ActiveModal::PromptBranch => {
            let b = app.input_branch.clone();
            app.active_modal = ActiveModal::None;
            app.run_create_branch(&b);
        }
        ActiveModal::PromptCheckout => {
            let c = app.input_checkout.clone();
            app.active_modal = ActiveModal::None;
            app.run_checkout(&c);
        }
        ActiveModal::PromptMerge => {
            let m = app.input_merge.clone();
            app.active_modal = ActiveModal::None;
            app.run_merge(&m);
        }
        ActiveModal::PromptRemoteAdd => {
            let n = app.remote_name.clone();
            let u = app.remote_url.clone();
            app.active_modal = ActiveModal::None;
            app.run_remote_add(&n, &u);
        }
        ActiveModal::PromptRm => {
            let p = app.input_rm_path.clone();
            app.active_modal = ActiveModal::None;
            app.run_rm_cached(&p);
        }
        ActiveModal::ConfirmAbortMerge => {
            app.active_modal = ActiveModal::None;
            app.run_merge_abort();
        }
        ActiveModal::ConfirmLogout => {
            app.active_modal = ActiveModal::None;
            app.run_logout();
        }
        ActiveModal::ConfirmInit => {
            app.active_modal = ActiveModal::None;
            app.run_init();
        }
        _ => {
            app.active_modal = ActiveModal::None;
        }
    }
}

// Text editing helpers
fn handle_text_input(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> bool {
    let Some((text, cursor)) = app.get_active_input_mut() else {
        return false;
    };

    match code {
        KeyCode::Char(c) => {
            if modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT) {
                match c {
                    'u' | 'U' => {
                        text.clear();
                        *cursor = 0;
                        return true;
                    }
                    'w' | 'W' => {
                        delete_word_before(text, cursor);
                        return true;
                    }
                    'a' | 'A' => {
                        *cursor = 0;
                        return true;
                    }
                    'e' | 'E' => {
                        *cursor = text.chars().count();
                        return true;
                    }
                    _ => return false,
                }
            }
            insert_char(text, *cursor, c);
            *cursor += 1;
            true
        }
        KeyCode::Backspace => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                delete_word_before(text, cursor);
            } else if *cursor > 0 {
                *cursor = remove_char_before(text, *cursor);
            }
            true
        }
        KeyCode::Delete => {
            remove_char_at(text, *cursor);
            true
        }
        KeyCode::Left => {
            *cursor = cursor.saturating_sub(1);
            true
        }
        KeyCode::Right => {
            let len = text.chars().count();
            *cursor = (*cursor + 1).min(len);
            true
        }
        KeyCode::Home => {
            *cursor = 0;
            true
        }
        KeyCode::End => {
            *cursor = text.chars().count();
            true
        }
        _ => false,
    }
}

fn insert_char(s: &mut String, pos: usize, c: char) {
    let mut chars: Vec<char> = s.chars().collect();
    let idx = pos.min(chars.len());
    chars.insert(idx, c);
    *s = chars.into_iter().collect();
}

fn remove_char_before(s: &mut String, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut chars: Vec<char> = s.chars().collect();
    if pos <= chars.len() {
        chars.remove(pos - 1);
        *s = chars.into_iter().collect();
        pos - 1
    } else {
        chars.pop();
        let new_len = chars.len();
        *s = chars.into_iter().collect();
        new_len
    }
}

fn remove_char_at(s: &mut String, pos: usize) {
    let mut chars: Vec<char> = s.chars().collect();
    if pos < chars.len() {
        chars.remove(pos);
        *s = chars.into_iter().collect();
    }
}

fn delete_word_before(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let mut chars: Vec<char> = text.chars().collect();
    let mut new_pos = *cursor;
    while new_pos > 0 && chars[new_pos - 1].is_whitespace() {
        new_pos -= 1;
    }
    while new_pos > 0 && !chars[new_pos - 1].is_whitespace() {
        new_pos -= 1;
    }
    chars.drain(new_pos..*cursor);
    *text = chars.into_iter().collect();
    *cursor = new_pos;
}

fn handle_paste(app: &mut App, pasted: &str) {
    if let Some((text, cursor)) = app.get_active_input_mut() {
        let mut chars: Vec<char> = text.chars().collect();
        let idx = (*cursor).min(chars.len());
        let added: Vec<char> = pasted
            .chars()
            .filter(|c| *c != '\n' && *c != '\r')
            .collect();
        let count = added.len();
        for (i, c) in added.into_iter().enumerate() {
            chars.insert(idx + i, c);
        }
        *text = chars.into_iter().collect();
        *cursor += count;
    }
}
