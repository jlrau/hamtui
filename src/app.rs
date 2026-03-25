use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use tokio::sync::mpsc;

use crate::event::Event;
use crate::hamachi::{self, ClientStatus, HamachiResult, Network};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Popup(PopupKind),
    Confirm(ConfirmAction),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PopupKind {
    SetNickname,
    NetworkActions,
    PeerActions,
    JoinNetwork,
    JoinPassword,
    CreateNetwork,
    CreatePassword,
    SetPassword,
    AccessSelect,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteNetwork,
    EvictPeer,
    LeaveNetwork,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusedSection {
    Hamachi,
    Networks,
}

/// What is selected in the Hamachi bar row
#[derive(Debug, Clone, PartialEq)]
pub enum HamachiSelection {
    Nickname,
    Logout, // or Login when logged out
    Quit,
}

/// Items in the flat network list
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkRow {
    Network(usize),           // index into app.networks
    Peer(usize, usize),       // (network_idx, peer_idx)
    JoinNetwork,
    CreateNetwork,
}

/// Actions available for a selected network
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkAction {
    OnlineOffline,
    Leave,
    Delete,
    Password,
    Access,
}

impl NetworkAction {
    pub fn label(&self, is_online: bool) -> &str {
        match self {
            NetworkAction::OnlineOffline => {
                if is_online { "Go Offline" } else { "Go Online" }
            }
            NetworkAction::Leave => "Leave",
            NetworkAction::Delete => "Delete",
            NetworkAction::Password => "Password",
            NetworkAction::Access => "Access",
        }
    }

    pub fn all() -> Vec<NetworkAction> {
        vec![
            NetworkAction::OnlineOffline,
            NetworkAction::Leave,
            NetworkAction::Delete,
            NetworkAction::Password,
            NetworkAction::Access,
        ]
    }
}

/// Actions available for a selected peer
#[derive(Debug, Clone, PartialEq)]
pub enum PeerAction {
    Evict,
}

impl PeerAction {
    pub fn label(&self) -> &str {
        match self {
            PeerAction::Evict => "Evict",
        }
    }

    pub fn all() -> Vec<PeerAction> {
        vec![PeerAction::Evict]
    }
}

pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub focused_section: FocusedSection,
    pub hamachi_selection: HamachiSelection,
    pub client_status: ClientStatus,
    pub networks: Vec<Network>,
    pub network_list_state: ListState,
    pub network_rows: Vec<NetworkRow>,
    pub input_buffer: String,
    pub temp_buffer: String,
    pub error_message: String,
    pub loading: bool,
    pub loading_message: String,
    pub loading_tick: usize,
    pub last_refresh: Instant,
    // Popup state
    pub network_action_state: ListState,
    pub peer_action_state: ListState,
    pub access_list_state: ListState,
    pub confirm_list_state: ListState,
    // Context for popup actions
    pub context_network_idx: Option<usize>,
    pub context_peer_idx: Option<usize>,
    // Background command channel
    pub cmd_tx: mpsc::UnboundedSender<HamachiResult>,
    pub cmd_rx: mpsc::UnboundedReceiver<HamachiResult>,
}

impl App {
    pub fn new() -> Self {
        let mut network_list_state = ListState::default();
        network_list_state.select(Some(0));
        let mut network_action_state = ListState::default();
        network_action_state.select(Some(0));
        let mut peer_action_state = ListState::default();
        peer_action_state.select(Some(0));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            focused_section: FocusedSection::Hamachi,
            hamachi_selection: HamachiSelection::Nickname,
            client_status: ClientStatus::default(),
            networks: Vec::new(),
            network_list_state,
            network_rows: vec![NetworkRow::JoinNetwork, NetworkRow::CreateNetwork],
            input_buffer: String::new(),
            temp_buffer: String::new(),
            error_message: String::new(),
            loading: false,
            loading_message: String::new(),
            loading_tick: 0,
            last_refresh: Instant::now() - AUTO_REFRESH_INTERVAL,
            network_action_state,
            peer_action_state,
            access_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            confirm_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            context_network_idx: None,
            context_peer_idx: None,
            cmd_tx,
            cmd_rx,
        }
    }

    /// Rebuild the flat network_rows list from current networks
    pub fn rebuild_network_rows(&mut self) {
        self.network_rows.clear();
        for (net_idx, net) in self.networks.iter().enumerate() {
            self.network_rows.push(NetworkRow::Network(net_idx));
            for (peer_idx, _) in net.peers.iter().enumerate() {
                self.network_rows.push(NetworkRow::Peer(net_idx, peer_idx));
            }
        }
        self.network_rows.push(NetworkRow::JoinNetwork);
        self.network_rows.push(NetworkRow::CreateNetwork);
    }

    pub fn selected_network_row(&self) -> Option<&NetworkRow> {
        self.network_list_state
            .selected()
            .and_then(|i| self.network_rows.get(i))
    }

    pub fn context_network(&self) -> Option<&Network> {
        self.context_network_idx
            .and_then(|i| self.networks.get(i))
    }

    pub fn context_network_id(&self) -> Option<String> {
        self.context_network().map(|n| n.id.clone())
    }

    pub async fn refresh_status(&mut self) {
        self.client_status = hamachi::get_status().await;
        self.networks = hamachi::get_networks().await;
        self.rebuild_network_rows();

        // Clamp network list selection
        if self.network_rows.is_empty() {
            self.network_list_state.select(None);
        } else if let Some(selected) = self.network_list_state.selected() {
            if selected >= self.network_rows.len() {
                self.network_list_state
                    .select(Some(self.network_rows.len() - 1));
            }
        }

        self.last_refresh = Instant::now();
    }

    pub async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key).await,
            Event::Tick => {
                if self.loading {
                    self.loading_tick += 1;
                } else if self.last_refresh.elapsed() >= AUTO_REFRESH_INTERVAL {
                    self.refresh_status().await;
                }
            }
            Event::Resize(_, _) => {}
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match &self.input_mode.clone() {
            InputMode::Normal => self.handle_normal_key(key).await,
            InputMode::Popup(kind) => self.handle_popup_key(key, kind.clone()).await,
            InputMode::Confirm(action) => self.handle_confirm_key(key, action.clone()).await,
        }
    }

    async fn handle_normal_key(&mut self, key: KeyEvent) {
        if self.loading {
            return;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.navigate_up(),
            KeyCode::Down | KeyCode::Char('j') => self.navigate_down(),
            KeyCode::Left | KeyCode::Char('h') => self.navigate_left(),
            KeyCode::Right | KeyCode::Char('l') => self.navigate_right(),
            KeyCode::Tab => self.toggle_section(),
            KeyCode::Enter => self.activate_selection().await,
            _ => {}
        }
    }

    fn navigate_up(&mut self) {
        match self.focused_section {
            FocusedSection::Hamachi => {
                // Already at top, do nothing
            }
            FocusedSection::Networks => {
                if let Some(selected) = self.network_list_state.selected() {
                    if selected > 0 {
                        self.network_list_state.select(Some(selected - 1));
                    } else {
                        // Move up to Hamachi section
                        self.focused_section = FocusedSection::Hamachi;
                    }
                } else {
                    self.focused_section = FocusedSection::Hamachi;
                }
            }
        }
    }

    fn navigate_down(&mut self) {
        match self.focused_section {
            FocusedSection::Hamachi => {
                // Move down to Networks section
                self.focused_section = FocusedSection::Networks;
                if self.network_list_state.selected().is_none() && !self.network_rows.is_empty() {
                    self.network_list_state.select(Some(0));
                }
            }
            FocusedSection::Networks => {
                if let Some(selected) = self.network_list_state.selected() {
                    if selected + 1 < self.network_rows.len() {
                        self.network_list_state.select(Some(selected + 1));
                    }
                }
            }
        }
    }

    fn navigate_left(&mut self) {
        if self.focused_section == FocusedSection::Hamachi {
            self.hamachi_selection = match self.hamachi_selection {
                HamachiSelection::Quit => HamachiSelection::Logout,
                HamachiSelection::Logout => HamachiSelection::Nickname,
                HamachiSelection::Nickname => HamachiSelection::Nickname,
            };
        }
    }

    fn navigate_right(&mut self) {
        if self.focused_section == FocusedSection::Hamachi {
            self.hamachi_selection = match self.hamachi_selection {
                HamachiSelection::Nickname => HamachiSelection::Logout,
                HamachiSelection::Logout => HamachiSelection::Quit,
                HamachiSelection::Quit => HamachiSelection::Quit,
            };
        }
    }

    fn toggle_section(&mut self) {
        self.focused_section = match self.focused_section {
            FocusedSection::Hamachi => {
                if !self.network_rows.is_empty() {
                    if self.network_list_state.selected().is_none() {
                        self.network_list_state.select(Some(0));
                    }
                    FocusedSection::Networks
                } else {
                    FocusedSection::Hamachi
                }
            }
            FocusedSection::Networks => FocusedSection::Hamachi,
        };
    }

    async fn activate_selection(&mut self) {
        match self.focused_section {
            FocusedSection::Hamachi => {
                match self.hamachi_selection {
                    HamachiSelection::Nickname => {
                        self.input_buffer.clear();
                        self.input_mode = InputMode::Popup(PopupKind::SetNickname);
                    }
                    HamachiSelection::Logout => {
                        if self.client_status.is_logged_in() {
                            self.do_logout();
                        } else {
                            self.do_login();
                        }
                    }
                    HamachiSelection::Quit => {
                        self.should_quit = true;
                    }
                }
            }
            FocusedSection::Networks => {
                if !self.client_status.is_logged_in() {
                    return;
                }
                let Some(row) = self.selected_network_row().cloned() else {
                    return;
                };
                match row {
                    NetworkRow::Network(net_idx) => {
                        self.context_network_idx = Some(net_idx);
                        self.network_action_state.select(Some(0));
                        self.input_mode = InputMode::Popup(PopupKind::NetworkActions);
                    }
                    NetworkRow::Peer(net_idx, peer_idx) => {
                        self.context_network_idx = Some(net_idx);
                        self.context_peer_idx = Some(peer_idx);
                        self.peer_action_state.select(Some(0));
                        self.input_mode = InputMode::Popup(PopupKind::PeerActions);
                    }
                    NetworkRow::JoinNetwork => {
                        self.input_buffer.clear();
                        self.input_mode = InputMode::Popup(PopupKind::JoinNetwork);
                    }
                    NetworkRow::CreateNetwork => {
                        self.input_buffer.clear();
                        self.input_mode = InputMode::Popup(PopupKind::CreateNetwork);
                    }
                }
            }
        }
    }

    async fn handle_popup_key(&mut self, key: KeyEvent, kind: PopupKind) {
        match kind {
            PopupKind::NetworkActions => self.handle_network_actions_key(key).await,
            PopupKind::PeerActions => self.handle_peer_actions_key(key).await,
            PopupKind::JoinNetwork => self.handle_text_input_key(key, kind).await,
            PopupKind::JoinPassword => self.handle_text_input_key(key, kind).await,
            PopupKind::CreateNetwork => self.handle_text_input_key(key, kind).await,
            PopupKind::CreatePassword => self.handle_text_input_key(key, kind).await,
            PopupKind::SetNickname => self.handle_text_input_key(key, kind).await,
            PopupKind::SetPassword => self.handle_text_input_key(key, kind).await,
            PopupKind::AccessSelect => self.handle_access_select_key(key).await,
            PopupKind::Error => {
                match key.code {
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn handle_network_actions_key(&mut self, key: KeyEvent) {
        let actions = NetworkAction::all();
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(selected) = self.network_action_state.selected() {
                    if selected > 0 {
                        self.network_action_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(selected) = self.network_action_state.selected() {
                    if selected + 1 < actions.len() {
                        self.network_action_state.select(Some(selected + 1));
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(selected) = self.network_action_state.selected() {
                    if let Some(action) = actions.get(selected) {
                        self.execute_network_action(action.clone()).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn execute_network_action(&mut self, action: NetworkAction) {
        let Some(net_id) = self.context_network_id() else {
            return;
        };
        match action {
            NetworkAction::OnlineOffline => {
                let is_online = self.context_network().map(|n| n.is_online).unwrap_or(false);
                self.do_toggle_online(&net_id, is_online);
            }
            NetworkAction::Leave => {
                self.confirm_list_state.select(Some(0));
                self.input_mode = InputMode::Confirm(ConfirmAction::LeaveNetwork);
            }
            NetworkAction::Delete => {
                self.confirm_list_state.select(Some(0));
                self.input_mode = InputMode::Confirm(ConfirmAction::DeleteNetwork);
            }
            NetworkAction::Password => {
                self.input_buffer.clear();
                self.input_mode = InputMode::Popup(PopupKind::SetPassword);
            }
            NetworkAction::Access => {
                self.access_list_state.select(Some(0));
                self.input_mode = InputMode::Popup(PopupKind::AccessSelect);
            }
        }
    }

    async fn handle_peer_actions_key(&mut self, key: KeyEvent) {
        let actions = PeerAction::all();
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(selected) = self.peer_action_state.selected() {
                    if selected > 0 {
                        self.peer_action_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(selected) = self.peer_action_state.selected() {
                    if selected + 1 < actions.len() {
                        self.peer_action_state.select(Some(selected + 1));
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(selected) = self.peer_action_state.selected() {
                    if let Some(action) = actions.get(selected) {
                        self.execute_peer_action(action.clone()).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn execute_peer_action(&mut self, action: PeerAction) {
        match action {
            PeerAction::Evict => {
                self.confirm_list_state.select(Some(0));
                self.input_mode = InputMode::Confirm(ConfirmAction::EvictPeer);
            }
        }
    }

    async fn handle_text_input_key(&mut self, key: KeyEvent, kind: PopupKind) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.temp_buffer.clear();
            }
            KeyCode::Enter => self.submit_text_popup(kind).await,
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
    }

    async fn submit_text_popup(&mut self, kind: PopupKind) {
        let input = self.input_buffer.clone();

        match kind {
            PopupKind::JoinNetwork => {
                if !input.is_empty() {
                    // Move to password popup
                    self.temp_buffer = input;
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Popup(PopupKind::JoinPassword);
                    return;
                }
            }
            PopupKind::JoinPassword => {
                // temp_buffer has the network id, input has the password
                self.do_join_network(&self.temp_buffer.clone(), &input);
            }
            PopupKind::CreateNetwork => {
                if !input.is_empty() {
                    // Move to password popup
                    self.temp_buffer = input;
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Popup(PopupKind::CreatePassword);
                    return;
                }
            }
            PopupKind::CreatePassword => {
                // temp_buffer has the network name, input has the password
                self.do_create_network(&self.temp_buffer.clone(), &input);
            }
            PopupKind::SetNickname => {
                if !input.is_empty() {
                    self.do_set_nickname(&input);
                }
            }
            PopupKind::SetPassword => {
                if let Some(net_id) = self.context_network_id() {
                    self.do_set_password(&net_id, &input);
                }
            }
            _ => {}
        }
    }

    async fn handle_access_select_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j') => {
                let cur = self.access_list_state.selected().unwrap_or(0);
                self.access_list_state.select(Some(if cur == 0 { 1 } else { 0 }));
            }
            KeyCode::Enter => {
                let lock = self.access_list_state.selected().unwrap_or(0) == 0;
                if let Some(net_id) = self.context_network_id() {
                    self.do_set_access(&net_id, lock);
                }
            }
            _ => {}
        }
    }

    async fn handle_confirm_key(&mut self, key: KeyEvent, action: ConfirmAction) {
        match key.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j') => {
                let cur = self.confirm_list_state.selected().unwrap_or(0);
                self.confirm_list_state.select(Some(if cur == 0 { 1 } else { 0 }));
            }
            KeyCode::Enter => {
                if self.confirm_list_state.selected().unwrap_or(0) == 0 {
                    self.execute_confirm(action).await;
                } else {
                    self.input_mode = InputMode::Normal;
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    async fn execute_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeleteNetwork => {
                if let Some(net_id) = self.context_network_id() {
                    self.do_delete_network(&net_id);
                }
            }
            ConfirmAction::EvictPeer => {
                let net_id = self.context_network_id();
                let peer_id = self
                    .context_network_idx
                    .and_then(|ni| {
                        self.context_peer_idx.and_then(|pi| {
                            self.networks.get(ni).and_then(|n| {
                                n.peers.get(pi).map(|p| p.client_id.clone())
                            })
                        })
                    });
                if let (Some(net), Some(peer)) = (net_id, peer_id) {
                    self.do_evict(&net, &peer);
                }
            }
            ConfirmAction::LeaveNetwork => {
                if let Some(net_id) = self.context_network_id() {
                    self.do_leave_network(&net_id);
                }
            }
        }
    }

    fn show_error(&mut self, message: String) {
        self.error_message = message;
        self.input_mode = InputMode::Popup(PopupKind::Error);
    }

    fn set_loading(&mut self, message: &str) {
        self.loading = true;
        self.loading_message = message.to_string();
        self.loading_tick = 0;
    }

    fn clear_loading(&mut self) {
        self.loading = false;
        self.loading_message.clear();
    }

    /// Check if a background command has completed. Called from the main loop.
    pub async fn poll_command_result(&mut self) {
        if !self.loading {
            return;
        }
        match self.cmd_rx.try_recv() {
            Ok(result) => {
                self.clear_loading();
                if !result.success {
                    self.show_error(result.output);
                }
                self.refresh_status().await;
            }
            Err(_) => {
                // Still waiting
            }
        }
    }

    // --- Hamachi command wrappers ---

    fn do_login(&mut self) {
        if self.client_status.is_logged_in() {
            return;
        }
        self.set_loading("Logging in");
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::login().await;
            let _ = tx.send(result);
        });
    }

    fn do_logout(&mut self) {
        self.set_loading("Logging out");
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::logout().await;
            let _ = tx.send(result);
        });
    }

    fn do_create_network(&mut self, name: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        if password.is_empty() {
            self.show_error("Password cannot be blank".to_string());
            return;
        }
        self.set_loading("Creating network");
        let name = name.to_string();
        let password = password.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::create_network(&name, &password).await;
            let _ = tx.send(result);
        });
    }

    fn do_join_network(&mut self, name: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Joining network");
        let name = name.to_string();
        let password = password.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::join_network(&name, &password).await;
            let _ = tx.send(result);
        });
    }

    fn do_leave_network(&mut self, net_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Leaving network");
        let net_id = net_id.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::leave_network(&net_id).await;
            let _ = tx.send(result);
        });
    }

    fn do_delete_network(&mut self, net_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Deleting network");
        let net_id = net_id.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::delete_network(&net_id).await;
            let _ = tx.send(result);
        });
    }

    fn do_set_nickname(&mut self, nickname: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Setting nickname");
        let nickname = nickname.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::set_nickname(&nickname).await;
            let _ = tx.send(result);
        });
    }

    fn do_evict(&mut self, network: &str, client_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Evicting peer");
        let network = network.to_string();
        let client_id = client_id.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::evict(&network, &client_id).await;
            let _ = tx.send(result);
        });
    }

    fn do_set_password(&mut self, network: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Setting password");
        let network = network.to_string();
        let password = password.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::set_password(&network, &password).await;
            let _ = tx.send(result);
        });
    }

    fn do_set_access(&mut self, net_id: &str, lock: bool) {
        self.input_mode = InputMode::Normal;
        self.set_loading(if lock { "Locking" } else { "Unlocking" });
        let net_id = net_id.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = hamachi::set_access(&net_id, lock).await;
            let _ = tx.send(result);
        });
    }

    fn do_toggle_online(&mut self, net_id: &str, is_online: bool) {
        self.input_mode = InputMode::Normal;
        self.set_loading(if is_online { "Going offline" } else { "Going online" });
        let net_id = net_id.to_string();
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            let result = if is_online {
                hamachi::go_offline(&net_id).await
            } else {
                hamachi::go_online(&net_id).await
            };
            let _ = tx.send(result);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hamachi::{Network, Peer, PeerStatus};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_test_app() -> App {
        let mut app = App::new();
        app.client_status = ClientStatus {
            version: "2.1.0.203".to_string(),
            pid: "1234".to_string(),
            status: "logged in".to_string(),
            client_id: "090-123-456".to_string(),
            address: "25.10.20.30".to_string(),
            nickname: "test-box".to_string(),
            lmi_account: "test@example.com".to_string(),
        };
        app.networks = vec![
            Network {
                id: "net-1".to_string(),
                name: "Network-One".to_string(),
                capacity: "2/5".to_string(),
                owner: "a@b.com".to_string(),
                is_online: true,
                peers: vec![
                    Peer {
                        client_id: "090-111-111".to_string(),
                        nickname: "peer-a".to_string(),
                        address: "25.10.1.1".to_string(),
                        status: PeerStatus::Online,
                        connection_type: "direct".to_string(),
                    },
                    Peer {
                        client_id: "090-222-222".to_string(),
                        nickname: "peer-b".to_string(),
                        address: "25.10.1.2".to_string(),
                        status: PeerStatus::Online,
                        connection_type: "relay".to_string(),
                    },
                ],
            },
            Network {
                id: "net-2".to_string(),
                name: "Network-Two".to_string(),
                capacity: "1/5".to_string(),
                owner: "c@d.com".to_string(),
                is_online: false,
                peers: vec![Peer {
                    client_id: "090-333-333".to_string(),
                    nickname: "peer-c".to_string(),
                    address: "25.10.2.1".to_string(),
                    status: PeerStatus::Offline,
                    connection_type: "offline".to_string(),
                }],
            },
        ];
        app.rebuild_network_rows();
        app.network_list_state.select(Some(0));
        app
    }

    fn make_logged_out_app() -> App {
        let mut app = App::new();
        app.client_status = ClientStatus::default();
        app
    }

    // ===== Initial state tests =====

    #[test]
    fn new_app_defaults() {
        let app = App::new();
        assert!(!app.should_quit);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
        assert_eq!(app.hamachi_selection, HamachiSelection::Nickname);
        assert!(app.networks.is_empty());
        assert!(!app.loading);
        // Only JoinNetwork and CreateNetwork rows when no networks
        assert_eq!(app.network_rows.len(), 2);
        assert_eq!(app.network_rows[0], NetworkRow::JoinNetwork);
        assert_eq!(app.network_rows[1], NetworkRow::CreateNetwork);
    }

    // ===== Network rows rebuild =====

    #[test]
    fn rebuild_network_rows_correct() {
        let app = make_test_app();
        // Network-One, peer-a, peer-b, Network-Two, peer-c, JoinNetwork, CreateNetwork
        assert_eq!(app.network_rows.len(), 7);
        assert_eq!(app.network_rows[0], NetworkRow::Network(0));
        assert_eq!(app.network_rows[1], NetworkRow::Peer(0, 0));
        assert_eq!(app.network_rows[2], NetworkRow::Peer(0, 1));
        assert_eq!(app.network_rows[3], NetworkRow::Network(1));
        assert_eq!(app.network_rows[4], NetworkRow::Peer(1, 0));
        assert_eq!(app.network_rows[5], NetworkRow::JoinNetwork);
        assert_eq!(app.network_rows[6], NetworkRow::CreateNetwork);
    }

    // ===== Navigation tests =====

    #[test]
    fn navigate_down_from_hamachi_to_networks() {
        let mut app = make_test_app();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
        app.navigate_down();
        assert_eq!(app.focused_section, FocusedSection::Networks);
        assert_eq!(app.network_list_state.selected(), Some(0));
    }

    #[test]
    fn navigate_up_from_networks_top_to_hamachi() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(0));
        app.navigate_up();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
    }

    #[test]
    fn navigate_down_within_networks() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(0));
        app.navigate_down();
        assert_eq!(app.network_list_state.selected(), Some(1));
        app.navigate_down();
        assert_eq!(app.network_list_state.selected(), Some(2));
    }

    #[test]
    fn navigate_up_within_networks() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(3));
        app.navigate_up();
        assert_eq!(app.network_list_state.selected(), Some(2));
    }

    #[test]
    fn navigate_down_at_bottom_stays() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(6)); // CreateNetwork = last
        app.navigate_down();
        assert_eq!(app.network_list_state.selected(), Some(6));
    }

    #[test]
    fn navigate_up_at_hamachi_top_stays() {
        let mut app = make_test_app();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
        app.navigate_up();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
    }

    // ===== Hamachi left/right navigation =====

    #[test]
    fn hamachi_left_right_navigation() {
        let mut app = make_test_app();
        assert_eq!(app.hamachi_selection, HamachiSelection::Nickname);
        app.navigate_right();
        assert_eq!(app.hamachi_selection, HamachiSelection::Logout);
        app.navigate_right();
        assert_eq!(app.hamachi_selection, HamachiSelection::Quit);
        app.navigate_right();
        assert_eq!(app.hamachi_selection, HamachiSelection::Quit); // stays
        app.navigate_left();
        assert_eq!(app.hamachi_selection, HamachiSelection::Logout);
        app.navigate_left();
        assert_eq!(app.hamachi_selection, HamachiSelection::Nickname);
        app.navigate_left();
        assert_eq!(app.hamachi_selection, HamachiSelection::Nickname); // stays
    }

    // ===== Tab toggle =====

    #[test]
    fn tab_toggles_sections() {
        let mut app = make_test_app();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
        app.toggle_section();
        assert_eq!(app.focused_section, FocusedSection::Networks);
        app.toggle_section();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
    }

    #[test]
    fn tab_with_no_networks_stays_hamachi() {
        let mut app = make_logged_out_app();
        app.networks.clear();
        app.network_rows.clear();
        app.toggle_section();
        assert_eq!(app.focused_section, FocusedSection::Hamachi);
    }

    // ===== Popup transitions =====

    #[test]
    fn enter_on_network_opens_network_actions() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(0)); // Network(0)

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::NetworkActions));
        assert_eq!(app.context_network_idx, Some(0));
    }

    #[test]
    fn enter_on_peer_opens_peer_actions() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(1)); // Peer(0, 0)

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::PeerActions));
        assert_eq!(app.context_network_idx, Some(0));
        assert_eq!(app.context_peer_idx, Some(0));
    }

    #[test]
    fn enter_on_join_network_opens_popup() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(5)); // JoinNetwork

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::JoinNetwork));
    }

    #[test]
    fn enter_on_create_network_opens_popup() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Networks;
        app.network_list_state.select(Some(6)); // CreateNetwork

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::CreateNetwork));
    }

    #[test]
    fn enter_on_nickname_opens_nickname_popup() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Hamachi;
        app.hamachi_selection = HamachiSelection::Nickname;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::SetNickname));
    }

    #[test]
    fn enter_on_quit_sets_should_quit() {
        let mut app = make_test_app();
        app.focused_section = FocusedSection::Hamachi;
        app.hamachi_selection = HamachiSelection::Quit;

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });

        assert!(app.should_quit);
    }

    // ===== Join/Create popup tests =====

    #[test]
    fn join_network_enter_opens_password() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::JoinNetwork);
        app.input_buffer = "420-656-988".to_string();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Enter), PopupKind::JoinNetwork).await;
        });
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::JoinPassword));
        assert_eq!(app.temp_buffer, "420-656-988");
        assert_eq!(app.input_buffer, "");
    }

    #[test]
    fn join_network_empty_stays() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::JoinNetwork);
        app.input_buffer.clear();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Enter), PopupKind::JoinNetwork).await;
        });
        // Should not transition (input was empty)
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::JoinNetwork));
    }

    #[test]
    fn create_network_enter_opens_password() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetwork);
        app.input_buffer = "my-net".to_string();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Enter), PopupKind::CreateNetwork).await;
        });
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::CreatePassword));
        assert_eq!(app.temp_buffer, "my-net");
        assert_eq!(app.input_buffer, "");
    }

    #[test]
    fn join_network_esc_closes() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::JoinNetwork);
        app.input_buffer = "something".to_string();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Esc), PopupKind::JoinNetwork).await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input_buffer, "");
    }

    // ===== Network actions popup =====

    #[test]
    fn network_actions_esc_closes() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::NetworkActions);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_network_actions_key(make_key(KeyCode::Esc)).await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn network_actions_navigate() {
        let mut app = make_test_app();
        app.network_action_state.select(Some(0));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_network_actions_key(make_key(KeyCode::Down)).await;
        });
        assert_eq!(app.network_action_state.selected(), Some(1));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_network_actions_key(make_key(KeyCode::Up)).await;
        });
        assert_eq!(app.network_action_state.selected(), Some(0));
    }

    // ===== Text input popup =====

    #[test]
    fn text_input_esc_closes() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::SetNickname);
        app.input_buffer = "test".to_string();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Esc), PopupKind::SetNickname).await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input_buffer, "");
    }

    #[test]
    fn text_input_typing() {
        let mut app = make_test_app();
        app.input_buffer.clear();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Char('h')), PopupKind::SetNickname).await;
            app.handle_text_input_key(make_key(KeyCode::Char('i')), PopupKind::SetNickname).await;
        });
        assert_eq!(app.input_buffer, "hi");
    }

    #[test]
    fn text_input_backspace() {
        let mut app = make_test_app();
        app.input_buffer = "hello".to_string();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_text_input_key(make_key(KeyCode::Backspace), PopupKind::SetNickname).await;
        });
        assert_eq!(app.input_buffer, "hell");
    }

    // ===== Confirm dialog =====

    #[test]
    fn confirm_no_returns_to_normal() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::DeleteNetwork);
        app.confirm_list_state.select(Some(1)); // No

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Enter), ConfirmAction::DeleteNetwork)
                .await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn confirm_esc_returns_to_normal() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::LeaveNetwork);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Esc), ConfirmAction::LeaveNetwork)
                .await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn confirm_up_down_toggles_selection() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::DeleteNetwork);
        app.confirm_list_state.select(Some(0));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Down), ConfirmAction::DeleteNetwork)
                .await;
        });
        assert_eq!(app.confirm_list_state.selected(), Some(1));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Up), ConfirmAction::DeleteNetwork)
                .await;
        });
        assert_eq!(app.confirm_list_state.selected(), Some(0));
    }

    // ===== Error popup =====

    #[test]
    fn error_popup_enter_dismisses() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::Error);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Enter), PopupKind::Error).await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn error_popup_esc_dismisses() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::Error);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Esc), PopupKind::Error).await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    // ===== Ctrl+C =====

    #[test]
    fn ctrl_c_quits() {
        let mut app = make_test_app();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            app.handle_key(key).await;
        });
        assert!(app.should_quit);
    }

    // ===== Loading state =====

    #[test]
    fn set_loading_and_clear() {
        let mut app = App::new();
        app.set_loading("test");
        assert!(app.loading);
        assert_eq!(app.loading_message, "test");
        app.clear_loading();
        assert!(!app.loading);
    }

    #[test]
    fn loading_blocks_normal_key_input() {
        let mut app = make_test_app();
        app.loading = true;
        let initial = app.focused_section.clone();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_normal_key(make_key(KeyCode::Down)).await;
        });
        assert_eq!(app.focused_section, initial);
    }

    // ===== Show error =====

    #[test]
    fn show_error_sets_mode_and_message() {
        let mut app = App::new();
        app.show_error("test error".to_string());
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::Error));
        assert_eq!(app.error_message, "test error");
    }

    // ===== Logged out networks disabled =====

    #[test]
    fn logged_out_network_enter_does_nothing() {
        let mut app = make_logged_out_app();
        app.focused_section = FocusedSection::Networks;
        app.network_rows = vec![NetworkRow::JoinNetwork, NetworkRow::CreateNetwork];
        app.network_list_state.select(Some(0));

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.activate_selection().await;
        });
        assert_eq!(app.input_mode, InputMode::Normal);
    }
}
