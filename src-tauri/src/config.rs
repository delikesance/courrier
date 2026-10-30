use std::{collections::HashMap, env, fs, path::PathBuf};

#[derive(Clone)]
pub struct Config {
    pub user: String,
    pub pass: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
}

/// Cherche un fichier de config : $COURRIER_ENV, ~/.config/courrier/.env, puis .env en remontant depuis le cwd.
fn candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = env::var("COURRIER_ENV") { v.push(p.into()); }
    if let Some(d) = dirs_config() { v.push(d.join("courrier").join(".env")); }
    if let Ok(mut d) = env::current_dir() {
        loop {
            v.push(d.join(".env"));
            if !d.pop() { break; }
        }
    }
    v
}

fn dirs_config() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

pub fn load() -> Result<Config, String> {
    let mut map = HashMap::new();
    for p in candidates() {
        if let Ok(txt) = fs::read_to_string(&p) {
            for l in txt.lines() {
                if let Some((k, v)) = l.split_once('=') { map.insert(k.trim().to_string(), v.trim().to_string()); }
            }
            break;
        }
    }
    let get = |k: &str, d: &str| map.get(k).cloned().or_else(|| env::var(k).ok()).unwrap_or_else(|| d.to_string());
    let user = get("MAIL_USER", "");
    let pass = get("MAIL_PASS", "");
    if user.is_empty() || pass.is_empty() { return Err("MAIL_USER / MAIL_PASS introuvables (.env)".into()); }
    Ok(Config {
        user, pass,
        imap_host: get("IMAP_HOST", "imap.fastmail.com"),
        imap_port: get("IMAP_PORT", "993").parse().unwrap_or(993),
        smtp_host: get("SMTP_HOST", "smtp.fastmail.com"),
    })
}
