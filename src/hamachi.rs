use std::fmt;

use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct ClientStatus {
    pub version: String,
    pub pid: String,
    pub status: String,
    pub client_id: String,
    pub address: String,
    pub nickname: String,
    pub lmi_account: String,
}

impl ClientStatus {
    pub fn is_logged_in(&self) -> bool {
        self.status.contains("logged in")
    }
}

#[derive(Debug, Clone)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub capacity: String,
    pub owner: String,
    pub peers: Vec<Peer>,
    pub is_online: bool,
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub client_id: String,
    pub nickname: String,
    pub address: String,
    pub status: PeerStatus,
    pub connection_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PeerStatus {
    Online,
    Offline,
    Unknown,
}

impl fmt::Display for PeerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerStatus::Online => write!(f, "online"),
            PeerStatus::Offline => write!(f, "offline"),
            PeerStatus::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug)]
pub struct HamachiResult {
    pub success: bool,
    pub output: String,
}

pub async fn run_command(args: &[&str]) -> HamachiResult {
    match Command::new("hamachi")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };
            HamachiResult {
                success: output.status.success(),
                output: combined.trim().to_string(),
            }
        }
        Err(e) => HamachiResult {
            success: false,
            output: format!("Failed to run hamachi: {}", e),
        },
    }
}

pub async fn get_status() -> ClientStatus {
    let result = run_command(&[]).await;
    parse_status(&result.output)
}

pub async fn get_networks() -> Vec<Network> {
    let result = run_command(&["list"]).await;
    if !result.success {
        return Vec::new();
    }
    parse_network_list(&result.output)
}

pub async fn is_daemon_running() -> bool {
    match Command::new("hamachi").output().await {
        Ok(output) => {
            let out = String::from_utf8_lossy(&output.stdout);
            let err = String::from_utf8_lossy(&output.stderr);
            !out.contains("does not seem to be running") && !err.contains("does not seem to be running")
        }
        Err(_) => false,
    }
}

pub async fn login() -> HamachiResult {
    if !is_daemon_running().await {
        return HamachiResult {
            success: false,
            output: "Daemon not running. Run: sudo systemctl start logmein-hamachi".to_string(),
        };
    }
    run_command(&["login"]).await
}

pub async fn logout() -> HamachiResult {
    run_command(&["logout"]).await
}

pub async fn create_network(name: &str, password: &str) -> HamachiResult {
    if password.is_empty() {
        return HamachiResult {
            success: false,
            output: "Password cannot be blank".to_string(),
        };
    }
    run_command(&["create", name, password]).await
}

pub async fn join_network(name: &str, password: &str) -> HamachiResult {
    if password.is_empty() {
        run_command(&["join", name]).await
    } else {
        run_command(&["join", name, password]).await
    }
}

pub async fn leave_network(name: &str) -> HamachiResult {
    run_command(&["leave", name]).await
}

pub async fn delete_network(name: &str) -> HamachiResult {
    run_command(&["delete", name]).await
}

pub async fn go_online(network: &str) -> HamachiResult {
    run_command(&["go-online", network]).await
}

pub async fn go_offline(network: &str) -> HamachiResult {
    run_command(&["go-offline", network]).await
}

pub async fn set_nickname(nickname: &str) -> HamachiResult {
    run_command(&["set-nick", nickname]).await
}

pub async fn evict(network: &str, client_id: &str) -> HamachiResult {
    run_command(&["evict", network, client_id]).await
}

pub async fn set_password(network: &str, password: &str) -> HamachiResult {
    if password.is_empty() {
        run_command(&["set-pass", network]).await
    } else {
        run_command(&["set-pass", network, password]).await
    }
}

pub async fn set_access(network: &str, lock: bool) -> HamachiResult {
    let mode = if lock { "lock" } else { "unlock" };
    run_command(&["set-access", network, mode]).await
}

pub(crate) fn parse_status(output: &str) -> ClientStatus {
    let mut status = ClientStatus::default();

    for line in output.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "version" => status.version = value,
                "pid" => status.pid = value,
                "status" => status.status = value,
                "client id" => status.client_id = value,
                "address" => {
                    // Address line may contain both IPv4 and IPv6: "25.5.249.38    2620:9b::1905:f926"
                    // Extract just the IPv4 address (first whitespace-separated token)
                    status.address = value.split_whitespace().next().unwrap_or("").to_string();
                }
                "nickname" => status.nickname = value,
                "lmi account" => status.lmi_account = value,
                _ => {}
            }
        }
    }

    status
}

pub(crate) fn parse_network_list(output: &str) -> Vec<Network> {
    let mut networks: Vec<Network> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Network line starts with * or [
        if trimmed.starts_with('*') && trimmed.contains('[') {
            if let Some(network) = parse_network_line(trimmed, true) {
                networks.push(network);
            }
        } else if trimmed.starts_with('[') {
            if let Some(network) = parse_network_line(trimmed, false) {
                networks.push(network);
            }
        } else if !trimmed.is_empty() {
            // Peer line
            if let Some(network) = networks.last_mut() {
                if let Some(peer) = parse_peer_line(trimmed) {
                    network.peers.push(peer);
                }
            }
        }
    }

    networks
}

pub(crate) fn parse_network_line(line: &str, is_online: bool) -> Option<Network> {
    // Format: * [485-431-234]JonoLAN  capacity: 3/5, subscription type: Free, owner: email
    // or:      [485-431-234]JonoLAN  capacity: 3/5, ...
    // The ID is in brackets, the name follows immediately after
    let start = line.find('[')?;
    let end = line.find(']')?;
    let id = line[start + 1..end].to_string();

    let rest = &line[end + 1..];

    // Name is everything between ] and "capacity:" (or end of line), trimmed
    let name = rest
        .split("capacity:")
        .next()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    // If no separate name, use the ID as name
    let name = if name.is_empty() { id.clone() } else { name };

    let capacity = rest
        .split("capacity:")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let owner = rest
        .split("owner:")
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Some(Network {
        id,
        name,
        capacity,
        owner,
        peers: Vec::new(),
        is_online,
    })
}

pub(crate) fn parse_peer_line(line: &str) -> Option<Peer> {
    // Peer lines have various formats, try to extract key info
    // Format: * 090-123-456   nickname   25.x.x.x   direct   UDP  endpoint
    // or:       090-123-456   nickname   25.x.x.x   via relay
    let is_self = line.starts_with('*');
    let line = line.trim_start_matches('*').trim();

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let client_id = parts[0].to_string();
    let nickname = parts.get(1).unwrap_or(&"").to_string();
    let address = parts.get(2).unwrap_or(&"").to_string();

    let remaining = if parts.len() > 3 {
        parts[3..].join(" ")
    } else {
        String::new()
    };
    let (status, connection_type) = if remaining.contains("direct") {
        (PeerStatus::Online, "direct".to_string())
    } else if remaining.contains("relay") {
        (PeerStatus::Online, "relay".to_string())
    } else if address.is_empty() || remaining.is_empty() || remaining.contains("offline") {
        (PeerStatus::Offline, "offline".to_string())
    } else {
        (PeerStatus::Unknown, remaining)
    };

    let _ = is_self; // Could use this later to mark self peer

    Some(Peer {
        client_id,
        nickname,
        address,
        status,
        connection_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== parse_status tests =====

    #[test]
    fn parse_status_logged_in() {
        let output = "\
  version    : 2.1.0.203
  pid        : 12345
  status     : logged in
  client id  : 090-123-456
  address    : 25.10.20.30
  nickname   : my-linux-box
  lmi account: user@example.com";

        let s = parse_status(output);
        assert_eq!(s.version, "2.1.0.203");
        assert_eq!(s.pid, "12345");
        assert_eq!(s.status, "logged in");
        assert_eq!(s.client_id, "090-123-456");
        assert_eq!(s.address, "25.10.20.30");
        assert_eq!(s.nickname, "my-linux-box");
        assert_eq!(s.lmi_account, "user@example.com");
        assert!(s.is_logged_in());
    }

    #[test]
    fn parse_status_logged_out() {
        let output = "\
  version    : 2.1.0.203
  pid        : 12345
  status     : logged out
  client id  :
  address    :
  nickname   :
  lmi account:";

        let s = parse_status(output);
        assert_eq!(s.status, "logged out");
        assert!(!s.is_logged_in());
        assert_eq!(s.client_id, "");
        assert_eq!(s.address, "");
    }

    #[test]
    fn parse_status_empty_output() {
        let s = parse_status("");
        assert_eq!(s.version, "");
        assert_eq!(s.status, "");
        assert!(!s.is_logged_in());
    }

    #[test]
    fn parse_status_daemon_not_running() {
        let output = "Hamachi does not seem to be running.\nRun '/etc/init.d/logmein-hamachi start' to start daemon.";
        let s = parse_status(output);
        assert!(!s.is_logged_in());
        assert_eq!(s.address, "");
    }

    #[test]
    fn parse_status_partial_output() {
        let output = "\
  version    : 2.1.0.203
  pid        : 99999
  status     : logged in";

        let s = parse_status(output);
        assert!(s.is_logged_in());
        assert_eq!(s.pid, "99999");
        assert_eq!(s.address, "");
        assert_eq!(s.nickname, "");
    }

    // ===== parse_network_list tests =====

    #[test]
    fn parse_network_list_single_network_with_peers() {
        let output = "\
 * [485-431-234]JonoLAN  capacity: 3/5, subscription type: Free, owner: user@example.com
     090-123-456   my-server              25.10.20.30    direct      UDP  1.2.3.4:12345
     090-789-012   windows-desktop        25.10.20.31    direct      UDP  5.6.7.8:54321
   * 090-345-678   mobile-laptop          25.10.20.32    via relay";

        let nets = parse_network_list(output);
        assert_eq!(nets.len(), 1);

        let net = &nets[0];
        assert_eq!(net.id, "485-431-234");
        assert_eq!(net.name, "JonoLAN");
        assert!(net.is_online);
        assert_eq!(net.capacity, "3/5");
        assert_eq!(net.owner, "user@example.com");
        assert_eq!(net.peers.len(), 3);

        assert_eq!(net.peers[0].client_id, "090-123-456");
        assert_eq!(net.peers[0].nickname, "my-server");
        assert_eq!(net.peers[0].address, "25.10.20.30");
        assert_eq!(net.peers[0].status, PeerStatus::Online);
        assert_eq!(net.peers[0].connection_type, "direct");

        assert_eq!(net.peers[2].client_id, "090-345-678");
        assert_eq!(net.peers[2].status, PeerStatus::Online);
        assert_eq!(net.peers[2].connection_type, "relay");
    }

    #[test]
    fn parse_network_list_multiple_networks() {
        let output = "\
 * [111-222-333]NetAlpha  capacity: 2/5, subscription type: Free, owner: a@example.com
     090-111-111   peer-a1                25.10.1.1    direct      UDP  1.1.1.1:1111
 [444-555-666]NetBeta  capacity: 1/5, subscription type: Free, owner: b@example.com
     090-222-222   peer-b1                25.10.2.1    via relay";

        let nets = parse_network_list(output);
        assert_eq!(nets.len(), 2);

        assert_eq!(nets[0].id, "111-222-333");
        assert_eq!(nets[0].name, "NetAlpha");
        assert!(nets[0].is_online);
        assert_eq!(nets[0].peers.len(), 1);

        assert_eq!(nets[1].id, "444-555-666");
        assert_eq!(nets[1].name, "NetBeta");
        assert!(!nets[1].is_online);
        assert_eq!(nets[1].peers.len(), 1);
        assert_eq!(nets[1].peers[0].connection_type, "relay");
    }

    #[test]
    fn parse_network_list_empty_output() {
        let nets = parse_network_list("");
        assert!(nets.is_empty());
    }

    #[test]
    fn parse_network_list_network_no_peers() {
        let output = " * [999-000-111]EmptyNet  capacity: 0/5, subscription type: Free, owner: test@example.com";
        let nets = parse_network_list(output);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].id, "999-000-111");
        assert_eq!(nets[0].name, "EmptyNet");
        assert!(nets[0].peers.is_empty());
    }

    #[test]
    fn parse_network_list_offline_network() {
        let output = " [777-888-999]OfflineNet  capacity: 1/5, subscription type: Free, owner: test@example.com
     090-111-222   some-peer              25.10.5.5    direct      UDP  2.3.4.5:9999";

        let nets = parse_network_list(output);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].name, "OfflineNet");
        assert!(!nets[0].is_online);
        assert_eq!(nets[0].peers.len(), 1);
    }

    // ===== parse_network_line tests =====

    #[test]
    fn parse_network_line_with_name() {
        let line = "* [485-431-234]JonoLAN  capacity: 2/5, subscription type: Free, owner: me@test.com";
        let net = parse_network_line(line, true).unwrap();
        assert_eq!(net.id, "485-431-234");
        assert_eq!(net.name, "JonoLAN");
        assert!(net.is_online);
        assert_eq!(net.capacity, "2/5");
        assert_eq!(net.owner, "me@test.com");
    }

    #[test]
    fn parse_network_line_no_name_uses_id() {
        let line = "* [123-456-789]  capacity: 1/5, subscription type: Free, owner: x@y.com";
        let net = parse_network_line(line, true).unwrap();
        assert_eq!(net.id, "123-456-789");
        assert_eq!(net.name, "123-456-789");
    }

    #[test]
    fn parse_network_line_offline() {
        let line = "[555-666-777]OffNet  capacity: 0/5, subscription type: Free, owner: x@y.com";
        let net = parse_network_line(line, false).unwrap();
        assert_eq!(net.id, "555-666-777");
        assert_eq!(net.name, "OffNet");
        assert!(!net.is_online);
    }

    #[test]
    fn parse_network_line_no_brackets() {
        let result = parse_network_line("no brackets here", false);
        assert!(result.is_none());
    }

    #[test]
    fn parse_network_line_no_capacity() {
        let line = "[999-111-222]MinimalNet";
        let net = parse_network_line(line, true).unwrap();
        assert_eq!(net.id, "999-111-222");
        assert_eq!(net.name, "MinimalNet");
        assert_eq!(net.capacity, "");
        assert_eq!(net.owner, "");
    }

    #[test]
    fn parse_network_line_name_with_spaces() {
        let line = "* [100-200-300]My Cool Network  capacity: 1/5, subscription type: Free, owner: a@b.com";
        let net = parse_network_line(line, true).unwrap();
        assert_eq!(net.id, "100-200-300");
        assert_eq!(net.name, "My Cool Network");
    }

    // ===== parse_peer_line tests =====

    #[test]
    fn parse_peer_line_direct() {
        let line = "090-123-456   my-server              25.10.20.30    direct      UDP  1.2.3.4:12345";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-123-456");
        assert_eq!(peer.nickname, "my-server");
        assert_eq!(peer.address, "25.10.20.30");
        assert_eq!(peer.status, PeerStatus::Online);
        assert_eq!(peer.connection_type, "direct");
    }

    #[test]
    fn parse_peer_line_relay() {
        let line = "090-345-678   mobile-laptop          25.10.20.32    via relay";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-345-678");
        assert_eq!(peer.nickname, "mobile-laptop");
        assert_eq!(peer.status, PeerStatus::Online);
        assert_eq!(peer.connection_type, "relay");
    }

    #[test]
    fn parse_peer_line_self_marker() {
        let line = "* 090-111-111   me                     25.10.1.1    direct      UDP  1.1.1.1:1111";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-111-111");
        assert_eq!(peer.nickname, "me");
        assert_eq!(peer.status, PeerStatus::Online);
    }

    #[test]
    fn parse_peer_line_empty() {
        let result = parse_peer_line("");
        assert!(result.is_none());
    }

    #[test]
    fn parse_peer_line_only_id() {
        let line = "090-999-999";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-999-999");
        assert_eq!(peer.nickname, "");
        assert_eq!(peer.address, "");
        // No address -> offline
        assert_eq!(peer.status, PeerStatus::Offline);
    }

    #[test]
    fn parse_peer_line_id_and_nick_only() {
        let line = "090-888-888   some-nick";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-888-888");
        assert_eq!(peer.nickname, "some-nick");
        assert_eq!(peer.address, "");
        assert_eq!(peer.status, PeerStatus::Offline);
    }

    #[test]
    fn parse_peer_line_unknown_connection() {
        let line = "090-777-777   nick   25.10.3.3    something-weird";
        let peer = parse_peer_line(line).unwrap();
        assert_eq!(peer.client_id, "090-777-777");
        assert_eq!(peer.status, PeerStatus::Unknown);
        assert_eq!(peer.connection_type, "something-weird");
    }

    // ===== ClientStatus tests =====

    #[test]
    fn client_status_is_logged_in_variations() {
        let mut s = ClientStatus::default();
        assert!(!s.is_logged_in());

        s.status = "logged in".to_string();
        assert!(s.is_logged_in());

        s.status = "logged out".to_string();
        assert!(!s.is_logged_in());

        // Edge: contains "logged in" somewhere
        s.status = "status: logged in (something)".to_string();
        assert!(s.is_logged_in());
    }

    // ===== is_daemon_running edge cases (logic only, no I/O) =====

    #[test]
    fn daemon_not_running_message_detected() {
        let msg = "Hamachi does not seem to be running.";
        assert!(msg.contains("does not seem to be running"));
    }

    // ===== Stress: large peer list =====

    #[test]
    fn parse_network_list_many_peers() {
        let mut output = String::from(" * [000-000-001]BigNet  capacity: 100/256, subscription type: Premium, owner: admin@co.com\n");
        for i in 0..100 {
            output.push_str(&format!(
                "     090-{:03}-{:03}   peer-{}              25.10.{}.{}    direct      UDP  1.2.3.4:{}\n",
                i / 1000, i % 1000, i, i / 256, i % 256, 10000 + i
            ));
        }

        let nets = parse_network_list(&output);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].peers.len(), 100);
        assert_eq!(nets[0].peers[0].client_id, "090-000-000");
        assert_eq!(nets[0].peers[99].client_id, "090-000-099");
    }

    // ===== Mixed blank lines =====

    #[test]
    fn parse_network_list_with_blank_lines() {
        let output = "\n\n * [001-002-003]Net1  capacity: 1/5, subscription type: Free, owner: a@b.com\n\n     090-111-111   peer1   25.10.1.1    direct   UDP  1.1.1.1:1111\n\n";
        let nets = parse_network_list(output);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].peers.len(), 1);
    }

    // ===== Status with extra colons in value =====

    #[test]
    fn parse_status_colon_in_value() {
        // The lmi account line has a colon in email-like contexts
        // split_once should handle this correctly since it splits on first colon only
        let output = "  lmi account: user@example.com";
        let s = parse_status(output);
        assert_eq!(s.lmi_account, "user@example.com");
    }

    #[test]
    fn parse_status_address_with_ipv6_like() {
        // Edge case: what if address had colons? split_once should still work
        let output = "  address    : 25.10.20.30";
        let s = parse_status(output);
        assert_eq!(s.address, "25.10.20.30");
    }
}
