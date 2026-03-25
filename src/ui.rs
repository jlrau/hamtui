use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{
    App, ConfirmAction, FocusedSection, HamachiSelection, InputMode,
    NetworkAction, NetworkRow, PeerAction, PopupKind,
};
use crate::hamachi::PeerStatus;

const CYAN: Color = Color::Cyan;
const GREEN: Color = Color::Green;
const YELLOW: Color = Color::Yellow;
const RED: Color = Color::Red;
const GRAY: Color = Color::DarkGray;
const WHITE: Color = Color::White;

pub fn render(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Hamachi box (status + selectable row)
            Constraint::Min(3),   // Networks list
        ])
        .split(frame.area());

    render_hamachi_box(frame, app, outer[0]);
    render_networks(frame, app, outer[1]);

    match &app.input_mode {
        InputMode::Popup(kind) => render_popup(frame, app, kind.clone()),
        InputMode::Confirm(action) => render_confirm(frame, app, action.clone()),
        InputMode::Normal => {}
    }
}

fn render_hamachi_box(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focused_section == FocusedSection::Hamachi;
    let border_color = if focused { CYAN } else { GRAY };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " HamTUI ",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Status line
            Constraint::Length(1), // Selectable row
        ])
        .split(inner);

    // Status line
    let status = &app.client_status;
    let status_indicator = if status.is_logged_in() {
        Span::styled(
            "● Online",
            Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "● Offline",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )
    };

    let ip = if status.address.is_empty() {
        "-"
    } else {
        &status.address
    };
    let id = if status.client_id.is_empty() {
        "-"
    } else {
        &status.client_id
    };

    let status_line = Line::from(vec![
        Span::raw(" "),
        status_indicator,
        Span::styled(" │ ", Style::default().fg(GRAY)),
        Span::styled(ip.to_string(), Style::default().fg(WHITE)),
        Span::styled(" │ ", Style::default().fg(GRAY)),
        Span::styled(id.to_string(), Style::default().fg(WHITE)),
    ]);
    frame.render_widget(Paragraph::new(status_line), rows[0]);

    // Selectable row: nickname ... [Logout/Login] [Quit]
    let nick_display = if status.nickname.is_empty() {
        "-".to_string()
    } else {
        status.nickname.clone()
    };

    let login_label = if status.is_logged_in() {
        "Logout"
    } else {
        "Login"
    };

    let is_selected = |sel: &HamachiSelection| -> bool {
        focused && app.hamachi_selection == *sel
    };

    let nick_style = if is_selected(&HamachiSelection::Nickname) {
        Style::default()
            .fg(CYAN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };

    let logout_style = if is_selected(&HamachiSelection::Logout) {
        Style::default()
            .fg(CYAN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };

    let quit_style = if is_selected(&HamachiSelection::Quit) {
        Style::default()
            .fg(CYAN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };

    let nick_prefix = if is_selected(&HamachiSelection::Nickname) {
        "▸ "
    } else {
        "  "
    };

    let selectable_line = Line::from(vec![
        Span::styled(nick_prefix, nick_style),
        Span::styled(nick_display, nick_style),
        Span::raw("  "),
        Span::styled(format!("[{}]", login_label), logout_style),
        Span::raw(" "),
        Span::styled("[Quit]", quit_style),
    ]);
    frame.render_widget(Paragraph::new(selectable_line), rows[1]);
}

fn render_networks(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focused_section == FocusedSection::Networks;
    let border_color = if focused { CYAN } else { GRAY };
    let logged_in = app.client_status.is_logged_in();

    let mut items: Vec<ListItem> = Vec::new();

    for row in &app.network_rows {
        match row {
            NetworkRow::Network(net_idx) => {
                let net = &app.networks[*net_idx];
                let online_count = net
                    .peers
                    .iter()
                    .filter(|p| p.status == PeerStatus::Online)
                    .count();
                let indicator = if net.is_online {
                    Span::styled("● ", Style::default().fg(if logged_in { GREEN } else { GRAY }))
                } else {
                    Span::styled("○ ", Style::default().fg(GRAY))
                };

                let name_color = if logged_in { WHITE } else { GRAY };
                let name = Span::styled(
                    net.name.clone(),
                    Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                );
                let id = if net.id != net.name {
                    Span::styled(format!(" ({})", net.id), Style::default().fg(GRAY))
                } else {
                    Span::raw("")
                };
                let count = Span::styled(
                    format!(" {}/{}", online_count, net.peers.len()),
                    Style::default().fg(GRAY),
                );

                items.push(ListItem::new(Line::from(vec![indicator, name, id, count])));
            }
            NetworkRow::Peer(net_idx, peer_idx) => {
                let peer = &app.networks[*net_idx].peers[*peer_idx];
                let (dot_color, conn_color) = if !logged_in {
                    (GRAY, GRAY)
                } else {
                    match peer.status {
                        PeerStatus::Online => {
                            if peer.connection_type == "relay" {
                                (YELLOW, YELLOW)
                            } else {
                                (GREEN, GREEN)
                            }
                        }
                        PeerStatus::Offline => (RED, GRAY),
                        PeerStatus::Unknown => (GRAY, GRAY),
                    }
                };

                let nick_color = if logged_in { WHITE } else { GRAY };
                let dot = Span::styled("  ● ", Style::default().fg(dot_color));
                let nick = Span::styled(peer.nickname.clone(), Style::default().fg(nick_color));
                let conn = Span::styled(
                    format!(" {}", peer.connection_type),
                    Style::default().fg(conn_color),
                );

                items.push(ListItem::new(Line::from(vec![dot, nick, conn])));
            }
            NetworkRow::JoinNetwork => {
                let color = if logged_in { CYAN } else { GRAY };
                items.push(ListItem::new(Line::from(Span::styled(
                    "+ Join Network",
                    Style::default().fg(color),
                ))));
            }
            NetworkRow::CreateNetwork => {
                let color = if logged_in { CYAN } else { GRAY };
                items.push(ListItem::new(Line::from(Span::styled(
                    "+ Create Network",
                    Style::default().fg(color),
                ))));
            }
        }
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " No networks",
            Style::default().fg(GRAY),
        ))));
    }

    let title = format!(" Networks ({}) ", app.networks.len());
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));

    if app.loading {
        let max_dots = 3;
        let dot_count = (app.loading_tick % (max_dots + 1)) as usize;
        let dots = ".".repeat(dot_count);
        let padding = " ".repeat(max_dots - dot_count);
        let loading_text = format!(" {}{}{} ", app.loading_message, dots, padding);
        block = block.title_bottom(Span::styled(
            loading_text,
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    if focused {
        frame.render_stateful_widget(list, area, &mut app.network_list_state);
    } else {
        // Render without selection highlight when not focused
        let mut empty_state = ratatui::widgets::ListState::default();
        frame.render_stateful_widget(list, area, &mut empty_state);
    }
}


fn render_popup(frame: &mut Frame, app: &mut App, kind: PopupKind) {
    match kind {
        PopupKind::SetNickname => {
            render_text_input_popup(frame, app, "Set Nickname", "New nickname:");
        }
        PopupKind::SetPassword => {
            render_text_input_popup(frame, app, "Set Password", "New password (empty to remove):");
        }
        PopupKind::JoinNetwork => {
            render_text_input_popup(frame, app, "Join Network", "Network ID:");
        }
        PopupKind::JoinPassword => {
            render_text_input_popup(frame, app, "Join Network", "Password (empty for none):");
        }
        PopupKind::CreateNetwork => {
            render_text_input_popup(frame, app, "Create Network", "Network Name:");
        }
        PopupKind::CreatePassword => {
            render_text_input_popup(frame, app, "Create Network", "Password:");
        }
        PopupKind::NetworkActions => {
            render_network_actions_popup(frame, app);
        }
        PopupKind::PeerActions => {
            render_peer_actions_popup(frame, app);
        }
        PopupKind::AccessSelect => {
            render_access_popup(frame, app);
        }
        PopupKind::Error => {
            render_error_popup(frame, &app.error_message);
        }
    }
}

fn render_text_input_popup(frame: &mut Frame, app: &App, title: &str, prompt: &str) {
    let area = centered_rect(90, 7, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .split(inner);

    let prompt_widget = Paragraph::new(prompt).style(Style::default().fg(WHITE));
    frame.render_widget(prompt_widget, chunks[0]);

    let input_display = format!("▸ {}_", app.input_buffer);
    let input_widget = Paragraph::new(input_display).style(Style::default().fg(GREEN));
    frame.render_widget(input_widget, chunks[2]);
}

fn render_network_actions_popup(frame: &mut Frame, app: &mut App) {
    let net = app.context_network();
    let net_name = net.map(|n| n.name.as_str()).unwrap_or("?");
    let is_online = net.map(|n| n.is_online).unwrap_or(false);

    let actions = NetworkAction::all();
    let height = (actions.len() as u16 + 2).min(12);
    let area = centered_rect(90, height, frame.area());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = actions
        .iter()
        .map(|action| {
            ListItem::new(Line::from(Span::styled(
                action.label(is_online),
                Style::default().fg(WHITE),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    format!(" {} ", net_name),
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .fg(CYAN)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(list, area, &mut app.network_action_state);
}

fn render_peer_actions_popup(frame: &mut Frame, app: &mut App) {
    let (peer_name, peer_addr) = app
        .context_network_idx
        .and_then(|ni| {
            app.context_peer_idx.and_then(|pi| {
                app.networks
                    .get(ni)
                    .and_then(|n| n.peers.get(pi))
                    .map(|p| (p.nickname.clone(), p.address.clone()))
            })
        })
        .unwrap_or_else(|| ("?".to_string(), "".to_string()));

    let actions = PeerAction::all();
    let height = (actions.len() as u16 + 2).min(8);
    let area = centered_rect(90, height, frame.area());
    frame.render_widget(Clear, area);

    let mut items: Vec<ListItem> = Vec::new();

    // Action items
    for action in &actions {
        items.push(ListItem::new(Line::from(Span::styled(
            action.label(),
            Style::default().fg(WHITE),
        ))));
    }

    let title = if peer_addr.is_empty() {
        format!(" {} ", peer_name)
    } else {
        format!(" {} ({}) ", peer_name, peer_addr)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    title,
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .fg(CYAN)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(list, area, &mut app.peer_action_state);
}

fn render_error_popup(frame: &mut Frame, message: &str) {
    let area = centered_rect(90, 8, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RED))
        .title(Span::styled(
            " Error ",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        ));

    let text = format!("{}\n\nPress Enter to dismiss", message);
    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(WHITE))
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

fn render_access_popup(frame: &mut Frame, app: &mut App) {
    let net_name = app
        .context_network()
        .map(|n| n.name.as_str())
        .unwrap_or("?");

    let area = centered_rect(90, 4, frame.area());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = vec!["Lock", "Unlock"]
        .iter()
        .map(|label| {
            ListItem::new(Line::from(Span::styled(
                *label,
                Style::default().fg(WHITE),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    format!(" Access: {} ", net_name),
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .fg(CYAN)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(list, area, &mut app.access_list_state);
}

fn render_confirm(frame: &mut Frame, app: &mut App, action: ConfirmAction) {
    let title = match action {
        ConfirmAction::DeleteNetwork => {
            let name = app
                .context_network()
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            format!(" Delete '{}' ", name)
        }
        ConfirmAction::EvictPeer => {
            let name = app
                .context_network_idx
                .and_then(|ni| {
                    app.context_peer_idx.and_then(|pi| {
                        app.networks
                            .get(ni)
                            .and_then(|n| n.peers.get(pi))
                            .map(|p| p.nickname.as_str())
                    })
                })
                .unwrap_or("?");
            format!(" Evict '{}' ", name)
        }
        ConfirmAction::LeaveNetwork => {
            let name = app
                .context_network()
                .map(|n| n.name.as_str())
                .unwrap_or("?");
            format!(" Leave '{}' ", name)
        }
    };

    let area = centered_rect(90, 4, frame.area());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = vec!["Yes", "No"]
        .iter()
        .map(|label| {
            ListItem::new(Line::from(Span::styled(
                *label,
                Style::default().fg(WHITE),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    title,
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .fg(CYAN)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(list, area, &mut app.confirm_list_state);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let max_width: u16 = 60;
    let desired_width = (area.width as u32 * percent_x as u32 / 100) as u16;
    let width = desired_width.min(max_width).min(area.width);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height),
            Constraint::Fill(1),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width),
            Constraint::Fill(1),
        ])
        .split(vertical[1])[1]
}
