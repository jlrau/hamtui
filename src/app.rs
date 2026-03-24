use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::event::Event;
use crate::hamachi::{self, ClientStatus, Network};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Popup(PopupKind),
    Confirm(ConfirmAction),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PopupKind {
    CreateNetworkName,
    CreateNetworkPassword,
    JoinNetworkName,
    JoinNetworkPassword,
    SetNickname,
    SetPassword,
    EvictSelectPeer,
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
pub enum FocusedPanel {
    Actions,
    Networks,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Login,
    Logout,
    Create,
    Join,
    Leave,
    Delete,
    Nickname,
    Evict,
    Password,
    Access,
    OnlineOffline,
    Quit,
}

impl Action {
    pub fn label(&self) -> &str {
        match self {
            Action::Login => "Login",
            Action::Logout => "Logout",
            Action::Create => "Create",
            Action::Join => "Join",
            Action::Leave => "Leave",
            Action::Delete => "Delete",
            Action::Nickname => "Nickname",
            Action::Evict => "Evict",
            Action::Password => "Password",
            Action::Access => "Access",
            Action::OnlineOffline => "Online/Offline",
            Action::Quit => "Quit",
        }
    }
}

pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub focused_panel: FocusedPanel,
    pub client_status: ClientStatus,
    pub networks: Vec<Network>,
    pub action_list_state: ListState,
    pub network_list_state: ListState,
    pub peer_list_state: ListState,
    pub input_buffer: String,
    pub temp_buffer: String,
    pub error_message: String,
    pub loading: bool,
    pub loading_message: String,
    pub access_selection: usize, // 0 = Lock, 1 = Unlock
    pub last_refresh: Instant,
}

impl App {
    pub fn new() -> Self {
        let mut action_list_state = ListState::default();
        action_list_state.select(Some(0));
        let mut network_list_state = ListState::default();
        network_list_state.select(Some(0));
        let mut peer_list_state = ListState::default();
        peer_list_state.select(Some(0));

        Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            focused_panel: FocusedPanel::Actions,
            client_status: ClientStatus::default(),
            networks: Vec::new(),
            action_list_state,
            network_list_state,
            peer_list_state,
            input_buffer: String::new(),
            temp_buffer: String::new(),
            error_message: String::new(),
            loading: false,
            loading_message: String::new(),
            access_selection: 0,
            last_refresh: Instant::now() - AUTO_REFRESH_INTERVAL,
        }
    }

    pub fn available_actions(&self) -> Vec<Action> {
        if self.client_status.is_logged_in() {
            vec![
                Action::Create,
                Action::Join,
                Action::OnlineOffline,
                Action::Leave,
                Action::Delete,
                Action::Evict,
                Action::Password,
                Action::Access,
                Action::Nickname,
                Action::Logout,
                Action::Quit,
            ]
        } else {
            vec![Action::Login, Action::Quit]
        }
    }

    pub fn selected_action(&self) -> Option<Action> {
        self.action_list_state
            .selected()
            .and_then(|i| self.available_actions().get(i).cloned())
    }

    pub fn selected_network(&self) -> Option<&Network> {
        self.network_list_state
            .selected()
            .and_then(|i| self.networks.get(i))
    }

    pub fn selected_network_id(&self) -> Option<String> {
        self.selected_network().map(|n| n.id.clone())
    }

    pub async fn refresh_status(&mut self) {
        self.client_status = hamachi::get_status().await;
        self.networks = hamachi::get_networks().await;

        if !self.networks.is_empty() {
            if let Some(selected) = self.network_list_state.selected() {
                if selected >= self.networks.len() {
                    self.network_list_state.select(Some(self.networks.len() - 1));
                }
            }
        } else {
            self.network_list_state.select(None);
        }

        self.clamp_peer_selection();
        self.clamp_action_selection();
        self.last_refresh = Instant::now();
    }

    fn clamp_peer_selection(&mut self) {
        if let Some(network) = self.selected_network() {
            if network.peers.is_empty() {
                self.peer_list_state.select(None);
            } else if let Some(selected) = self.peer_list_state.selected() {
                if selected >= network.peers.len() {
                    self.peer_list_state.select(Some(network.peers.len() - 1));
                }
            }
        } else {
            self.peer_list_state.select(None);
        }
    }

    fn clamp_action_selection(&mut self) {
        let actions = self.available_actions();
        if let Some(selected) = self.action_list_state.selected() {
            if selected >= actions.len() {
                self.action_list_state.select(Some(actions.len().saturating_sub(1)));
            }
        }
    }

    pub async fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key).await,
            Event::Tick => {
                if self.last_refresh.elapsed() >= AUTO_REFRESH_INTERVAL && !self.loading {
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
            KeyCode::Up => self.navigate_up(),
            KeyCode::Down => self.navigate_down(),
            KeyCode::Left => self.focused_panel = FocusedPanel::Actions,
            KeyCode::Right => {
                if !self.networks.is_empty() {
                    self.focused_panel = FocusedPanel::Networks;
                }
            }
            KeyCode::Tab => self.toggle_focus(),
            KeyCode::Enter => self.execute_selected_action().await,
            _ => {}
        }
    }

    async fn execute_selected_action(&mut self) {
        if self.focused_panel != FocusedPanel::Actions {
            return;
        }

        let Some(action) = self.selected_action() else {
            return;
        };

        match action {
            Action::Login => self.do_login().await,
            Action::Logout => self.do_logout().await,
            Action::Create => {
                self.input_buffer.clear();
                self.temp_buffer.clear();
                self.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);
            }
            Action::Join => {
                self.input_buffer.clear();
                self.temp_buffer.clear();
                self.input_mode = InputMode::Popup(PopupKind::JoinNetworkName);
            }
            Action::Leave => {
                if self.selected_network().is_some() {
                    self.input_mode = InputMode::Confirm(ConfirmAction::LeaveNetwork);
                }
            }
            Action::Delete => {
                if self.selected_network().is_some() {
                    self.input_mode = InputMode::Confirm(ConfirmAction::DeleteNetwork);
                }
            }
            Action::Nickname => {
                self.input_buffer.clear();
                self.input_mode = InputMode::Popup(PopupKind::SetNickname);
            }
            Action::Evict => {
                if let Some(net) = self.selected_network() {
                    if !net.peers.is_empty() {
                        self.peer_list_state.select(Some(0));
                        self.input_mode = InputMode::Popup(PopupKind::EvictSelectPeer);
                    }
                }
            }
            Action::Password => {
                if self.selected_network().is_some() {
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Popup(PopupKind::SetPassword);
                }
            }
            Action::Access => {
                if self.selected_network().is_some() {
                    self.access_selection = 0;
                    self.input_mode = InputMode::Popup(PopupKind::AccessSelect);
                }
            }
            Action::OnlineOffline => self.do_toggle_online().await,
            Action::Quit => self.should_quit = true,
        }
    }

    async fn handle_popup_key(&mut self, key: KeyEvent, kind: PopupKind) {
        if kind == PopupKind::EvictSelectPeer {
            self.handle_evict_select_key(key).await;
            return;
        }

        if kind == PopupKind::AccessSelect {
            self.handle_access_select_key(key).await;
            return;
        }

        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.temp_buffer.clear();
            }
            KeyCode::Enter => self.submit_popup(kind.clone()).await,
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }

        if kind == PopupKind::Error && key.code != KeyCode::Esc {
            if key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
                self.input_mode = InputMode::Normal;
            }
        }
    }

    async fn handle_evict_select_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up => {
                if let Some(selected) = self.peer_list_state.selected() {
                    if selected > 0 {
                        self.peer_list_state.select(Some(selected - 1));
                    }
                }
            }
            KeyCode::Down => {
                if let Some(network) = self.selected_network() {
                    let max = network.peers.len().saturating_sub(1);
                    if let Some(selected) = self.peer_list_state.selected() {
                        if selected < max {
                            self.peer_list_state.select(Some(selected + 1));
                        }
                    }
                }
            }
            KeyCode::Enter => {
                if self.peer_list_state.selected().is_some() {
                    self.input_mode = InputMode::Confirm(ConfirmAction::EvictPeer);
                }
            }
            _ => {}
        }
    }

    async fn handle_confirm_key(&mut self, key: KeyEvent, action: ConfirmAction) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.execute_confirm(action).await;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    async fn handle_access_select_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Up | KeyCode::Down => {
                self.access_selection = if self.access_selection == 0 { 1 } else { 0 };
            }
            KeyCode::Enter => {
                let lock = self.access_selection == 0;
                self.input_mode = InputMode::Normal;
                if let Some(net_id) = self.selected_network_id() {
                    self.set_loading(if lock { "Locking..." } else { "Unlocking..." });
                    let result = hamachi::set_access(&net_id, lock).await;
                    self.clear_loading();
                    if !result.success {
                        self.show_error(format!("Set access failed: {}", result.output));
                    }
                    self.refresh_status().await;
                }
            }
            _ => {}
        }
    }

    async fn submit_popup(&mut self, kind: PopupKind) {
        let input = self.input_buffer.clone();

        match kind {
            PopupKind::CreateNetworkName => {
                if !input.is_empty() {
                    self.temp_buffer = input;
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Popup(PopupKind::CreateNetworkPassword);
                }
            }
            PopupKind::CreateNetworkPassword => {
                self.do_create_network(&self.temp_buffer.clone(), &input).await;
            }
            PopupKind::JoinNetworkName => {
                if !input.is_empty() {
                    self.temp_buffer = input;
                    self.input_buffer.clear();
                    self.input_mode = InputMode::Popup(PopupKind::JoinNetworkPassword);
                }
            }
            PopupKind::JoinNetworkPassword => {
                self.do_join_network(&self.temp_buffer.clone(), &input).await;
            }
            PopupKind::SetNickname => {
                if !input.is_empty() {
                    self.do_set_nickname(&input).await;
                }
            }
            PopupKind::SetPassword => {
                if let Some(net_id) = self.selected_network_id() {
                    self.do_set_password(&net_id, &input).await;
                }
            }
            PopupKind::EvictSelectPeer => {
                // Handled by handle_evict_select_key
            }
            PopupKind::AccessSelect => {
                // Handled by handle_access_select_key
            }
            PopupKind::Error => {
                self.input_mode = InputMode::Normal;
            }
        }
    }

    async fn execute_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeleteNetwork => {
                if let Some(net_id) = self.selected_network_id() {
                    self.do_delete_network(&net_id).await;
                }
            }
            ConfirmAction::EvictPeer => {
                let net_id = self.selected_network_id();
                let peer_id = self.selected_network().and_then(|n| {
                    self.peer_list_state
                        .selected()
                        .and_then(|i| n.peers.get(i))
                        .map(|p| p.client_id.clone())
                });
                if let (Some(net), Some(peer)) = (net_id, peer_id) {
                    self.do_evict(&net, &peer).await;
                }
            }
            ConfirmAction::LeaveNetwork => {
                if let Some(net_id) = self.selected_network_id() {
                    self.do_leave_network(&net_id).await;
                }
            }
        }
    }

    fn navigate_up(&mut self) {
        match self.focused_panel {
            FocusedPanel::Actions => {
                if let Some(selected) = self.action_list_state.selected() {
                    if selected > 0 {
                        self.action_list_state.select(Some(selected - 1));
                    }
                }
            }
            FocusedPanel::Networks => {
                if let Some(selected) = self.network_list_state.selected() {
                    if selected > 0 {
                        self.network_list_state.select(Some(selected - 1));
                        self.peer_list_state.select(Some(0));
                    }
                }
            }
        }
    }

    fn navigate_down(&mut self) {
        match self.focused_panel {
            FocusedPanel::Actions => {
                let actions = self.available_actions();
                if let Some(selected) = self.action_list_state.selected() {
                    if selected + 1 < actions.len() {
                        self.action_list_state.select(Some(selected + 1));
                    }
                }
            }
            FocusedPanel::Networks => {
                if let Some(selected) = self.network_list_state.selected() {
                    if selected + 1 < self.networks.len() {
                        self.network_list_state.select(Some(selected + 1));
                        self.peer_list_state.select(Some(0));
                    }
                }
            }
        }
    }

    fn toggle_focus(&mut self) {
        self.focused_panel = match self.focused_panel {
            FocusedPanel::Actions => {
                if !self.networks.is_empty() {
                    FocusedPanel::Networks
                } else {
                    FocusedPanel::Actions
                }
            }
            FocusedPanel::Networks => FocusedPanel::Actions,
        };
    }

    fn show_error(&mut self, message: String) {
        self.error_message = message;
        self.input_mode = InputMode::Popup(PopupKind::Error);
    }

    fn set_loading(&mut self, message: &str) {
        self.loading = true;
        self.loading_message = message.to_string();
    }

    fn clear_loading(&mut self) {
        self.loading = false;
        self.loading_message.clear();
    }

    // --- Hamachi command wrappers ---

    async fn do_login(&mut self) {
        if self.client_status.is_logged_in() {
            return;
        }
        self.set_loading("Logging in...");
        let result = hamachi::login().await;
        self.clear_loading();
        if !result.success {
            self.show_error(format!("Login failed: {}", result.output));
        }
        self.refresh_status().await;
    }

    async fn do_logout(&mut self) {
        self.set_loading("Logging out...");
        let result = hamachi::logout().await;
        self.clear_loading();
        if !result.success {
            self.show_error(format!("Logout failed: {}", result.output));
        }
        self.refresh_status().await;
    }

    async fn do_toggle_online(&mut self) {
        if let Some(network) = self.selected_network() {
            let net_id = network.id.clone();
            let is_online = network.is_online;
            self.set_loading(if is_online { "Going offline..." } else { "Going online..." });
            let result = if is_online {
                hamachi::go_offline(&net_id).await
            } else {
                hamachi::go_online(&net_id).await
            };
            self.clear_loading();
            if !result.success {
                self.show_error(format!("Failed: {}", result.output));
            }
            self.refresh_status().await;
        }
    }

    async fn do_create_network(&mut self, name: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Creating network...");
        let result = hamachi::create_network(name, password).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Create failed: {}", result.output));
        }
    }

    async fn do_join_network(&mut self, name: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Joining network...");
        let result = hamachi::join_network(name, password).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Join failed: {}", result.output));
        }
    }

    async fn do_leave_network(&mut self, net_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Leaving network...");
        let result = hamachi::leave_network(net_id).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Leave failed: {}", result.output));
        }
    }

    async fn do_delete_network(&mut self, net_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Deleting network...");
        let result = hamachi::delete_network(net_id).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Delete failed: {}", result.output));
        }
    }

    async fn do_set_nickname(&mut self, nickname: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Setting nickname...");
        let result = hamachi::set_nickname(nickname).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Set nickname failed: {}", result.output));
        }
    }

    async fn do_evict(&mut self, network: &str, client_id: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Evicting peer...");
        let result = hamachi::evict(network, client_id).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Evict failed: {}", result.output));
        }
    }

    async fn do_set_password(&mut self, network: &str, password: &str) {
        self.input_mode = InputMode::Normal;
        self.set_loading("Setting password...");
        let result = hamachi::set_password(network, password).await;
        self.clear_loading();
        if result.success {
            self.refresh_status().await;
        } else {
            self.show_error(format!("Set password failed: {}", result.output));
        }
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
        app.network_list_state.select(Some(0));
        app.peer_list_state.select(Some(0));
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
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
        assert!(app.networks.is_empty());
        assert!(!app.loading);
        assert_eq!(app.action_list_state.selected(), Some(0));
    }

    // ===== Available actions tests =====

    #[test]
    fn available_actions_logged_in() {
        let app = make_test_app();
        let actions = app.available_actions();
        assert_eq!(actions[0], Action::Create);
        assert_eq!(actions.last(), Some(&Action::Quit));
        assert_eq!(actions.len(), 11);
        assert!(!actions.contains(&Action::Login));
    }

    #[test]
    fn available_actions_logged_out() {
        let app = make_logged_out_app();
        let actions = app.available_actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], Action::Login);
        assert_eq!(actions[1], Action::Quit);
    }

    // ===== Navigation tests =====

    #[test]
    fn navigate_down_actions() {
        let mut app = make_test_app();
        assert_eq!(app.action_list_state.selected(), Some(0));
        app.navigate_down();
        assert_eq!(app.action_list_state.selected(), Some(1));
        app.navigate_down();
        assert_eq!(app.action_list_state.selected(), Some(2));
    }

    #[test]
    fn navigate_up_actions_at_top() {
        let mut app = make_test_app();
        assert_eq!(app.action_list_state.selected(), Some(0));
        app.navigate_up();
        assert_eq!(app.action_list_state.selected(), Some(0)); // stays at 0
    }

    #[test]
    fn navigate_down_actions_at_bottom() {
        let mut app = make_test_app();
        let max = app.available_actions().len() - 1;
        app.action_list_state.select(Some(max));
        app.navigate_down();
        assert_eq!(app.action_list_state.selected(), Some(max)); // stays at bottom
    }

    #[test]
    fn navigate_down_networks() {
        let mut app = make_test_app();
        app.focused_panel = FocusedPanel::Networks;
        assert_eq!(app.network_list_state.selected(), Some(0));
        app.navigate_down();
        assert_eq!(app.network_list_state.selected(), Some(1));
    }

    #[test]
    fn navigate_down_networks_at_bottom() {
        let mut app = make_test_app();
        app.focused_panel = FocusedPanel::Networks;
        app.network_list_state.select(Some(1)); // last network
        app.navigate_down();
        assert_eq!(app.network_list_state.selected(), Some(1)); // stays
    }

    #[test]
    fn navigate_up_networks() {
        let mut app = make_test_app();
        app.focused_panel = FocusedPanel::Networks;
        app.network_list_state.select(Some(1));
        app.navigate_up();
        assert_eq!(app.network_list_state.selected(), Some(0));
        // Peer selection resets when changing network
        assert_eq!(app.peer_list_state.selected(), Some(0));
    }

    // ===== Focus switching tests =====

    #[test]
    fn toggle_focus_actions_to_networks() {
        let mut app = make_test_app();
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
        app.toggle_focus();
        assert_eq!(app.focused_panel, FocusedPanel::Networks);
    }

    #[test]
    fn toggle_focus_networks_to_actions() {
        let mut app = make_test_app();
        app.focused_panel = FocusedPanel::Networks;
        app.toggle_focus();
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
    }

    #[test]
    fn toggle_focus_no_networks_stays_on_actions() {
        let mut app = make_logged_out_app();
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
        app.toggle_focus();
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
    }

    #[test]
    fn left_right_focus() {
        let mut app = make_test_app();
        // Start on actions, right goes to networks
        app.handle_normal_key_sync(make_key(KeyCode::Right));
        assert_eq!(app.focused_panel, FocusedPanel::Networks);
        // Left goes back to actions
        app.handle_normal_key_sync(make_key(KeyCode::Left));
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
    }

    #[test]
    fn right_with_no_networks_stays() {
        let mut app = make_logged_out_app();
        app.handle_normal_key_sync(make_key(KeyCode::Right));
        assert_eq!(app.focused_panel, FocusedPanel::Actions);
    }

    // ===== Selected network/action tests =====

    #[test]
    fn selected_network_valid() {
        let app = make_test_app();
        let net = app.selected_network().unwrap();
        assert_eq!(net.name, "Network-One");
    }

    #[test]
    fn selected_network_none_when_empty() {
        let mut app = make_logged_out_app();
        app.network_list_state.select(None);
        assert!(app.selected_network().is_none());
    }

    #[test]
    fn selected_action_matches_index() {
        let mut app = make_test_app();
        app.action_list_state.select(Some(0));
        assert_eq!(app.selected_action(), Some(Action::Create));
        app.action_list_state.select(Some(1));
        assert_eq!(app.selected_action(), Some(Action::Join));
    }

    // ===== Input mode transitions =====

    #[test]
    fn popup_create_opens() {
        let mut app = make_test_app();
        app.action_list_state.select(Some(1)); // Create
        // Can't call async, but we can test mode transition directly
        app.input_buffer = "something".to_string();
        app.temp_buffer = "old".to_string();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);
        // Simulate clearing (as execute_selected_action would)
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::CreateNetworkName));
    }

    #[test]
    fn popup_esc_returns_to_normal() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);
        app.input_buffer = "test".to_string();
        app.temp_buffer = "temp".to_string();

        // Simulate ESC in popup
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Esc), PopupKind::CreateNetworkName).await;
        });

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input_buffer, "");
        assert_eq!(app.temp_buffer, "");
    }

    #[test]
    fn popup_typing_appends_to_buffer() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::SetNickname);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Char('h')), PopupKind::SetNickname).await;
            app.handle_popup_key(make_key(KeyCode::Char('i')), PopupKind::SetNickname).await;
        });

        assert_eq!(app.input_buffer, "hi");
    }

    #[test]
    fn popup_backspace_removes_char() {
        let mut app = make_test_app();
        app.input_buffer = "hello".to_string();
        app.input_mode = InputMode::Popup(PopupKind::SetNickname);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Backspace), PopupKind::SetNickname).await;
        });

        assert_eq!(app.input_buffer, "hell");
    }

    #[test]
    fn popup_backspace_on_empty_buffer() {
        let mut app = make_test_app();
        app.input_buffer.clear();
        app.input_mode = InputMode::Popup(PopupKind::SetNickname);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_popup_key(make_key(KeyCode::Backspace), PopupKind::SetNickname).await;
        });

        assert_eq!(app.input_buffer, "");
    }

    #[test]
    fn popup_create_name_enter_moves_to_password() {
        let mut app = make_test_app();
        app.input_buffer = "my-network".to_string();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.submit_popup(PopupKind::CreateNetworkName).await;
        });

        assert_eq!(app.temp_buffer, "my-network");
        assert_eq!(app.input_buffer, "");
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::CreateNetworkPassword));
    }

    #[test]
    fn popup_create_name_empty_stays() {
        let mut app = make_test_app();
        app.input_buffer.clear();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.submit_popup(PopupKind::CreateNetworkName).await;
        });

        // Should stay on same popup since name is empty
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::CreateNetworkName));
    }

    #[test]
    fn popup_join_name_enter_moves_to_password() {
        let mut app = make_test_app();
        app.input_buffer = "join-net".to_string();
        app.input_mode = InputMode::Popup(PopupKind::JoinNetworkName);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.submit_popup(PopupKind::JoinNetworkName).await;
        });

        assert_eq!(app.temp_buffer, "join-net");
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::JoinNetworkPassword));
    }

    #[test]
    fn popup_set_nickname_empty_no_action() {
        let mut app = make_test_app();
        app.input_buffer.clear();
        app.input_mode = InputMode::Popup(PopupKind::SetNickname);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.submit_popup(PopupKind::SetNickname).await;
        });

        // Should remain in the popup since nothing was submitted
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::SetNickname));
    }

    // ===== Confirm dialog tests =====

    #[test]
    fn confirm_n_returns_to_normal() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::DeleteNetwork);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Char('n')), ConfirmAction::DeleteNetwork).await;
        });

        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn confirm_esc_returns_to_normal() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::LeaveNetwork);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Esc), ConfirmAction::LeaveNetwork).await;
        });

        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn confirm_random_key_stays() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Confirm(ConfirmAction::EvictPeer);

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_confirm_key(make_key(KeyCode::Char('x')), ConfirmAction::EvictPeer).await;
        });

        assert_eq!(app.input_mode, InputMode::Confirm(ConfirmAction::EvictPeer));
    }

    // ===== Error popup dismiss =====

    #[test]
    fn error_popup_enter_dismisses() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::Error);
        app.error_message = "Something failed".to_string();

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

    // ===== Ctrl+C quit =====

    #[test]
    fn ctrl_c_quits() {
        let mut app = make_test_app();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
            app.handle_key(key).await;
        });

        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits_from_popup() {
        let mut app = make_test_app();
        app.input_mode = InputMode::Popup(PopupKind::CreateNetworkName);

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
        assert!(!app.loading);

        app.set_loading("test");
        assert!(app.loading);
        assert_eq!(app.loading_message, "test");

        app.clear_loading();
        assert!(!app.loading);
        assert_eq!(app.loading_message, "");
    }

    #[test]
    fn loading_blocks_normal_key_input() {
        let mut app = make_test_app();
        app.loading = true;
        let initial_selection = app.action_list_state.selected();

        tokio::runtime::Runtime::new().unwrap().block_on(async {
            app.handle_normal_key(make_key(KeyCode::Down)).await;
        });

        // Selection should not change while loading
        assert_eq!(app.action_list_state.selected(), initial_selection);
    }

    // ===== Show error =====

    #[test]
    fn show_error_sets_mode_and_message() {
        let mut app = App::new();
        app.show_error("test error".to_string());
        assert_eq!(app.input_mode, InputMode::Popup(PopupKind::Error));
        assert_eq!(app.error_message, "test error");
    }

    // ===== Clamp selection tests =====

    #[test]
    fn clamp_peer_selection_empty_peers() {
        let mut app = make_test_app();
        app.networks[0].peers.clear();
        app.peer_list_state.select(Some(5));
        app.clamp_peer_selection();
        assert_eq!(app.peer_list_state.selected(), None);
    }

    #[test]
    fn clamp_peer_selection_out_of_bounds() {
        let mut app = make_test_app();
        app.peer_list_state.select(Some(99));
        app.clamp_peer_selection();
        assert_eq!(app.peer_list_state.selected(), Some(1)); // 2 peers, clamped to 1
    }

    #[test]
    fn clamp_action_selection_after_logout() {
        let mut app = make_test_app();
        // Logged in has 11 actions, select the last one
        app.action_list_state.select(Some(10));
        // Now simulate logging out
        app.client_status.status = "logged out".to_string();
        app.clamp_action_selection();
        // Logged out only has 2 actions, should clamp to 1
        assert_eq!(app.action_list_state.selected(), Some(1));
    }

    // ===== Action labels =====

    #[test]
    fn action_labels_are_nonempty() {
        let actions = vec![
            Action::Login, Action::Logout, Action::Create, Action::Join,
            Action::Leave, Action::Delete, Action::Nickname, Action::Evict,
            Action::Password, Action::Access, Action::OnlineOffline, Action::Quit,
        ];
        for action in actions {
            assert!(!action.label().is_empty(), "Action {:?} has empty label", action);
        }
    }

    // Helper: synchronous handle for non-async key handling in normal mode
    impl App {
        fn handle_normal_key_sync(&mut self, key: KeyEvent) {
            match key.code {
                KeyCode::Up => self.navigate_up(),
                KeyCode::Down => self.navigate_down(),
                KeyCode::Left => self.focused_panel = FocusedPanel::Actions,
                KeyCode::Right => {
                    if !self.networks.is_empty() {
                        self.focused_panel = FocusedPanel::Networks;
                    }
                }
                KeyCode::Tab => self.toggle_focus(),
                _ => {}
            }
        }
    }
}
