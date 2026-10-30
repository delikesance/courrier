use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, fs, path::PathBuf, sync::Mutex};

const SERVICE: &str = "dev.angeloclauin.courrier";

/// Réglages non secrets, stockés dans <config>/courrier/settings.json. Le mot de passe va dans le trousseau système.
#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

#[derive(Clone)]
pub struct Config {
    pub user: String,
    pub pass: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

static CACHE: Mutex<Option<Config>> = Mutex::new(None);

fn dir() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("courrier")
}
fn settings_path() -> PathBuf { dir().join("settings.json") }

fn entry(email: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, email).map_err(|e| e.to_string())
}

pub fn read_settings() -> Option<Settings> {
    serde_json::from_str(&fs::read_to_string(settings_path()).ok()?).ok()
}

pub fn save(s: &Settings, pass: &str) -> Result<(), String> {
    entry(&s.email)?.set_password(pass).map_err(|e| format!("trousseau système : {e}"))?;
    fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    fs::write(settings_path(), serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    crate::mail::reset();
    *CACHE.lock().unwrap() = Some(Config {
        user: s.email.clone(), pass: pass.to_string(), imap_host: s.imap_host.clone(),
        imap_port: s.imap_port, smtp_host: s.smtp_host.clone(), smtp_port: s.smtp_port,
    });
    Ok(())
}

pub fn clear() {
    if let Some(s) = read_settings() { if let Ok(e) = entry(&s.email) { let _ = e.delete_credential(); } }
    let _ = fs::remove_file(settings_path());
    *CACHE.lock().unwrap() = None;
    crate::mail::reset();
}

/// Mode dev : COURRIER_ENV=/chemin/.env pour contourner l'écran de connexion.
fn from_env_file() -> Option<Config> {
    let txt = fs::read_to_string(env::var("COURRIER_ENV").ok()?).ok()?;
    let m: HashMap<_, _> = txt.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    let g = |k: &str, d: &str| m.get(k).cloned().unwrap_or_else(|| d.to_string());
    Some(Config {
        user: g("MAIL_USER", ""), pass: g("MAIL_PASS", ""), imap_host: g("IMAP_HOST", "imap.fastmail.com"),
        imap_port: g("IMAP_PORT", "993").parse().unwrap_or(993), smtp_host: g("SMTP_HOST", "smtp.fastmail.com"),
        smtp_port: g("SMTP_PORT", "465").parse().unwrap_or(465),
    })
}

pub fn load() -> Result<Config, String> {
    if let Some(c) = CACHE.lock().unwrap().clone() { return Ok(c); }
    if let Some(c) = from_env_file() { return Ok(c); }
    let s = read_settings().ok_or("non connecté")?;
    let pass = entry(&s.email)?.get_password().map_err(|e| format!("mot de passe introuvable dans le trousseau : {e}"))?;
    let c = Config { user: s.email, pass, imap_host: s.imap_host, imap_port: s.imap_port, smtp_host: s.smtp_host, smtp_port: s.smtp_port };
    *CACHE.lock().unwrap() = Some(c.clone());
    Ok(c)
}
