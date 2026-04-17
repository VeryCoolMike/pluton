use std::collections::HashMap;

use chrono::{Local, TimeZone, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};

use pluton_core::account_management::Settings;
use pluton_core::networking::definitions;

/// A command the user can type.
pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

pub fn load_account_names() -> Vec<String> {
    confy::load::<Settings>("pluton", None)
        .map(|s| s.accounts.into_iter().map(|a| a.username).collect())
        .unwrap_or_default()
}

pub const COMMANDS: &[Command] = &[
    Command {
        name: "/kick",
        usage: "/kick <user> <reason>",
        description: "Kick a user from the server",
    },
];

/// A single chat message ready for display.
#[derive(Clone)]
pub struct DisplayMessage {
    pub sender: String,
    pub text: String,
    pub time: String,
}

/// Which screen the TUI is showing.
#[derive(Clone, PartialEq)]
pub enum Screen {
    Login,
    Register,
    Connecting,
    Chat,
    ServerSettings,
    UserSettings
}

/// Which login field is focused.
#[derive(Clone, PartialEq)]
pub enum LoginField {
    ServerUrl,
    Account,
    Password,
}

impl LoginField {
    pub fn next(&self) -> Self {
        match self {
            Self::ServerUrl => Self::Account,
            Self::Account => Self::Password,
            Self::Password => Self::ServerUrl,
        }
    }
}

/// Which register field is focused.
#[derive(Clone, PartialEq)]
pub enum RegisterField {
    HomeAddress,
    Username,
    Password,
}

impl RegisterField {
    pub fn next(&self) -> Self {
        match self {
            Self::HomeAddress => Self::Username,
            Self::Username => Self::Password,
            Self::Password => Self::HomeAddress,
        }
    }
}

/// The shared application state.
pub struct App {
    pub screen: Screen,

    // Login
    pub login_field: LoginField,
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub login_error: Option<String>,
    pub login_info: Option<String>,
    pub accounts: Vec<String>,
    pub selected_account: usize,

    // Register
    pub register_field: RegisterField,
    pub register_home_address: String,
    pub register_username: String,
    pub register_password: String,
    pub register_error: Option<String>,

    // Connection status
    pub status_message: Option<String>,

    // Chat state
    pub server_name: String,
    pub messages: Vec<DisplayMessage>,
    pub channels: Vec<definitions::Channel>,
    pub voice_channels: Vec<definitions::Channel>,
    pub current_channel: definitions::Channel,
    pub peers: HashMap<VerifyingKey, definitions::Peer>,
    pub input: String,
    pub input_cursor: usize,
    pub selected_channel: usize,
    pub channel_list_focused: bool,

    // Signing key (available after login)
    pub signing_key: Option<SigningKey>,
    pub current_message_id: u32,

    // Command autocomplete
    pub command_selected: usize,

    // Quit flag
    pub quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Login,
            login_field: LoginField::ServerUrl,
            server_url: String::from("ws://127.0.0.1:6767"),
            username: String::new(),
            password: String::new(),
            login_error: None,
            login_info: None,
            accounts: load_account_names(),
            selected_account: 0,
            register_field: RegisterField::HomeAddress,
            register_home_address: String::new(),
            register_username: String::new(),
            register_password: String::new(),
            register_error: None,
            status_message: None,
            server_name: String::new(),
            messages: Vec::new(),
            channels: Vec::new(),
            voice_channels: Vec::new(),
            current_channel: definitions::Channel {
                id: 0,
                name: String::from("general"),
            },
            peers: HashMap::new(),
            input: String::new(),
            input_cursor: 0,
            selected_channel: 0,
            channel_list_focused: false,
            signing_key: None,
            current_message_id: 0,
            command_selected: 0,
            quit: false,
        }
    }

    pub fn refresh_accounts(&mut self) {
        self.accounts = load_account_names();
        if self.selected_account >= self.accounts.len() {
            self.selected_account = self.accounts.len().saturating_sub(1);
        }
    }

    pub fn selected_account_name(&self) -> Option<&str> {
        self.accounts.get(self.selected_account).map(|s| s.as_str())
    }

    pub fn add_message(&mut self, sender: String, text: String, timestamp: i64) {
        let time = Utc
            .timestamp_opt(timestamp, 0)
            .single()
            .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
            .unwrap_or_default();

        self.messages.push(DisplayMessage { sender, text, time });
    }

    pub fn resolve_username(&self, key: &VerifyingKey) -> String {
        self.peers
            .get(key)
            .map(|p| p.username.clone())
            .unwrap_or_else(|| String::from("???"))
    }

    pub fn input_insert(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            let prev = self.input[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.drain(prev..self.input_cursor);
            self.input_cursor = prev;
        }
    }

    pub fn input_delete(&mut self) {
        if self.input_cursor < self.input.len() {
            let next = self.input[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input.len());
            self.input.drain(self.input_cursor..next);
        }
    }

    pub fn input_move_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn input_move_right(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input_cursor = self.input[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input.len());
        }
    }

    pub fn input_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn input_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    pub fn take_input(&mut self) -> String {
        let s = self.input.clone();
        self.input.clear();
        self.input_cursor = 0;
        s
    }

    pub fn next_message_id(&mut self) -> u32 {
        let id = self.current_message_id;
        self.current_message_id += 1;
        id
    }

    /// Returns the list of commands matching the current input, or empty if
    /// the input doesn't start with `/` or already has a space (done typing the command).
    pub fn matching_commands(&self) -> Vec<&'static Command> {
        let prefix = self.input.trim_end();
        if !prefix.starts_with('/') || prefix.contains(' ') {
            return Vec::new();
        }
        COMMANDS
            .iter()
            .filter(|cmd| cmd.name.starts_with(prefix))
            .collect()
    }

    /// Whether the command palette popup should be shown.
    pub fn show_command_palette(&self) -> bool {
        self.input.starts_with('/') && !self.input.contains(' ') && !self.matching_commands().is_empty()
    }
}
