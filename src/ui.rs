use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, ConfirmAction, FocusedPanel, InputMode, PopupKind};
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
            Constraint::Length(3), // Status bar
            Constraint::Min(3),   // Main content
        ])
        .split(frame.area());

    render_status_bar(frame, app, outer[0]);
    render_main_panels(frame, app, outer[1]);

    // Overlays
    if app.loading {
        render_loading(frame, &app.loading_message);
    }

    match &app.input_mode {
        InputMode::Popup(kind) => render_popup(frame, app, kind.clone()),
        InputMode::Confirm(action) => render_confirm(frame, app, action.clone()),
        InputMode::Normal => {}
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status = &app.client_status;

    let status_indicator = if status.is_logged_in() {
        Span::styled("● Online", Style::default().fg(GREEN).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("● Offline", Style::default().fg(RED).add_modifier(Modifier::BOLD))
    };

    let ip = if status.address.is_empty() { "-" } else { &status.address };
    let nick = if status.nickname.is_empty() { "-" } else { &status.nickname };
    let id = if status.client_id.is_empty() { "-" } else { &status.client_id };

    let line = Line::from(vec![
        Span::raw(" "),
        status_indicator,
        Span::styled(" │ ", Style::default().fg(GRAY)),
        Span::styled(format!("{}", ip), Style::default().fg(WHITE)),
        Span::styled(" │ ", Style::default().fg(GRAY)),
        Span::styled(format!("{}", id), Style::default().fg(WHITE)),
        Span::styled(" │ ", Style::default().fg(GRAY)),
        Span::styled(format!("{}", nick), Style::default().fg(WHITE)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " Hamachi ",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

fn render_main_panels(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18), // Fixed-width action menu
            Constraint::Min(20),   // Networks + peers
        ])
        .split(area);

    render_action_menu(frame, app, chunks[0]);
    render_networks(frame, app, chunks[1]);
}

fn render_action_menu(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focused_panel == FocusedPanel::Actions;
    let border_color = if focused { CYAN } else { GRAY };

    let actions = app.available_actions();
    let items: Vec<ListItem> = actions
        .iter()
        .map(|action| {
            ListItem::new(Line::from(Span::styled(
                action.label(),
                Style::default().fg(WHITE),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " Actions ",
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

    frame.render_stateful_widget(list, area, &mut app.action_list_state);
}

fn render_networks(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focused_panel == FocusedPanel::Networks;
    let border_color = if focused { CYAN } else { GRAY };

    // Build a flat list: network headers + indented peers
    let mut items: Vec<ListItem> = Vec::new();
    let mut row_map: Vec<(usize, Option<usize>)> = Vec::new(); // (network_idx, peer_idx)

    for (net_idx, net) in app.networks.iter().enumerate() {
        let online_count = net.peers.iter().filter(|p| p.status == PeerStatus::Online).count();
        let indicator = if net.is_online {
            Span::styled("● ", Style::default().fg(GREEN))
        } else {
            Span::styled("○ ", Style::default().fg(GRAY))
        };

        let name = Span::styled(
            format!("{}", net.name),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        );
        let id = if net.id != net.name {
            Span::styled(
                format!(" ({})", net.id),
                Style::default().fg(GRAY),
            )
        } else {
            Span::raw("")
        };
        let count = Span::styled(
            format!(" {}/{}", online_count, net.peers.len()),
            Style::default().fg(GRAY),
        );

        items.push(ListItem::new(Line::from(vec![indicator, name, id, count])));
        row_map.push((net_idx, None));

        for (peer_idx, peer) in net.peers.iter().enumerate() {
            let (dot_color, conn_color) = match peer.status {
                PeerStatus::Online => {
                    if peer.connection_type == "relay" {
                        (YELLOW, YELLOW)
                    } else {
                        (GREEN, GREEN)
                    }
                }
                PeerStatus::Offline => (RED, GRAY),
                PeerStatus::Unknown => (GRAY, GRAY),
            };

            let dot = Span::styled("  ● ", Style::default().fg(dot_color));
            let nick = Span::styled(
                format!("{}", peer.nickname),
                Style::default().fg(WHITE),
            );
            let addr = Span::styled(
                format!(" {}", peer.address),
                Style::default().fg(GRAY),
            );
            let conn = Span::styled(
                format!(" {}", peer.connection_type),
                Style::default().fg(conn_color),
            );

            items.push(ListItem::new(Line::from(vec![dot, nick, addr, conn])));
            row_map.push((net_idx, Some(peer_idx)));
        }
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " No networks",
            Style::default().fg(GRAY),
        ))));
    }

    let title = format!(" Networks ({}) ", app.networks.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    title,
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    frame.render_stateful_widget(list, area, &mut app.network_list_state);
}

fn render_loading(frame: &mut Frame, message: &str) {
    let area = centered_rect(40, 5, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(YELLOW))
        .title(Span::styled(
            " Loading ",
            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(message.to_string())
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}

fn render_popup(frame: &mut Frame, app: &mut App, kind: PopupKind) {
    let (title, prompt) = match kind {
        PopupKind::CreateNetworkName => ("Create Network", "Network name:"),
        PopupKind::CreateNetworkPassword => ("Create Network", "Password:"),
        PopupKind::JoinNetworkName => ("Join Network", "Network ID:"),
        PopupKind::JoinNetworkPassword => ("Join Network", "Password (empty for none):"),
        PopupKind::SetNickname => ("Set Nickname", "New nickname:"),
        PopupKind::SetPassword => ("Set Password", "New password (empty to remove):"),
        PopupKind::EvictSelectPeer => {
            render_evict_peer_popup(frame, app);
            return;
        }
        PopupKind::AccessSelect => {
            render_access_popup(frame, app);
            return;
        }
        PopupKind::Error => {
            render_error_popup(frame, &app.error_message);
            return;
        }
    };

    let area = centered_rect(50, 7, frame.area());
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

fn render_error_popup(frame: &mut Frame, message: &str) {
    let area = centered_rect(60, 8, frame.area());
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

fn render_evict_peer_popup(frame: &mut Frame, app: &mut App) {
    let peers: Vec<(String, String, String)> = app
        .selected_network()
        .map(|n| {
            n.peers
                .iter()
                .map(|p| (p.nickname.clone(), p.client_id.clone(), p.address.clone()))
                .collect()
        })
        .unwrap_or_default();

    let height = (peers.len() as u16 + 4).min(16);
    let area = centered_rect(50, height, frame.area());
    frame.render_widget(Clear, area);

    let items: Vec<ListItem> = peers
        .iter()
        .map(|(nick, id, _addr)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}", nick), Style::default().fg(WHITE)),
                Span::styled(format!("  {}", id), Style::default().fg(GRAY)),
            ]))
        })
        .collect();

    let net_name = app.selected_network().map(|n| n.name.as_str()).unwrap_or("?");
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    format!(" Evict from {} ", net_name),
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

    frame.render_stateful_widget(list, area, &mut app.peer_list_state);
}

fn render_access_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(40, 6, frame.area());
    frame.render_widget(Clear, area);

    let net_name = app.selected_network().map(|n| n.name.as_str()).unwrap_or("?");

    let options = vec!["Lock", "Unlock"];
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == app.access_selection {
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            let symbol = if i == app.access_selection { "▸ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(symbol, style),
                Span::styled(*label, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(Span::styled(
                format!(" Access: {} ", net_name),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            )),
    );

    frame.render_widget(list, area);
}

fn render_confirm(frame: &mut Frame, app: &App, action: ConfirmAction) {
    let message = match action {
        ConfirmAction::DeleteNetwork => {
            let name = app.selected_network().map(|n| n.name.as_str()).unwrap_or("?");
            format!("Delete network '{}'?", name)
        }
        ConfirmAction::EvictPeer => {
            let peer = app.selected_network().and_then(|n| {
                app.peer_list_state.selected().and_then(|i| n.peers.get(i))
            });
            let name = peer.map(|p| p.nickname.as_str()).unwrap_or("?");
            format!("Evict peer '{}'?", name)
        }
        ConfirmAction::LeaveNetwork => {
            let name = app.selected_network().map(|n| n.name.as_str()).unwrap_or("?");
            format!("Leave network '{}'?", name)
        }
    };

    let area = centered_rect(50, 7, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(YELLOW))
        .title(Span::styled(
            " Confirm ",
            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        ));

    let text = format!("{}\n\n[Y]es  [N]o", message);
    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(WHITE))
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
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
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
