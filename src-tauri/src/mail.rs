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

#[derive(Serialize)]
pub struct Summary { pub uid: u32, pub from: String, pub subject: String, pub date: String, pub seen: bool }

#[derive(Serialize)]
pub struct Full {
    pub uid: u32, pub from: String, pub from_addr: String, pub to: String, pub subject: String,
    pub date: String, pub message_id: String, pub text: String, pub html: Option<String>,
}

pub fn list_folders() -> Result<Vec<String>, String> {
    let mut s = connect(&config::load()?)?;
    let names = s.list(None, Some("*")).map_err(e)?;
    let mut v: Vec<String> = names.iter().map(|n| n.name().to_string()).collect();
    let _ = s.logout();
    // INBOX d'abord, puis ordre alphabétique
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
    let mut s = connect(&config::load()?)?;
    let mb = s.select(folder).map_err(e)?;
    if mb.exists == 0 { let _ = s.logout(); return Ok(vec![]); }
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
    let _ = s.logout();
    out.sort_by(|a, b| b.uid.cmp(&a.uid));
    Ok(out)
}

pub fn get_message(folder: &str, uid: u32) -> Result<Full, String> {
    let mut s = connect(&config::load()?)?;
    s.select(folder).map_err(e)?;
    let fetched = s.uid_fetch(uid.to_string(), "BODY.PEEK[]").map_err(e)?;
    let raw = fetched.iter().next().and_then(|f| f.body()).ok_or("message introuvable")?.to_vec();
    let _ = s.uid_store(uid.to_string(), "+FLAGS (\\Seen)");
    let _ = s.logout();
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
    let mut s = connect(&config::load()?)?;
    s.select(folder).map_err(e)?;
    // Déplace vers la corbeille Fastmail ("Trash"), sinon marque supprimé.
    if folder != "Trash" && s.uid_mv(uid.to_string(), "Trash").is_ok() { let _ = s.logout(); return Ok(()); }
    s.uid_store(uid.to_string(), "+FLAGS (\\Deleted)").map_err(e)?;
    s.expunge().map_err(e)?;
    let _ = s.logout();
    Ok(())
}

pub fn send(to: &str, subject: &str, body: &str, in_reply_to: Option<&str>) -> Result<(), String> {
    let c = config::load()?;
    let from: Mailbox = c.user.parse().map_err(e)?;
    let mut b = Message::builder().from(from).subject(subject);
    for addr in to.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        b = b.to(addr.parse().map_err(|x| format!("adresse invalide « {addr} » : {x}"))?);
    }
    if let Some(id) = in_reply_to.filter(|s| !s.is_empty()) {
        b = b.in_reply_to(format!("<{id}>")).references(format!("<{id}>"));
    }
    let msg = b.body(body.to_string()).map_err(e)?;
    let mailer = SmtpTransport::relay(&c.smtp_host).map_err(e)?
        .credentials(Credentials::new(c.user.clone(), c.pass.clone())).build();
    mailer.send(&msg).map_err(e)?;
    // Fastmail n'archive pas les envois SMTP : on dépose une copie dans "Sent".
    if let Ok(mut s) = connect(&c) {
        let _ = s.append_with_flags("Sent", msg.formatted(), &[imap::types::Flag::Seen]);
        let _ = s.logout();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_read() {
        let f = list_folders().unwrap(); println!("folders: {f:?}");
        let l = list_messages("INBOX", 10).unwrap();
        for m in &l { println!("{} | {} | {} | seen={}", m.uid, m.from, m.subject, m.seen); }
        if let Some(m) = l.first() { let full = get_message("INBOX", m.uid).unwrap(); println!("text {} chars, html {}", full.text.len(), full.html.is_some()); }
    }
}
