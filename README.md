# Courrier

Client mail sobre (IMAP/SMTP) en Tauri 2 + Rust, pour Windows et Linux.

## Config
Créer `.env` (ou `~/.config/courrier/.env`) :
```
MAIL_USER=...
MAIL_PASS=...   # mot de passe d'application
IMAP_HOST=imap.fastmail.com
SMTP_HOST=smtp.fastmail.com
```

## Dev
`npm install && npx tauri dev`

## Release
`git tag v0.1.0 && git push --tags` : la GitHub Action construit et publie les installeurs.
