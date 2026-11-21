//! Sharing open tabs between the person's computers on the local network, read-only.
//!
//! * Every 2 s each TextCraft broadcasts a small "beacon" (UDP, port 47821): its name,
//!   the TCP port it answers on, and the list of its tabs.
//! * Another TextCraft asks for one tab's text over TCP (port 47822, or any free port).
//! * Everything (beacons, requests, answers) is encrypted with ChaCha20-Poly1305 using a
//!   key made from the secret code (PBKDF2-SHA256). Without the code nothing can be read
//!   or faked; computers with a different code simply don't see each other.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};

pub const BEACON_PORT: u16 = 47821;
pub const TCP_PORT: u16 = 47822;
const PEER_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_TEXT: usize = 32 * 1024 * 1024;

// ---------------------------------------------------------------- crypto

pub fn derive_key(code: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(code.trim().as_bytes(), b"textcraft-lan-v1", 120_000, &mut key);
    key
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if getrandom::fill(&mut b).is_err() {
        // Extremely unlikely; fall back to time-based bytes so the app keeps working.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        for (i, x) in b.iter_mut().enumerate() {
            *x = (t >> ((i % 16) * 8)) as u8 ^ (i as u8).wrapping_mul(31);
        }
    }
    b
}

pub fn seal(key: &[u8; 32], purpose: &[u8], plain: &[u8]) -> Vec<u8> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce: [u8; 12] = random_bytes();
    let mut out = nonce.to_vec();
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plain, aad: purpose })
        .expect("encryption never fails for in-memory data");
    out.extend_from_slice(&ct);
    out
}

pub fn open(key: &[u8; 32], purpose: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 + 16 {
        return None;
    }
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher.decrypt(Nonce::from_slice(&data[..12]), Payload { msg: &data[12..], aad: purpose }).ok()
}

// ---------------------------------------------------------------- messages

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TabInfo {
    pub id: u64,
    pub title: String,
    pub lang: String,
    pub rev: u64,
    pub dirty: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Beacon {
    app: String,
    id: String,
    name: String,
    os: String,
    port: u16,
    tabs: Vec<TabInfo>,
}

#[derive(Serialize, Deserialize)]
struct Request {
    get: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TabContent {
    pub id: u64,
    pub title: String,
    pub lang: String,
    pub rev: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub os: String,
    pub addr: IpAddr,
    pub port: u16,
    pub tabs: Vec<TabInfo>,
    pub last_seen: Instant,
}

// ---------------------------------------------------------------- shared state

struct LocalTab {
    info: TabInfo,
    text: Arc<String>,
}

struct Shared {
    enabled: bool,
    key: Option<[u8; 32]>,
    my_id: String,
    name: String,
    port: u16,
    hint: Option<Ipv4Addr>,
    /// Where to listen (all networks normally; this computer only for tests).
    bind_ip: Ipv4Addr,
    listen_port: u16,
    send_port: u16,
    tabs: Vec<LocalTab>,
    peers: HashMap<String, Peer>,
    fetched: HashMap<(String, u64), TabContent>,
    pending: HashSet<(String, u64)>,
    problem: Option<String>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            enabled: false,
            key: None,
            my_id: String::new(),
            name: String::new(),
            port: 0,
            hint: None,
            bind_ip: Ipv4Addr::UNSPECIFIED,
            listen_port: BEACON_PORT,
            send_port: BEACON_PORT,
            tabs: Vec::new(),
            peers: HashMap::new(),
            fetched: HashMap::new(),
            pending: HashSet::new(),
            problem: None,
        }
    }
}

#[derive(Clone)]
pub struct Net {
    s: Arc<Mutex<Shared>>,
    started: Arc<Mutex<bool>>,
}

pub fn computer_name() -> String {
    let n = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .unwrap_or_default();
    let n = n.trim();
    if n.is_empty() {
        "Ordinateur".to_string()
    } else {
        n.to_string()
    }
}

/// A code that is easy to type on both computers, e.g. "ocre-plume-4821".
pub fn suggest_code() -> String {
    const A: [&str; 12] = ["ocre", "sienne", "indigo", "sepia", "carmin", "craie", "fusain", "lavis", "pastel", "gouache", "encre", "vermillon"];
    const B: [&str; 12] = ["plume", "pinceau", "carnet", "papier", "palette", "atelier", "esquisse", "croquis", "toile", "gomme", "crayon", "chevalet"];
    let r: [u8; 4] = random_bytes();
    format!("{}-{}-{:04}", A[r[0] as usize % 12], B[r[1] as usize % 12], u16::from_le_bytes([r[2], r[3]]) % 10000)
}

fn lock(s: &Arc<Mutex<Shared>>) -> std::sync::MutexGuard<'_, Shared> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

fn write_frame(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(data.len() as u32).to_be_bytes())?;
    stream.write_all(data)?;
    stream.flush()
}

fn read_frame(stream: &mut TcpStream, max: usize) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len)?;
    let n = u32::from_be_bytes(len) as usize;
    if n > max {
        return Err(std::io::Error::other("message trop grand"));
    }
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

/// Asks one computer for one tab's text.
pub fn fetch(addr: SocketAddr, key: &[u8; 32], tab: u64) -> Option<TabContent> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    s.set_write_timeout(Some(Duration::from_secs(5))).ok()?;
    let req = serde_json::to_vec(&Request { get: tab }).ok()?;
    write_frame(&mut s, &seal(key, b"textcraft-req", &req)).ok()?;
    let resp = read_frame(&mut s, MAX_TEXT + 4096).ok()?;
    let plain = open(key, b"textcraft-resp", &resp)?;
    serde_json::from_slice(&plain).ok()
}

fn broadcast_targets(hint: Option<Ipv4Addr>) -> Vec<Ipv4Addr> {
    let mut t = vec![Ipv4Addr::BROADCAST];
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        for i in ifs {
            if i.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = &i.addr {
                let b = Ipv4Addr::from(u32::from(v4.ip) | !u32::from(v4.netmask));
                if !t.contains(&b) {
                    t.push(b);
                }
            }
        }
    }
    if let Some(h) = hint {
        t.push(h);
    }
    t
}

impl Net {
    pub fn new() -> Self {
        let id: [u8; 8] = random_bytes();
        let mut shared = Shared {
            my_id: id.iter().map(|b| format!("{b:02x}")).collect(),
            name: computer_name(),
            bind_ip: Ipv4Addr::UNSPECIFIED,
            listen_port: BEACON_PORT,
            send_port: BEACON_PORT,
            ..Default::default()
        };
        // Test aid: TEXTCRAFT_TEST_LOOPBACK="listen,send" keeps everything on this computer
        // (no firewall question), so two copies can be tried side by side.
        if let Ok(spec) = std::env::var("TEXTCRAFT_TEST_LOOPBACK") {
            let ports: Vec<u16> = spec.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if let [l, snd] = ports[..] {
                shared.bind_ip = Ipv4Addr::LOCALHOST;
                shared.listen_port = l;
                shared.send_port = snd;
                shared.hint = Some(Ipv4Addr::LOCALHOST);
            }
        }
        Net { s: Arc::new(Mutex::new(shared)), started: Arc::new(Mutex::new(false)) }
    }

    #[cfg(test)]
    fn for_test(listen: u16, send: u16) -> Self {
        let n = Net::new();
        {
            let mut g = lock(&n.s);
            g.bind_ip = Ipv4Addr::LOCALHOST;
            g.listen_port = listen;
            g.send_port = send;
        }
        n
    }

    /// Turns sharing on or off and sets the code, name and optional address of another computer.
    pub fn configure(&self, enabled: bool, key: Option<[u8; 32]>, name: &str, hint: &str) {
        {
            let mut s = lock(&self.s);
            let key_changed = s.key != key;
            s.enabled = enabled && key.is_some();
            s.key = key;
            s.name = if name.trim().is_empty() { computer_name() } else { name.trim().chars().take(60).collect() };
            if s.bind_ip != Ipv4Addr::LOCALHOST {
                s.hint = hint.trim().parse().ok();
            }
            if key_changed || !enabled {
                s.peers.clear();
                s.fetched.clear();
            }
        }
        if enabled && key.is_some() {
            self.start();
        }
    }

    /// Starts the network threads (once; the firewall question only comes when sharing is turned on).
    fn start(&self) {
        let mut started = self.started.lock().unwrap_or_else(|e| e.into_inner());
        if *started {
            return;
        }
        *started = true;

        let (ip, listen_port) = {
            let g = lock(&self.s);
            (g.bind_ip, g.listen_port)
        };
        // Answer requests for our tabs.
        let listener = TcpListener::bind((ip, TCP_PORT)).or_else(|_| TcpListener::bind((ip, 0)));
        match listener {
            Ok(l) => {
                lock(&self.s).port = l.local_addr().map(|a| a.port()).unwrap_or(0);
                let s = self.s.clone();
                std::thread::spawn(move || {
                    for conn in l.incoming().flatten() {
                        let s = s.clone();
                        std::thread::spawn(move || serve(conn, s));
                    }
                });
            }
            Err(e) => lock(&self.s).problem = Some(format!("Impossible d'ouvrir le partage : {e}")),
        }

        // Listen for the other computers' beacons.
        match beacon_socket(ip, listen_port) {
            Ok(sock) => {
                let s = self.s.clone();
                std::thread::spawn(move || listen(sock, s));
            }
            Err(e) => lock(&self.s).problem = Some(format!("Impossible d'écouter le réseau : {e}")),
        }

        // Announce ourselves.
        let s = self.s.clone();
        std::thread::spawn(move || announce(s));
    }

    /// Publishes this computer's tabs (called by the app about once a second).
    pub fn set_tabs(&self, tabs: Vec<(TabInfo, Arc<String>)>) {
        let mut s = lock(&self.s);
        s.tabs = tabs.into_iter().map(|(info, text)| LocalTab { info, text }).collect();
    }

    pub fn enabled(&self) -> bool {
        lock(&self.s).enabled
    }

    pub fn problem(&self) -> Option<String> {
        lock(&self.s).problem.clone()
    }

    /// The other computers seen in the last few seconds, by name.
    pub fn peers(&self) -> Vec<Peer> {
        let mut s = lock(&self.s);
        s.peers.retain(|_, p| p.last_seen.elapsed() < PEER_TIMEOUT);
        let mut v: Vec<Peer> = s.peers.values().cloned().collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    }

    /// Asks for a tab's text in the background; the answer shows up in `fetched`.
    pub fn request(&self, peer_id: &str, tab: u64, ctx: &eframe::egui::Context) {
        let (addr, key) = {
            let mut s = lock(&self.s);
            let k = (peer_id.to_string(), tab);
            if s.pending.contains(&k) {
                return;
            }
            let Some(p) = s.peers.get(peer_id) else { return };
            let Some(key) = s.key else { return };
            let addr = SocketAddr::new(p.addr, p.port);
            s.pending.insert(k);
            (addr, key)
        };
        let s = self.s.clone();
        let peer = peer_id.to_string();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let got = fetch(addr, &key, tab);
            let mut g = lock(&s);
            g.pending.remove(&(peer.clone(), tab));
            if let Some(c) = got {
                g.fetched.insert((peer, tab), c);
            }
            drop(g);
            ctx.request_repaint();
        });
    }

    pub fn fetched(&self, peer_id: &str, tab: u64) -> Option<TabContent> {
        lock(&self.s).fetched.get(&(peer_id.to_string(), tab)).cloned()
    }
}

fn beacon_socket(ip: Ipv4Addr, port: u16) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_broadcast(true)?;
    sock.bind(&SocketAddr::from((ip, port)).into())?;
    let udp: UdpSocket = sock.into();
    udp.set_read_timeout(Some(Duration::from_secs(1)))?;
    Ok(udp)
}

fn announce(s: Arc<Mutex<Shared>>) {
    let (ip, send_port) = {
        let g = lock(&s);
        (g.bind_ip, g.send_port)
    };
    let Ok(sock) = UdpSocket::bind((ip, 0)) else { return };
    let _ = sock.set_broadcast(true);
    let mut targets = Vec::new();
    let mut targets_age = Instant::now() - Duration::from_secs(60);
    loop {
        let msg = {
            let g = lock(&s);
            match (g.enabled, g.key) {
                (true, Some(key)) if g.port != 0 => {
                    let b = Beacon {
                        app: "textcraft".into(),
                        id: g.my_id.clone(),
                        name: g.name.clone(),
                        os: std::env::consts::OS.into(),
                        port: g.port,
                        tabs: g.tabs.iter().take(200).map(|t| t.info.clone()).collect(),
                    };
                    serde_json::to_vec(&b).ok().map(|j| (seal(&key, b"textcraft-beacon", &j), g.hint))
                }
                _ => None,
            }
        };
        if let Some((data, hint)) = msg {
            if targets_age.elapsed() > Duration::from_secs(30) {
                targets = if ip == Ipv4Addr::LOCALHOST { vec![Ipv4Addr::LOCALHOST] } else { broadcast_targets(hint) };
                targets_age = Instant::now();
            }
            for t in &targets {
                let _ = sock.send_to(&data, (*t, send_port));
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn listen(sock: UdpSocket, s: Arc<Mutex<Shared>>) {
    let mut buf = vec![0u8; 65536];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        let mut g = lock(&s);
        let (true, Some(key)) = (g.enabled, g.key) else { continue };
        let Some(plain) = open(&key, b"textcraft-beacon", &buf[..n]) else { continue };
        let Ok(b) = serde_json::from_slice::<Beacon>(&plain) else { continue };
        if b.app != "textcraft" || b.id == g.my_id {
            continue;
        }
        g.peers.insert(
            b.id.clone(),
            Peer { id: b.id, name: b.name, os: b.os, addr: from.ip(), port: b.port, tabs: b.tabs, last_seen: Instant::now() },
        );
    }
}

fn serve(mut conn: TcpStream, s: Arc<Mutex<Shared>>) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(20)));
    let Ok(frame) = read_frame(&mut conn, 64 * 1024) else { return };
    let (key, answer) = {
        let g = lock(&s);
        let (true, Some(key)) = (g.enabled, g.key) else { return };
        let Some(plain) = open(&key, b"textcraft-req", &frame) else { return };
        let Ok(req) = serde_json::from_slice::<Request>(&plain) else { return };
        let Some(t) = g.tabs.iter().find(|t| t.info.id == req.get) else { return };
        let mut text = t.text.as_str();
        if text.len() > MAX_TEXT {
            let mut cut = MAX_TEXT;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text = &text[..cut];
        }
        let c = TabContent { id: t.info.id, title: t.info.title.clone(), lang: t.info.lang.clone(), rev: t.info.rev, text: text.to_string() };
        (key, c)
    };
    if let Ok(j) = serde_json::to_vec(&answer) {
        let _ = write_frame(&mut conn, &seal(&key, b"textcraft-resp", &j));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_messages_need_the_right_code() {
        let k = derive_key("ocre-plume-1234");
        let other = derive_key("ocre-plume-1235");
        let msg = seal(&k, b"textcraft-beacon", b"bonjour");
        assert_eq!(open(&k, b"textcraft-beacon", &msg).as_deref(), Some(&b"bonjour"[..]));
        assert!(open(&other, b"textcraft-beacon", &msg).is_none(), "wrong code must not read");
        assert!(open(&k, b"textcraft-resp", &msg).is_none(), "a beacon can't pass as an answer");
        let mut bad = msg.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open(&k, b"textcraft-beacon", &bad).is_none(), "tampering is detected");
    }

    #[test]
    fn a_tab_can_be_fetched_over_the_network_with_the_code_only() {
        let net = Net::for_test(47911, 47912);
        let key = derive_key("sienne-carnet-0042");
        net.configure(true, Some(key), "Test", "");
        let text = Arc::new("# Bonjour\nligne 2 non enregistrée".to_string());
        net.set_tabs(vec![(TabInfo { id: 7, title: "notes.md".into(), lang: "Markdown".into(), rev: 99, dirty: true }, text.clone())]);
        let port = lock(&net.s).port;
        assert!(port != 0);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let got = fetch(addr, &key, 7).expect("fetch with the right code");
        assert_eq!(got.text, *text);
        assert_eq!(got.title, "notes.md");
        assert!(fetch(addr, &derive_key("autre-code-0000"), 7).is_none(), "wrong code gets nothing");
        assert!(fetch(addr, &key, 8).is_none(), "unknown tab gets nothing");
    }

    #[test]
    fn two_computers_find_each_other_and_read_a_tab() {
        let key = derive_key("indigo-toile-7777");
        let a = Net::for_test(47921, 47922);
        let b = Net::for_test(47922, 47921);
        let stranger = Net::for_test(47923, 47921); // listens elsewhere, other code
        a.configure(true, Some(key), "PC Windows", "127.0.0.1");
        b.configure(true, Some(key), "PC Linux", "127.0.0.1");
        stranger.configure(true, Some(derive_key("autre-code-0000")), "Intrus", "127.0.0.1");
        let text = Arc::new("copie-moi\nentre les PC".to_string());
        a.set_tabs(vec![(TabInfo { id: 3, title: "presse-papier.txt".into(), lang: "Plain".into(), rev: 1, dirty: true }, text.clone())]);

        let start = Instant::now();
        let peer = loop {
            if let Some(p) = b.peers().into_iter().find(|p| p.name == "PC Windows" && !p.tabs.is_empty()) {
                break p;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "B never saw A");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(peer.tabs.len(), 1);
        assert!(b.peers().iter().all(|p| p.name != "Intrus"), "a computer with another code stays invisible");

        let got = fetch(SocketAddr::new(peer.addr, peer.port), &key, 3).expect("B reads A's tab");
        assert_eq!(got.text, *text);
    }
}
