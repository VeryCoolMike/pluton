mod app;
mod network;
mod ui;

use std::collections::HashMap;
use std::path;

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::execute;
use futures_util::{SinkExt, StreamExt};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use rfd::AsyncFileDialog;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

use ed25519_dalek::Signer;
use pluton_core::cryptography::get_signing_key;
use pluton_core::networking::definitions;
use pluton_core::{account_management, helper};
use pluton_core::helper::logging::*;

use app::{App, LoginField, RegisterField, Screen};
use network::NetEvent;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging
    set_program_name(String::from("Pluton TUI"));
    set_log_directory(default_log_dir().expect("Unable to find home directory"));
    init_logging().await?;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    let mut app = App::new();

    // Network events channel (network -> TUI)
    let (net_tx, mut net_rx) = mpsc::unbounded_channel::<NetEvent>();

    // Outgoing websocket messages (TUI -> network writer)
    let mut ws_tx: Option<mpsc::UnboundedSender<Message>> = None;

    // Crossterm event stream (async, no blocking)
    let mut event_stream = EventStream::new();

    loop {
        // Draw
        terminal.draw(|f| ui::draw(f, &app))?;
        if app.quit {
            break;
        }

        // Wait for either a terminal event or a network event
        tokio::select! {
            // Terminal input
            maybe_event = event_stream.next() => {
                let Some(Ok(Event::Key(key))) = maybe_event else { continue };
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Ctrl+C always quits
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c')
                {
                    app.quit = true;
                    continue;
                }

                match app.screen {
                    Screen::Login => {
                        // Ctrl+N switches to Register screen
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('n')
                        {
                            app.screen = Screen::Register;
                            app.login_error = None;
                            app.login_info = None;
                            continue;
                        }

                        if key.code == KeyCode::Enter {
                            if app.accounts.is_empty() {
                                app.login_error = Some(
                                    "No accounts. Press Ctrl+N to create one.".into(),
                                );
                                continue;
                            }
                            let Some(selected) = app.selected_account_name().map(str::to_string)
                            else {
                                app.login_error = Some("Select an account".into());
                                continue;
                            };
                            if app.password.is_empty() {
                                app.login_error = Some("Password required".into());
                                continue;
                            }

                            app.username = selected;
                            app.login_error = None;
                            app.login_info = None;
                            app.screen = Screen::Connecting;
                            app.status_message = Some("Signing in...".into());

                            let url = app.server_url.clone();
                            let username = app.username.clone();
                            let password = app.password.clone();

                            // Create a fresh send channel for this connection
                            let (tx, rx) = mpsc::unbounded_channel::<Message>();
                            ws_tx = Some(tx);

                            let net_tx_c = net_tx.clone();
                            tokio::spawn(async move {
                                run_network(net_tx_c, rx, url, username, password).await;
                            });
                        } else {
                            handle_login_input(&mut app, key.code);
                        }
                    }
                    Screen::Register => {
                        if key.code == KeyCode::Esc {
                            app.screen = Screen::Login;
                            app.register_error = None;
                            continue;
                        }
                        if key.code == KeyCode::Enter {
                            if app.register_home_address.is_empty() {
                                app.register_error = Some("Home address required".into());
                                continue;
                            }
                            if app.register_username.is_empty() {
                                app.register_error = Some("Username required".into());
                                continue;
                            }
                            if app.register_password.is_empty() {
                                app.register_error = Some("Password required".into());
                                continue;
                            }

                            match account_management::sign_up(
                                app.register_username.clone(),
                                app.register_password.clone(),
                                app.register_home_address.clone(),
                            )
                            .await
                            {
                                Ok(()) => {
                                    let new_name = app.register_username.clone();
                                    app.password.clear();
                                    app.register_username.clear();
                                    app.register_password.clear();
                                    app.register_home_address.clear();
                                    app.register_error = None;
                                    app.refresh_accounts();
                                    if let Some(idx) =
                                        app.accounts.iter().position(|u| u == &new_name)
                                    {
                                        app.selected_account = idx;
                                    }
                                    app.login_info = Some(
                                        "Account created. Sign in below.".into(),
                                    );
                                    app.screen = Screen::Login;
                                }
                                Err(e) => {
                                    app.register_error =
                                        Some(format!("Sign up failed: {e:#}"));
                                }
                            }
                        } else {
                            handle_register_input(&mut app, key.code);
                        }
                    }
                    Screen::Connecting => {
                        if key.code == KeyCode::Esc {
                            app.quit = true;
                        }
                    }
                    Screen::Chat => {
                        handle_chat_input(&mut app, key.code, key.modifiers, &ws_tx).await;
                    }
                    Screen::ServerSettings | Screen::UserSettings => {}
                }
            }

            // Network events
            Some(ev) = net_rx.recv() => {
                match ev {
                    NetEvent::Connected => {
                        app.status_message = Some("Authenticating...".into());
                    }
                    NetEvent::HandshakeOk(_session_id) => {
                        app.screen = Screen::Chat;
                    }
                    NetEvent::SigningKey(key) => {
                        app.signing_key = Some(key);
                    }
                    NetEvent::VerifyingKey(key) => {
                        app.verifying_key = Some(key);
                    }
                    NetEvent::SessionGranted(session_id) => {
                        app.session_id = session_id;
                    }
                    NetEvent::Error(e) => {
                        app.login_error = Some(e);
                        app.screen = Screen::Login;
                        ws_tx = None;
                    }
                    NetEvent::Incoming(msg) => {
                        handle_incoming(msg, &mut app);
                    }
                    NetEvent::Disconnected => {
                        app.login_error = Some("Disconnected from server".into());
                        app.screen = Screen::Login;
                        ws_tx = None;
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_register_input(app: &mut App, key: KeyCode) {
    let field = match app.register_field {
        RegisterField::HomeAddress => &mut app.register_home_address,
        RegisterField::Username => &mut app.register_username,
        RegisterField::Password => &mut app.register_password,
    };

    match key {
        KeyCode::Tab | KeyCode::Down => {
            app.register_field = app.register_field.next();
        }
        KeyCode::Backspace => {
            field.pop();
        }
        KeyCode::Char(c) => {
            field.push(c);
        }
        _ => {}
    }
}

fn handle_login_input(app: &mut App, key: KeyCode) {
    // Account field — navigate the account list instead of editing text.
    if app.login_field == LoginField::Account {
        match key {
            KeyCode::Tab => {
                app.login_field = app.login_field.next();
            }
            KeyCode::Up => {
                if app.selected_account > 0 {
                    app.selected_account -= 1;
                }
            }
            KeyCode::Down => {
                if app.selected_account + 1 < app.accounts.len() {
                    app.selected_account += 1;
                }
            }
            KeyCode::Esc => {
                app.quit = true;
            }
            _ => {}
        }
        return;
    }

    let field = match app.login_field {
        LoginField::ServerUrl => &mut app.server_url,
        LoginField::Account => unreachable!(),
        LoginField::Password => &mut app.password,
    };

    match key {
        KeyCode::Tab | KeyCode::Down => {
            app.login_field = app.login_field.next();
        }
        KeyCode::Backspace => {
            field.pop();
        }
        KeyCode::Char(c) => {
            field.push(c);
        }
        KeyCode::Esc => {
            app.quit = true;
        }
        _ => {}
    }
}

async fn handle_chat_input(
    app: &mut App,
    key: KeyCode,
    modifiers: KeyModifiers,
    ws_tx: &Option<mpsc::UnboundedSender<Message>>,
) {
    // Ctrl+L toggles channel list focus
    if modifiers.contains(KeyModifiers::CONTROL) && key == KeyCode::Char('l') {
        app.channel_list_focused = !app.channel_list_focused;
        return;
    }

    if app.channel_list_focused {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.selected_channel > 0 {
                    app.selected_channel -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.selected_channel + 1 < app.channels.len() {
                    app.selected_channel += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(channel) = app.channels.get(app.selected_channel).cloned() {
                    app.current_channel = channel.clone();
                    app.messages.clear();

                    let req = definitions::TextNetworkMessage::ClientRequestMessages(
                        definitions::ClientRequestMessages {
                            range: 0..255,
                            channel,
                        },
                    );
                    if let Some(tx) = ws_tx {
                        if let Ok(json) = serde_json::to_string(&req) {
                            let _ = tx.send(Message::Text(json.into()));
                        }
                    }
                    app.channel_list_focused = false;
                }
            }
            KeyCode::Esc | KeyCode::Tab => {
                app.channel_list_focused = false;
            }
            _ => {}
        }
        return;
    }

    // Command palette navigation
    if app.show_command_palette() {
        let matches = app.matching_commands();
        match key {
            KeyCode::Up => {
                if app.command_selected > 0 {
                    app.command_selected -= 1;
                }
                return;
            }
            KeyCode::Down => {
                if app.command_selected + 1 < matches.len() {
                    app.command_selected += 1;
                }
                return;
            }
            KeyCode::Tab | KeyCode::Enter => {
                let idx = app.command_selected % matches.len();
                let name = matches[idx].name;
                app.input = format!("{} ", name);
                app.input_cursor = app.input.len();
                app.command_selected = 0;
                return;
            }
            _ => {
                // Reset selection when typing
                app.command_selected = 0;
            }
        }
    }

    // Input mode
    match key {
        KeyCode::Enter => {
            let text = app.take_input();
            if text.is_empty() {
                return;
            }

            let Some(tx) = ws_tx else { return };

            // /kick command
            if text.starts_with("/kick ") {
                let args: Vec<&str> = text.split_whitespace().collect();
                if args.len() < 3 {
                    return;
                }
                let target_name = args[1];
                let reason = args[2..].join(" ");

                let recipient = app
                    .peers
                    .iter()
                    .find(|(_, v)| v.username == target_name)
                    .map(|(k, _)| *k);

                if let Some(recipient) = recipient {
                    let kick = definitions::TextNetworkMessage::ClientKickRequest(
                        definitions::ClientKickRequest { recipient, reason },
                    );
                    if let Ok(json) = serde_json::to_string(&kick) {
                        let _ = tx.send(Message::Text(json.into()));
                    }
                }
                return;
            } else if text.starts_with("/download ") {
                let args: Vec<&str> = text.split_whitespace().collect();
                if args.len() != 2 {
                    pluton_log(&format!("Incorrect arguement count for download command, should be 2, was {}", args.len()), Importance::Error);
                    return;
                }

                let Ok(id) = args[1].parse::<u64>() else {
                    pluton_log("Unable to convert ID into u64 for download command", Importance::Error);
                    return;
                };

                let file_meta = match pluton_core::networking::download_file_meta(app.server_ip.clone(), id).await {
                    Ok(m) => m,
                    Err(e) => {
                        pluton_log(&format!("Unable to download file metadata: {e}"), Importance::Error);
                        return;
                    }
                };

                let file_data = match pluton_core::networking::download_file(app.server_ip.clone(), id).await {
                    Ok(m) => m,
                    Err(e) => {
                        pluton_log(&format!("Unable to download file: {e}"), Importance::Error);
                        return
                    }
                };

                if let Some(handle) = AsyncFileDialog::new()
                    .set_directory("/home")
                    .set_file_name(file_meta.file_name)
                    .save_file()
                    .await
                {
                    let _ = handle.write(&file_data).await.map_err(|e| {
                        pluton_log(&format!("Failed to save file: {e}"), Importance::Error);
                    });
                };
            }

            // Normal message
            if let Some(signing_key) = app.signing_key.clone() {
                let id = app.next_message_id();
                let signed = signing_key.sign(text.as_bytes());
                let msg = definitions::TextNetworkMessage::ClientText(
                    definitions::ClientTextMessage {
                        plaintext: text,
                        signed_message: signed,
                        id,
                        attachments: app.current_files.clone(),
                        channel: app.current_channel.clone(),
                    },
                );
                if let Ok(json) = serde_json::to_string(&msg) {
                    let _ = tx.send(Message::Text(json.into()));
                }

                app.current_files.clear();
            }
        }
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Delete => app.input_delete(),
        KeyCode::Left => app.input_move_left(),
        KeyCode::Right => app.input_move_right(),
        KeyCode::Home => app.input_home(),
        KeyCode::End => app.input_end(),
        KeyCode::Tab => {
            app.channel_list_focused = true;
        }
        KeyCode::Esc => app.quit = true,
        KeyCode::Char(c) => {
            if c == 'u' && modifiers.contains(KeyModifiers::ALT) {
                let server_ip = app.server_ip.clone();
                let session_id = app.session_id.clone();

                let verifying_key = match app.verifying_key.clone() {
                    Some(m) => m,
                    None => {
                        return
                    }
                };

                let Some(file) = AsyncFileDialog::new()
                    .add_filter("any", &["*"])
                    .add_filter("image", &["png", "jpg", "jpeg", "webp"])
                    .set_directory("/")
                    .pick_file()
                    .await
                else {
                    return;
                };

                let data = file.read().await;

                let content_type = infer::get(&data)
                    .map(|t| t.mime_type().to_string())
                    .unwrap_or_else(|| {
                        mime_guess::from_path(file.file_name())
                            .first_or_octet_stream()
                            .essence_str()
                            .to_string()
                    });


                // We need to first upload the files and receive their IDs
                
                let file_id = pluton_core::networking::upload_file(
                    server_ip,
                    session_id,
                    file.file_name(),
                    data.clone(),
                    content_type
                ).await.expect("Error when uploading file");

                let constructed_file = definitions::FileDescriptor {
                    id: file_id,
                    file_name: file.file_name(),
                    file_size: data.len() as u64
                };

                app.current_files.push(constructed_file);
            } else {
                app.input_insert(c);
            }
        }
        _ => {}
    }
}

/// Process an incoming message and update app state.
fn handle_incoming(msg: definitions::TextNetworkMessage, app: &mut App) {
    match msg {
        definitions::TextNetworkMessage::ServerText(text_msg) => {
            if text_msg.channel_id != app.current_channel.id {
                return;
            }
            let sender = app.resolve_username(&text_msg.sender);
            app.add_message(sender, text_msg.plaintext, text_msg.timestamp, text_msg.attachments);
        }
        definitions::TextNetworkMessage::ServerStatus(status) => {
            app.server_name = status.name;
            app.peers = HashMap::new();

            for user in &status.users {
                app.peers.insert(
                    user.public_key,
                    definitions::Peer {
                        username: user.username.clone(),
                        address: user.address.clone(),
                        roles: user.roles.clone(),
                        status: user.status.clone(),
                    },
                );
            }
            app.current_channel = status.default_channel;
            app.channels = status.message_channels;
            app.voice_channels = status.voice_channels;
            app.messages.clear();
            for text_msg in &status.messages {
                let sender = app.resolve_username(&text_msg.sender);
                app.add_message(sender, text_msg.plaintext.clone(), text_msg.timestamp, text_msg.attachments.clone());
            }
        }
        definitions::TextNetworkMessage::UserJoin(user) => {
            app.peers.insert(
                user.public_key,
                definitions::Peer {
                    username: user.username,
                    address: user.address,
                    roles: user.roles,
                    status: user.status,
                },
            );
        }
        definitions::TextNetworkMessage::UserLeave(public_key) => {
            if let Some(peer) = app.peers.get_mut(&public_key) {
                peer.status = definitions::UserStatus::Offline
            }
        }
        definitions::TextNetworkMessage::ServerRequestMessages(response) => {
            app.messages.clear();
            for text_msg in &response.messages {
                let sender = app.resolve_username(&text_msg.sender);
                app.add_message(sender, text_msg.plaintext.clone(), text_msg.timestamp, text_msg.attachments.clone());
            }
        }
        definitions::TextNetworkMessage::UserStatusChange(change) => {
            if let Some(peer) = app.peers.get_mut(&change.public_key) {
                peer.status = change.status;
            }
        }
        _ => {}
    }
}

/// Connect to server, authenticate, and relay messages.
async fn run_network(
    event_tx: mpsc::UnboundedSender<NetEvent>,
    mut ws_rx: mpsc::UnboundedReceiver<Message>,
    url: String,
    username: String,
    password: String
) {
    // Sign in
    if let Err(e) = account_management::sign_in(username.clone(), password.clone()).await {
        let _ = event_tx.send(NetEvent::Error(format!("Sign in failed: {e}")));
        return;
    }

    let signing_key = match get_signing_key(&password).await {
        Ok(k) => k,
        Err(e) => {
            let _ = event_tx.send(NetEvent::Error(format!("Key error: {e}")));
            return;
        }
    };

    let account = match account_management::get_account().await {
        Ok(a) => a,
        Err(e) => {
            let _ = event_tx.send(NetEvent::Error(format!("Account error: {e}")));
            return;
        }
    };

    let public_key = match helper::general::vec_to_verifying_key(helper::base64::from_base64(
        account.verifying_key,
    )) {
        Ok(k) => k,
        Err(_) => {
            let _ = event_tx.send(NetEvent::Error("Bad verifying key".into()));
            return;
        }
    };

    // Connect websocket
    let (ws_stream, _) = match connect_async(&url).await {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(NetEvent::Error(format!("Connect failed: {e}")));
            return;
        }
    };

    let _ = event_tx.send(NetEvent::Connected);

    let (mut outgoing, mut incoming) = ws_stream.split();

    // Auth handshake
    let session_id =
        match pluton_core::networking::auth_handshake::auth_handshake_client(
            &mut outgoing,
            &mut incoming,
            &username,
            public_key,
            account.address,
            signing_key.clone(),
        )
        .await
        {
            Ok((definitions::HandshakeStatus::Complete, token)) => {
                let _ = event_tx.send(NetEvent::HandshakeOk(token.clone()));
                token
            }
            Ok((status, _)) => {
                let _ = event_tx.send(NetEvent::Error(format!("Handshake: {status:?}")));
                return;
            }
            Err(e) => {
                let _ = event_tx.send(NetEvent::Error(format!("Handshake: {e:?}")));
                return;
            }
        };

    let _ = event_tx.send(NetEvent::SessionGranted(session_id));

    // Send signing key to app
    let _ = event_tx.send(NetEvent::SigningKey(signing_key));

    let _ = event_tx.send(NetEvent::VerifyingKey(public_key));

    // Relay messages between server and TUI
    loop {
        tokio::select! {
            // Incoming from server
            msg = incoming.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(parsed) = serde_json::from_str::<definitions::TextNetworkMessage>(&text) {
                            let _ = event_tx.send(NetEvent::Incoming(parsed));
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ping/pong/binary - ignore
                }
            }
            // Outgoing from TUI
            msg = ws_rx.recv() => {
                match msg {
                    Some(m) => {
                        if outgoing.send(m).await.is_err() {
                            break;
                        }
                    }
                    None => break, // TUI dropped the sender, we're done
                }
            }
        }
    }

    let _ = event_tx.send(NetEvent::Disconnected);
}
