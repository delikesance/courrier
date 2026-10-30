mod config;
mod mail;

use serde::Serialize;

#[derive(Serialize)]
struct Folder { name: String }

#[tauri::command]
async fn list_folders() -> Result<Vec<Folder>, String> {
    tauri::async_runtime::spawn_blocking(|| mail::list_folders().map(|v| v.into_iter().map(|name| Folder { name }).collect()))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_messages(folder: String, limit: u32) -> Result<Vec<mail::Summary>, String> {
    tauri::async_runtime::spawn_blocking(move || mail::list_messages(&folder, limit))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_message(folder: String, uid: u32) -> Result<mail::Full, String> {
    tauri::async_runtime::spawn_blocking(move || mail::get_message(&folder, uid))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn send_mail(to: String, subject: String, body: String, in_reply_to: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || mail::send(&to, &subject, &body, in_reply_to.as_deref()))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn delete_message(folder: String, uid: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || mail::delete(&folder, uid))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn account() -> Result<String, String> { config::load().map(|c| c.user) }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_folders, list_messages, get_message, send_mail, delete_message, account])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Courrier");
}
