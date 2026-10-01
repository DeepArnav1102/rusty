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
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::auth::{self, Credentials};
use crate::tui::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    Status,
    Add,
    Commit,
    Log,
    Push,
    WriteTree,
    Init,
    RemoteAdd,
    Whoami,
    Logout,
    ClearLog,
    Help,
    Quit,
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub name: &'static str,
    pub shortcut: &'static str,
    pub description: &'static str,
    pub action: CommandAction,
}

pub const COMMANDS: &[CommandItem] = &[
    CommandItem {
        name: "Status",
        shortcut: "s",
        description: "Inspect working tree: list modified, untracked, and clean files against index.",
        action: CommandAction::Status,
    },
    CommandItem {
        name: "Stage / Add",
        shortcut: "a",
        description: "Stage modified or newly created files/directories into the staging index.",
        action: CommandAction::Add,
    },
    CommandItem {
        name: "Commit",
        shortcut: "c",
        description: "Package staged index into a persistent tree and record a new commit.",
        action: CommandAction::Commit,
    },
    CommandItem {
        name: "Commit Log",
        shortcut: "l",
        description: "Traverse commit tree parent graph from HEAD to print revision history.",
        action: CommandAction::Log,
    },
    CommandItem {
        name: "Push",
        shortcut: "p",
        description: "Upload local commit trees, blobs, and update remote branch reference.",
        action: CommandAction::Push,
    },
    CommandItem {
        name: "Write Tree",
        shortcut: "w",
        description: "Compile current index entries into SHA-256 tree objects and output root hash.",
        action: CommandAction::WriteTree,
    },
    CommandItem {
        name: "Init Repo",
        shortcut: "i",
        description: "Initialize an empty Rusty VCS repository structure in current directory.",
        action: CommandAction::Init,
    },
    CommandItem {
        name: "Remote Add",
        shortcut: "r",
        description: "Configure or update a remote upstream repository URL endpoint in config.",
        action: CommandAction::RemoteAdd,
    },
    CommandItem {
        name: "Whoami",
        shortcut: "u",
        description: "Display currently authenticated session user, token status, and server.",
        action: CommandAction::Whoami,
    },
    CommandItem {
        name: "Logout",
        shortcut: "o",
        description: "Clear active credentials and PAT token from ~/.rusty/credentials.json.",
        action: CommandAction::Logout,
    },
    CommandItem {
        name: "Clear Log",
        shortcut: "x",
        description: "Clear all past entries and messages from the Execution Log & Activity console.",
        action: CommandAction::ClearLog,
    },
    CommandItem {
        name: "Help",
        shortcut: "?",
        description: "Display keyboard shortcuts and navigation tips for the Rusty TUI.",
        action: CommandAction::Help,
    },
    CommandItem {
        name: "Quit",
        shortcut: "q",
        description: "Exit the Rusty interactive terminal dashboard.",
        action: CommandAction::Quit,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    AuthLogin,
    PromptAdd,
    PromptCommit,
    PromptRemoteAdd,
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

pub struct App {
    pub selected_index: usize,
    pub logs: Vec<String>,
    pub creds: Option<Credentials>,
    pub repo_detected: bool,
    pub repo_path: Option<PathBuf>,
    pub current_branch: String,
    pub active_modal: ActiveModal,

    // Cursor position within the currently active text input
    pub cursor_pos: usize,

    // Auth Modal state
    pub auth_email: String,
    pub auth_token: String,
    pub auth_server: String,
    pub auth_field: AuthField,
    pub auth_error: Option<String>,

    // Prompt Add state
    pub input_path: String,

    // Prompt Commit state
    pub input_message: String,

    // Prompt Remote Add state
    pub remote_name: String,
    pub remote_url: String,
    pub remote_field: RemoteField,

    // Execution status
    pub should_quit: bool,

    // Stores the rendered rect of the "Clear" button in the console for mouse click detection
    pub clear_btn_rect: Option<Rect>,
}

impl App {
    pub fn new() -> Self {
        let creds = auth::load_credentials().unwrap_or(None);
        let repo_path = find_repo_path().ok();
        let repo_detected = repo_path.is_some();
        let current_branch = if let Some(ref p) = repo_path {
            detect_branch(p)
        } else {
            "None".to_string()
        };

        let initial_modal = if creds.is_none() {
            ActiveModal::AuthLogin
        } else {
            ActiveModal::None
        };

        let mut app = Self {
            selected_index: 0,
            logs: Vec::new(),
            creds,
            repo_detected,
            repo_path,
            current_branch,
            active_modal: initial_modal,

            cursor_pos: 0,

            auth_email: String::new(),
            auth_token: String::new(),
            auth_server: "http://localhost:3000".to_string(),
            auth_field: AuthField::Email,
            auth_error: None,

            input_path: ".".to_string(),
            input_message: String::new(),

            remote_name: "origin".to_string(),
            remote_url: "http://localhost:3000".to_string(),
            remote_field: RemoteField::Name,

            should_quit: false,
            clear_btn_rect: None,
        };

        app.log_system("Welcome to Rusty VCS Terminal Dashboard! 🚀");
        if app.creds.is_none() {
            app.log_warn("Authentication required: Please log in using your email and PAT token.");
        } else if let Some(ref c) = app.creds {
            app.log_success(&format!("Authenticated as {} ({})", c.email, c.server));
        }

        if app.repo_detected {
            app.log_info(&format!(
                "Repository loaded. Active branch: {}",
                app.current_branch
            ));
        } else {
            app.log_warn(
                "No .rusty repository detected in current path. Use 'Init Repo' (i) to initialize.",
            );
        }

        app
    }

    pub fn refresh_repo_state(&mut self) {
        self.repo_path = find_repo_path().ok();
        self.repo_detected = self.repo_path.is_some();
        if let Some(ref p) = self.repo_path {
            self.current_branch = detect_branch(p);
        } else {
            self.current_branch = "None".to_string();
        }
    }

    pub fn log_info(&mut self, msg: &str) {
        self.logs.push(format!("ℹ [INFO] {}", msg));
        self.limit_logs();
    }

    pub fn log_success(&mut self, msg: &str) {
        self.logs.push(format!("✔ [SUCCESS] {}", msg));
        self.limit_logs();
    }

    pub fn log_warn(&mut self, msg: &str) {
        self.logs.push(format!("⚠ [WARN] {}", msg));
        self.limit_logs();
    }

    pub fn log_error(&mut self, msg: &str) {
        self.logs.push(format!("✖ [ERROR] {}", msg));
        self.limit_logs();
    }

    pub fn log_system(&mut self, msg: &str) {
        self.logs.push(format!("⚙ [SYSTEM] {}", msg));
        self.limit_logs();
    }

    pub fn clear_log(&mut self) {
        self.logs.clear();
        self.log_system("Execution log & activity cleared.");
    }

    fn limit_logs(&mut self) {
        if self.logs.len() > 300 {
            let drain_count = self.logs.len() - 300;
            self.logs.drain(0..drain_count);
        }
    }

    pub fn next_command(&mut self) {
        if self.selected_index + 1 < COMMANDS.len() {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
    }

    pub fn previous_command(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = COMMANDS.len() - 1;
        }
    }

    pub fn select_action(&mut self, action: CommandAction) {
        if let Some(pos) = COMMANDS.iter().position(|c| c.action == action) {
            self.selected_index = pos;
        }
    }

    pub fn execute_selected(&mut self) {
        let action = COMMANDS[self.selected_index].action;
        self.execute_action(action);
    }

    pub fn execute_action(&mut self, action: CommandAction) {
        match action {
            CommandAction::Quit => {
                self.should_quit = true;
            }
            CommandAction::Help => {
                self.active_modal = ActiveModal::Help;
            }
            CommandAction::Init => {
                self.run_init();
            }
            CommandAction::Status => {
                self.run_status();
            }
            CommandAction::Add => {
                if self.input_path.is_empty() {
                    self.input_path = ".".to_string();
                }
                self.cursor_pos = self.input_path.chars().count();
                self.active_modal = ActiveModal::PromptAdd;
            }
            CommandAction::Commit => {
                self.input_message.clear();
                self.cursor_pos = 0;
                self.active_modal = ActiveModal::PromptCommit;
            }
            CommandAction::Log => {
                self.run_log();
            }
            CommandAction::Push => {
                self.run_push();
            }
            CommandAction::WriteTree => {
                self.run_write_tree();
            }
            CommandAction::Whoami => {
                self.run_whoami();
            }
            CommandAction::RemoteAdd => {
                self.remote_field = RemoteField::Name;
                self.cursor_pos = self.remote_name.chars().count();
                self.active_modal = ActiveModal::PromptRemoteAdd;
            }
            CommandAction::Logout => {
                self.run_logout();
            }
            CommandAction::ClearLog => {
                self.clear_log();
            }
        }
    }

    fn run_init(&mut self) {
        self.log_info("Executing `rusty init`...");
        match crate::repository::init() {
            Ok(_) => {
                self.log_success("Repository initialized successfully in .rusty");
                self.refresh_repo_state();
            }
            Err(e) => {
                self.log_error(&format!("Init failed: {}", e));
            }
        }
    }

    fn run_status(&mut self) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        self.log_info("Checking status...");
        match capture_status(&repo_path) {
            Ok(lines) => {
                if lines.is_empty() {
                    self.log_success("Working tree is clean.");
                } else {
                    for line in lines {
                        self.log_info(&line);
                    }
                }
            }
            Err(e) => self.log_error(&format!("Status failed: {}", e)),
        }
    }

    pub fn run_add(&mut self, file: String) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        let target = if file.trim().is_empty() {
            "."
        } else {
            file.trim()
        };
        self.log_info(&format!("Staging files from '{}'...", target));

        match crate::add::add_file(Path::new(target), &repo_path) {
            Ok(_) => {
                self.log_success(&format!("Staged '{}' into index.", target));
            }
            Err(e) => {
                self.log_error(&format!("Add failed: {}", e));
            }
        }
    }

    pub fn run_commit(&mut self, msg: String) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        if msg.trim().is_empty() {
            self.log_error("Commit message cannot be empty.");
            return;
        }

        self.log_info(&format!("Creating commit: \"{}\"...", msg.trim()));
        match crate::commit::create_commit(&repo_path, msg.trim().to_string()) {
            Ok(hash) => {
                self.log_success(&format!("Committed: {}", hash));
                self.refresh_repo_state();
            }
            Err(e) => {
                self.log_error(&format!("Commit failed: {}", e));
            }
        }
    }

    fn run_log(&mut self) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        self.log_info("Fetching commit log...");
        match capture_log(&repo_path) {
            Ok(commits) => {
                if commits.is_empty() {
                    self.log_warn("No commits yet.");
                } else {
                    for c in commits {
                        self.log_info(&c);
                    }
                }
            }
            Err(e) => self.log_error(&format!("Log error: {}", e)),
        }
    }

    fn run_push(&mut self) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        if self.creds.is_none() {
            self.log_warn("Push requires authentication. Opening login dialog...");
            self.auth_field = AuthField::Email;
            self.cursor_pos = self.auth_email.chars().count();
            self.active_modal = ActiveModal::AuthLogin;
            return;
        }

        self.log_info("Pushing objects to remote repository...");
        match crate::push::push(&repo_path) {
            Ok(_) => {
                self.log_success("Push completed successfully!");
            }
            Err(e) => {
                self.log_error(&format!("Push failed: {}", e));
            }
        }
    }

    fn run_write_tree(&mut self) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        self.log_info("Writing index tree objects...");
        match crate::tree::write_tree(&repo_path) {
            Ok(hash) => {
                self.log_success(&format!("Root tree written: {}", hash));
            }
            Err(e) => {
                self.log_error(&format!("Write-tree failed: {}", e));
            }
        }
    }

    fn run_whoami(&mut self) {
        match auth::load_credentials() {
            Ok(Some(c)) => {
                self.creds = Some(c.clone());
                self.log_info(&format!("Active User:   {}", c.email));
                if let Some(ref u) = c.username {
                    self.log_info(&format!("Username:      {}", u));
                }
                self.log_info(&format!("Server:        {}", c.server));
                let masked_token = if c.token.len() > 6 {
                    format!("{}...{}", &c.token[..4], &c.token[c.token.len() - 2..])
                } else {
                    "******".to_string()
                };
                self.log_info(&format!("Token:         {}", masked_token));
            }
            Ok(None) => {
                self.creds = None;
                self.log_warn("Not logged in. Press 'u' or click login to authenticate.");
            }
            Err(e) => {
                self.log_error(&format!("Failed reading credentials: {}", e));
            }
        }
    }

    pub fn run_remote_add(&mut self, name: String, url: String) {
        self.refresh_repo_state();
        let Some(repo_path) = self.repo_path.clone() else {
            self.log_error("Not a rusty repository. Run 'Init' first.");
            return;
        };

        if name.trim().is_empty() || url.trim().is_empty() {
            self.log_error("Remote name and URL cannot be empty.");
            return;
        }

        self.log_info(&format!(
            "Adding remote '{}' -> {}",
            name.trim(),
            url.trim()
        ));
        match crate::remote::add_remote(&repo_path, name.trim(), url.trim()) {
            Ok(_) => {
                self.log_success(&format!("Added remote '{}'!", name.trim()));
            }
            Err(e) => {
                self.log_error(&format!("Failed adding remote: {}", e));
            }
        }
    }

    fn run_logout(&mut self) {
        match auth::logout() {
            Ok(_) => {
                self.creds = None;
                self.log_warn("Logged out. Credentials removed.");
            }
            Err(e) => {
                self.log_error(&format!("Logout error: {}", e));
            }
        }
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

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => {
                self.auth_error = Some(format!("HTTP client error: {}", e));
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

        // If 404, fallback to /api/v1/auth/cli-login
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
                            if let Some(msg) = parsed["message"].as_str() {
                                msg.to_string()
                            } else {
                                text
                            }
                        } else {
                            text
                        };
                    self.auth_error = Some(format!("Auth rejected ({}): {}", status, err_msg));
                    (false, None)
                }
            }
            Err(err) => {
                // Try fallback verification endpoint
                let alt_url = format!("{}/auth/verify-token", server);
                match client
                    .post(&alt_url)
                    .json(&serde_json::json!({ "email": email, "token": token }))
                    .send()
                {
                    Ok(res) if res.status().is_success() => (true, None),
                    _ => {
                        self.auth_error = Some(format!("Connection error to {}: {}", server, err));
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

            // Save credentials
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
            self.log_success(&format!("Successfully logged in as {}!", email));
        }
    }

    /// Access the active text field and cursor position for typing and editing
    pub fn get_active_input_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.active_modal {
            ActiveModal::PromptAdd => Some((&mut self.input_path, &mut self.cursor_pos)),
            ActiveModal::PromptCommit => Some((&mut self.input_message, &mut self.cursor_pos)),
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

fn insert_str(s: &mut String, pos: usize, text: &str) -> usize {
    let mut chars: Vec<char> = s.chars().collect();
    let idx = pos.min(chars.len());
    let added_chars: Vec<char> = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    let count = added_chars.len();
    for (i, c) in added_chars.into_iter().enumerate() {
        chars.insert(idx + i, c);
    }
    *s = chars.into_iter().collect();
    pos + count
}

fn handle_text_input(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> bool {
    let Some((text, cursor)) = app.get_active_input_mut() else {
        return false;
    };

    match code {
        KeyCode::Char(c) => {
            // Only intercept genuine CTRL combos (not SHIFT which is used for uppercase)
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
                        // Ctrl+A: move to beginning
                        *cursor = 0;
                        return true;
                    }
                    'e' | 'E' => {
                        // Ctrl+E: move to end
                        *cursor = text.chars().count();
                        return true;
                    }
                    _ => {
                        // Don't eat other ctrl combos — they might be terminal sequences
                        return false;
                    }
                }
            }
            // Insert the character (works for plain chars and SHIFT+char for uppercase)
            insert_char(text, *cursor, c);
            *cursor += 1;
            true
        }
        KeyCode::Backspace => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Backspace: delete word
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
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Left: jump word left
                let chars: Vec<char> = text.chars().collect();
                let mut pos = *cursor;
                while pos > 0 && chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                while pos > 0 && !chars[pos - 1].is_whitespace() {
                    pos -= 1;
                }
                *cursor = pos;
            } else {
                *cursor = cursor.saturating_sub(1);
            }
            true
        }
        KeyCode::Right => {
            let len = text.chars().count();
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Right: jump word right
                let chars: Vec<char> = text.chars().collect();
                let mut pos = *cursor;
                while pos < len && chars[pos].is_whitespace() {
                    pos += 1;
                }
                while pos < len && !chars[pos].is_whitespace() {
                    pos += 1;
                }
                *cursor = pos;
            } else {
                *cursor = (*cursor + 1).min(len);
            }
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

fn handle_paste(app: &mut App, pasted: &str) {
    if let Some((text, cursor)) = app.get_active_input_mut() {
        *cursor = insert_str(text, *cursor, pasted);
    }
}

fn find_repo_path() -> Result<PathBuf> {
    let mut current_dir = std::env::current_dir()?;

    loop {
        let repo_path = current_dir.join(".rusty");
        if repo_path.is_dir() {
            return Ok(repo_path);
        }
        if !current_dir.pop() {
            break;
        }
    }

    anyhow::bail!("Not a rusty repository");
}

fn detect_branch(repo_path: &Path) -> String {
    let head_path = repo_path.join("HEAD");
    if let Ok(head) = std::fs::read_to_string(head_path) {
        if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
            return branch.trim().to_string();
        } else if let Some(branch) = head.strip_prefix("ref: ") {
            return branch.trim().to_string();
        }
    }
    "main".to_string()
}

fn capture_status(repo_path: &Path) -> Result<Vec<String>> {
    use crate::index::Index;
    use crate::objects;
    use walkdir::WalkDir;

    let repo_root = repo_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid repo path"))?;
    let index = Index::load(repo_path)?;
    let mut results = Vec::new();
    let mut found_any = false;

    for entry in WalkDir::new(repo_root) {
        let entry = entry?;
        let path = entry.path();

        if path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == ".rusty" || s == ".git" || s == "target"
        }) {
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative_path = path
            .strip_prefix(repo_root)?
            .to_string_lossy()
            .replace('\\', "/");

        if let Some(index_entry) = index.entries.get(&relative_path) {
            let curr_hash = objects::hash_file(path)?;
            if curr_hash != index_entry.blob_hash {
                results.push(format!("📝 Modified:  {}", relative_path));
                found_any = true;
            }
        } else {
            results.push(format!("❓ Untracked: {}", relative_path));
            found_any = true;
        }
    }

    if !found_any {
        results.push("✨ Working directory clean, nothing modified.".to_string());
    }

    Ok(results)
}

fn capture_log(repo_path: &Path) -> Result<Vec<String>> {
    use crate::commit::get_commit;

    let head_path = repo_path.join("HEAD");
    let head = std::fs::read_to_string(head_path)?;
    let branch = head.strip_prefix("ref: ").unwrap_or(&head).trim();
    let branch_path = repo_path.join(branch);

    if !branch_path.exists() {
        return Ok(vec!["(no commits found on active branch)".to_string()]);
    }

    let hash = std::fs::read_to_string(branch_path)?;
    let mut current_hash = hash.trim().to_string();

    if current_hash.is_empty() {
        return Ok(vec!["(empty branch reference)".to_string()]);
    }

    let mut logs = Vec::new();
    let mut count = 0;

    while !current_hash.is_empty() && count < 30 {
        let commit = match get_commit(repo_path, &current_hash) {
            Ok(commit) => commit,
            Err(_) => break,
        };

        let short_hash = if current_hash.len() >= 8 {
            &current_hash[..8]
        } else {
            &current_hash
        };

        logs.push(format!("● [{}] {}", short_hash, commit.message));

        // Follow the first parent for the normal linear log view.
        // Merge commits can have multiple parents; parents[0] is the
        // current branch's history, while the other parent is the merged branch.
        match commit.parents.first() {
            Some(parent_hash) => {
                current_hash = parent_hash.clone();
            }
            None => {
                break;
            }
        }

        count += 1;
    }

    Ok(logs)
}

pub fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

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
                    Event::Mouse(mouse) => {
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                            handle_mouse_click(app, mouse.column, mouse.row);
                        }
                    }
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
    // Check if the clear button was clicked
    if let Some(btn_rect) = app.clear_btn_rect {
        if col >= btn_rect.x
            && col < btn_rect.x + btn_rect.width
            && row >= btn_rect.y
            && row < btn_rect.y + btn_rect.height
        {
            app.clear_log();
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
        ActiveModal::None => match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => app.previous_command(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => app.next_command(),
            KeyCode::Enter => app.execute_selected(),
            KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => {
                app.active_modal = ActiveModal::Help
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.select_action(CommandAction::Status);
                app.execute_action(CommandAction::Status);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                app.select_action(CommandAction::Add);
                app.execute_action(CommandAction::Add);
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                app.select_action(CommandAction::Commit);
                app.execute_action(CommandAction::Commit);
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.select_action(CommandAction::Log);
                app.execute_action(CommandAction::Log);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.select_action(CommandAction::Push);
                app.execute_action(CommandAction::Push);
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                app.select_action(CommandAction::WriteTree);
                app.execute_action(CommandAction::WriteTree);
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                app.select_action(CommandAction::Init);
                app.execute_action(CommandAction::Init);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.select_action(CommandAction::RemoteAdd);
                app.execute_action(CommandAction::RemoteAdd);
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                app.select_action(CommandAction::Whoami);
                app.execute_action(CommandAction::Whoami);
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                app.select_action(CommandAction::Logout);
                app.execute_action(CommandAction::Logout);
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                app.select_action(CommandAction::ClearLog);
                app.execute_action(CommandAction::ClearLog);
            }
            KeyCode::Tab => {
                app.auth_field = AuthField::Email;
                app.cursor_pos = app.auth_email.chars().count();
                app.active_modal = ActiveModal::AuthLogin;
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

        ActiveModal::PromptAdd => match code {
            KeyCode::Esc => app.active_modal = ActiveModal::None,
            KeyCode::Enter => {
                let target = app.input_path.clone();
                app.active_modal = ActiveModal::None;
                app.run_add(target);
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
                app.run_commit(msg);
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
                app.run_remote_add(name, url);
            }
            _ => {
                handle_text_input(app, code, modifiers);
            }
        },

        ActiveModal::Help => match code {
            KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Char('q')
            | KeyCode::Char('?')
            | KeyCode::Char('h') => {
                app.active_modal = ActiveModal::None;
            }
            _ => {}
        },
    }
}
