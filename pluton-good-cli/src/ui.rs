use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use pluton_core::networking::definitions::UserStatus;
use pluton_core::helper::general::size_to_descriptor;

use crate::app::{App, LoginField, RegisterField, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Login => draw_login(frame, app),
        Screen::Register => draw_register(frame, app),
        Screen::Connecting => draw_connecting(frame, app),
        Screen::Chat => draw_chat(frame, app),
        Screen::ServerSettings | Screen::UserSettings => todo!("Implement server settings and user settings")
    }
}

// ── Login Screen ────────────────────────────────────────────────────────

fn draw_login(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Center a box
    let popup = centered_rect(50, 60, area);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Pluton - Login ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let account_list_height = (app.accounts.len() as u16).clamp(1, 5) + 2; // +2 borders, min 1 row

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // server url
            Constraint::Length(1), // label
            Constraint::Length(account_list_height), // account list
            Constraint::Length(1), // label
            Constraint::Length(3), // password
            Constraint::Length(2), // spacer
            Constraint::Length(1), // help
            Constraint::Length(2), // error
            Constraint::Min(0),
        ])
        .split(inner);

    let active_style = Style::default().fg(Color::Yellow);
    let normal_style = Style::default().fg(Color::White);

    // Server URL
    let url_style = if app.login_field == LoginField::ServerUrl {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Server URL:").style(url_style),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(app.server_url.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(url_style),
            ),
        chunks[1],
    );

    // Account list
    let acc_style = if app.login_field == LoginField::Account {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Account:").style(acc_style),
        chunks[2],
    );
    let acc_items: Vec<ListItem> = if app.accounts.is_empty() {
        vec![ListItem::new(
            Span::styled("(none — Ctrl+N to create)", Style::default().fg(Color::DarkGray)),
        )]
    } else {
        app.accounts
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let is_sel = i == app.selected_account;
                let prefix = if is_sel { "> " } else { "  " };
                let style = if is_sel && app.login_field == LoginField::Account {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else if is_sel {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("{prefix}{name}")).style(style)
            })
            .collect()
    };
    frame.render_widget(
        List::new(acc_items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(acc_style),
        ),
        chunks[3],
    );

    // Password
    let pass_style = if app.login_field == LoginField::Password {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Password:").style(pass_style),
        chunks[4],
    );
    let masked: String = "*".repeat(app.password.len());
    frame.render_widget(
        Paragraph::new(masked)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(pass_style),
            ),
        chunks[5],
    );

    // Help
    frame.render_widget(
        Paragraph::new("Tab/Up/Down: navigate | Enter: connect | Ctrl+N: new account | Esc: quit")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        chunks[7],
    );

    // Error or info
    if let Some(ref err) = app.login_error {
        frame.render_widget(
            Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)),
            chunks[8],
        );
    } else if let Some(ref info) = app.login_info {
        frame.render_widget(
            Paragraph::new(info.as_str()).style(Style::default().fg(Color::Green)),
            chunks[8],
        );
    }

    // Place cursor (only on text fields)
    match app.login_field {
        LoginField::ServerUrl => {
            frame.set_cursor_position((
                chunks[1].x + 1 + app.server_url.len() as u16,
                chunks[1].y + 1,
            ));
        }
        LoginField::Account => {
            // No cursor — list navigation
        }
        LoginField::Password => {
            frame.set_cursor_position((
                chunks[5].x + 1 + app.password.len() as u16,
                chunks[5].y + 1,
            ));
        }
    }
}

// ── Register Screen ─────────────────────────────────────────────────────

fn draw_register(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let popup = centered_rect(50, 60, area);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Pluton - Create Account ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // home address
            Constraint::Length(1), // label
            Constraint::Length(3), // username
            Constraint::Length(1), // label
            Constraint::Length(3), // password
            Constraint::Length(2), // spacer
            Constraint::Length(1), // help
            Constraint::Length(2), // error
            Constraint::Min(0),
        ])
        .split(inner);

    let active_style = Style::default().fg(Color::Yellow);
    let normal_style = Style::default().fg(Color::White);

    // Home Address
    let addr_style = if app.register_field == RegisterField::HomeAddress {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Home Address:").style(addr_style),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(app.register_home_address.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(addr_style),
            ),
        chunks[1],
    );

    // Username
    let user_style = if app.register_field == RegisterField::Username {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Username:").style(user_style),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new(app.register_username.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(user_style),
            ),
        chunks[3],
    );

    // Password
    let pass_style = if app.register_field == RegisterField::Password {
        active_style
    } else {
        normal_style
    };
    frame.render_widget(
        Paragraph::new("Password:").style(pass_style),
        chunks[4],
    );
    let masked: String = "*".repeat(app.register_password.len());
    frame.render_widget(
        Paragraph::new(masked)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(pass_style),
            ),
        chunks[5],
    );

    // Help
    frame.render_widget(
        Paragraph::new("Tab: next | Enter: create | Esc: back")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[7],
    );

    // Error
    if let Some(ref err) = app.register_error {
        frame.render_widget(
            Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)),
            chunks[8],
        );
    }

    // Place cursor
    match app.register_field {
        RegisterField::HomeAddress => {
            frame.set_cursor_position((
                chunks[1].x + 1 + app.register_home_address.len() as u16,
                chunks[1].y + 1,
            ));
        }
        RegisterField::Username => {
            frame.set_cursor_position((
                chunks[3].x + 1 + app.register_username.len() as u16,
                chunks[3].y + 1,
            ));
        }
        RegisterField::Password => {
            frame.set_cursor_position((
                chunks[5].x + 1 + app.register_password.len() as u16,
                chunks[5].y + 1,
            ));
        }
    }
}

// ── Connecting Screen ───────────────────────────────────────────────────

fn draw_connecting(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let popup = centered_rect(40, 20, area);
    frame.render_widget(Clear, popup);

    let msg = app
        .status_message
        .as_deref()
        .unwrap_or("Connecting...");

    let block = Block::default()
        .title(" Pluton ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    frame.render_widget(
        Paragraph::new(msg).style(Style::default().fg(Color::Yellow)),
        inner,
    );
}

// ── Chat Screen ─────────────────────────────────────────────────────────

fn draw_chat(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Main layout: sidebar | chat
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(22), Constraint::Min(1)])
        .split(area);

    draw_sidebar(frame, app, main_chunks[0]);
    draw_chat_area(frame, app, main_chunks[1]);
}

fn draw_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    // Channels
    let channel_items: Vec<ListItem> = app
        .channels
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let prefix = if ch.id == app.current_channel.id {
                "> "
            } else {
                "  "
            };
            let style = if app.channel_list_focused && i == app.selected_channel {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if ch.id == app.current_channel.id {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("{prefix}# {}", ch.name)).style(style)
        })
        .collect();

    let channels_block = Block::default()
        .title(" Channels ")
        .borders(Borders::ALL)
        .border_style(if app.channel_list_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    frame.render_widget(
        List::new(channel_items).block(channels_block),
        chunks[0],
    );

    // Users (sorted: online first, then by name)
    let mut user_items: Vec<ListItem> = Vec::new();
    let mut all_users: Vec<_> = app.peers.values().collect();
    all_users.sort_by(|a, b| {
        let a_online = !matches!(a.status, UserStatus::Offline);
        let b_online = !matches!(b.status, UserStatus::Offline);
        b_online.cmp(&a_online).then(a.username.cmp(&b.username))
    });

    for peer in &all_users {
        let status_color = match peer.status {
            UserStatus::Online => Color::Green,
            UserStatus::DoNotDisturb => Color::Red,
            UserStatus::Sleep => Color::Yellow,
            UserStatus::Offline => Color::DarkGray,
        };
        let name_style = if matches!(peer.status, UserStatus::Offline) {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };
        let dot = Span::styled("● ", Style::default().fg(status_color));
        let name = Span::styled(peer.username.as_str(), name_style);
        user_items.push(ListItem::new(Line::from(vec![dot, name])));
    }

    let users_block = Block::default()
        .title(format!(" Users ({}) ", all_users.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    frame.render_widget(List::new(user_items).block(users_block), chunks[1]);
}

fn draw_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    // Messages
    let title = format!(" #{} ", app.current_channel.name);
    let msg_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner_height = msg_block.inner(chunks[0]).height as usize;

    let msg_lines: Vec<Line> = app
        .messages
        .iter()
        .flat_map(|m| {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("{} ", m.time),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{}: ", m.sender),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&m.text),
            ])];

            lines.extend(m.attachments.iter().map(|a| {
                Line::from(Span::styled(
                    format!("[{} - ID:{} - {}]", a.file_name, a.id, size_to_descriptor(a.file_size)),
                    Style::default().fg(Color::Yellow)
                ))
            }));

            lines
        })
        .collect();

    // Auto-scroll to bottom
    let scroll = if msg_lines.len() > inner_height {
        (msg_lines.len() - inner_height) as u16
    } else {
        0
    };

    frame.render_widget(
        Paragraph::new(msg_lines)
            .block(msg_block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        chunks[0],
    );

    // Input bar
    let input_title = if app.channel_list_focused {
        " Tab: back to chat "
    } else {
        " Message (Tab: channels) "
    };
    let input_block = Block::default()
        .title(input_title)
        .borders(Borders::ALL)
        .border_style(if !app.channel_list_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    frame.render_widget(
        Paragraph::new(app.input.as_str()).block(input_block),
        chunks[1],
    );

    // Command palette popup (rendered above the input bar)
    if app.show_command_palette() {
        let matches = app.matching_commands();
        let item_count = matches.len() as u16;
        let popup_height = item_count + 2; // +2 for borders

        // Position: directly above the input bar
        let popup_area = Rect {
            x: chunks[1].x,
            y: chunks[1].y.saturating_sub(popup_height),
            width: chunks[1].width.min(45),
            height: popup_height,
        };

        let items: Vec<ListItem> = matches
            .iter()
            .enumerate()
            .map(|(i, cmd)| {
                let style = if i == app.command_selected % matches.len() {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let line = Line::from(vec![
                    Span::styled(format!("{:<8}", cmd.name), style),
                    Span::styled(
                        format!(" {} ", cmd.usage),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("- {}", cmd.description),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let palette_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        frame.render_widget(Clear, popup_area);
        frame.render_widget(List::new(items).block(palette_block), popup_area);
    }

    // Show cursor in input when focused
    if !app.channel_list_focused {
        let cursor_x = chunks[1].x + 1 + app.input_cursor as u16;
        let cursor_y = chunks[1].y + 1;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
