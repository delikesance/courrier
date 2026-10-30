use crate::config::{self, Config};
use lettre::{message::Mailbox, transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use mail_parser::MessageParser;
use serde::Serialize;

type Session = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

fn e<T: std::fmt::Display>(x: T) -> String { x.to_string() }

fn connect(c: &Config) -> Result<Session, String> {
    let tls = native_tls::TlsConnector::builder().build().map_err(e)?;
    let client = imap::connect((c.imap_host.as_str(), c.imap_port), &c.imap_host, &tls).map_err(e)?;
    client.login(&c.user, &c.pass).map_err(|x| x.0.to_string())
}


static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

/// Session IMAP persistante : évite un handshake TLS + login à chaque action. Reconnecte si la connexion est tombée.
fn with<T>(f: impl Fn(&mut Session) -> Result<T, String>) -> Result<T, String> {
    let mut g = SESSION.lock().unwrap_or_else(|p| p.into_inner());
    let mut last = String::new();
    for _ in 0..2 {
        if g.is_none() { *g = Some(connect(&config::load()?)?); }
        match f(g.as_mut().unwrap()) {
            Ok(v) => return Ok(v),
            Err(e) => { last = e; *g = None; }
        }
    }
    Err(last)
}

pub fn reset() { *SESSION.lock().unwrap_or_else(|p| p.into_inner()) = None; }

#[derive(Serialize)]
pub struct Summary { pub uid: u32, pub from: String, pub subject: String, pub date: String, pub seen: bool }

#[derive(Serialize)]
pub struct Full {
    pub uid: u32, pub from: String, pub from_addr: String, pub to: String, pub subject: String,
    pub date: String, pub message_id: String, pub text: String, pub html: Option<String>,
}

pub fn list_folders() -> Result<Vec<String>, String> {
    let mut v = with(|s| Ok(s.list(None, Some("*")).map_err(e)?.iter().map(|n| n.name().to_string()).collect::<Vec<_>>()))?;
    v.sort_by_key(|n| (n != "INBOX", n.to_lowercase()));
    Ok(v)
}

fn addr_label(a: Option<&mail_parser::Address>) -> (String, String) {
    match a.and_then(|a| a.first()) {
        Some(x) => {
            let addr = x.address().unwrap_or("").to_string();
            let name = x.name().map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| addr.clone());
            (name, addr)
        }
        None => (String::new(), String::new()),
    }
}

pub fn list_messages(folder: &str, limit: u32) -> Result<Vec<Summary>, String> {
    let mut out = with(|s| {
        let mb = s.select(folder).map_err(e)?;
        if mb.exists == 0 { return Ok(vec![]); }
        let start = mb.exists.saturating_sub(limit.saturating_sub(1)).max(1);
        let fetched = s.fetch(format!("{}:*", start), "(UID FLAGS BODY.PEEK[HEADER])").map_err(e)?;
        let parser = MessageParser::default();
        let mut out = Vec::new();
        for f in fetched.iter() {
            let Some(uid) = f.uid else { continue };
            let Some(h) = f.header() else { continue };
            let Some(m) = parser.parse(h) else { continue };
            out.push(Summary {
                uid,
                from: addr_label(m.from()).0,
                subject: m.subject().unwrap_or("(sans objet)").to_string(),
                date: m.date().map(|d| d.to_rfc3339()).unwrap_or_default(),
                seen: f.flags().iter().any(|fl| matches!(fl, imap::types::Flag::Seen)),
            });
        }
        Ok(out)
    })?;
    out.sort_by(|a, b| b.uid.cmp(&a.uid));
    Ok(out)
}

pub fn get_message(folder: &str, uid: u32) -> Result<Full, String> {
    let raw = with(|s| {
        s.select(folder).map_err(e)?;
        let fetched = s.uid_fetch(uid.to_string(), "(FLAGS BODY.PEEK[])").map_err(e)?;
        let raw = fetched.iter().next().and_then(|f| f.body()).ok_or("message introuvable")?.to_vec();
        let _ = s.uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)");
        Ok(raw)
    })?;
    let m = MessageParser::default().parse(&raw).ok_or("message illisible")?;
    let (from, from_addr) = addr_label(m.from());
    let to = m.to().map(|a| a.iter().filter_map(|x| x.address()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    Ok(Full {
        uid, from, from_addr, to,
        subject: m.subject().unwrap_or("(sans objet)").to_string(),
        date: m.date().map(|d| d.to_rfc3339()).unwrap_or_default(),
        message_id: m.message_id().unwrap_or("").to_string(),
        text: m.body_text(0).map(|t| t.to_string()).unwrap_or_default(),
        html: m.body_html(0).map(|t| t.to_string()),
    })
}

pub fn delete(folder: &str, uid: u32) -> Result<(), String> {
    with(|s| {
        s.select(folder).map_err(e)?;
        if folder != "Trash" && s.uid_mv(uid.to_string(), "Trash").is_ok() { return Ok(()); }
        s.uid_store(uid.to_string(), "+FLAGS (\\Deleted)").map_err(e)?;
        s.expunge().map_err(e)?;
        Ok(())
    })
}

fn smtp_transport(c: &Config) -> Result<SmtpTransport, String> {
    // 465 = TLS implicite, sinon (587…) STARTTLS
    let b = if c.smtp_port == 465 { SmtpTransport::relay(&c.smtp_host) } else { SmtpTransport::starttls_relay(&c.smtp_host) }.map_err(e)?;
    Ok(b.port(c.smtp_port).credentials(Credentials::new(c.user.clone(), c.pass.clone())).build())
}

/// Vérifie IMAP puis SMTP avec ces réglages, sans rien enregistrer.
pub fn test_login(c: &Config) -> Result<(), String> {
    connect(c).map_err(|x| format!("IMAP : {x}"))?.logout().ok();
    match smtp_transport(c)?.test_connection() {
        Ok(true) => Ok(()),
        Ok(false) => Err("SMTP : connexion refusée".into()),
        Err(x) => Err(format!("SMTP : {x}")),
    }
}

fn message_id(domain: &str) -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}.{:x}@{}", n, std::process::id(), domain)
}

pub fn send(to: &str, subject: &str, body: &str, in_reply_to: Option<&str>) -> Result<(), String> {
    use lettre::message::SinglePart;
    let c = config::load()?;
    let from: Mailbox = c.user.parse().map_err(e)?;
    let domain = c.user.rsplit('@').next().unwrap_or("localhost");
    let mut b = Message::builder().from(from).subject(subject).message_id(Some(format!("<{}>", message_id(domain))));
    for addr in to.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        b = b.to(addr.parse().map_err(|x| format!("adresse invalide « {addr} » : {x}"))?);
    }
    if let Some(id) = in_reply_to.filter(|s| !s.is_empty()) {
        b = b.in_reply_to(format!("<{id}>")).references(format!("<{id}>"));
    }
    // text/plain; charset=utf-8 + MIME-Version : sans ça les filtres anti-spam (Proton, Gmail) sont sévères
    let msg = b.singlepart(SinglePart::plain(body.to_string())).map_err(e)?;
    smtp_transport(&c)?.send(&msg).map_err(e)?;
    // Fastmail n'archive pas les envois SMTP : copie dans "Sent", en tâche de fond pour ne pas bloquer l'UI.
    let raw = msg.formatted();
    std::thread::spawn(move || {
        let _ = with(|s| s.append_with_flags("Sent", &raw, &[imap::types::Flag::Seen]).map_err(e));
    });
    Ok(())
}

#[cfg(test)]
mod login_tests {
    use super::*;
    use crate::config;
    #[test]
    fn login_roundtrip() {
        let txt = std::fs::read_to_string("../.env").unwrap();
        let g = |k: &str| txt.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap().to_string();
        let mut c = Config { user: g("MAIL_USER"), pass: g("MAIL_PASS"), imap_host: "imap.fastmail.com".into(), imap_port: 993, smtp_host: "smtp.fastmail.com".into(), smtp_port: 465 };
        test_login(&c).unwrap(); println!("login OK (465)");
        c.smtp_port = 587; test_login(&c).unwrap(); println!("login OK (587 starttls)");
        let bad = Config { pass: "mauvais".into(), ..c.clone() };
        println!("mauvais mdp -> {:?}", test_login(&bad).err());
        let s = config::Settings { email: c.user.clone(), imap_host: c.imap_host.clone(), imap_port: 993, smtp_host: c.smtp_host.clone(), smtp_port: 465 };
        match config::save(&s, &c.pass) { Ok(()) => { println!("save OK, load={}", config::load().is_ok()); config::clear(); println!("cleared, settings={}", config::read_settings().is_none()); }
            Err(e) => println!("save KO: {e}") }
    }
}
