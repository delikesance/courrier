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

## Mises à jour
Tauri updater : l'app lit `releases/latest/download/latest.json` (signé par la clé `~/.tauri/courrier.key`, secret `TAURI_SIGNING_PRIVATE_KEY`). Ne pas perdre cette clé. Fonctionne sur Windows (NSIS/MSI) et Linux AppImage ; les paquets .deb/.rpm se mettent à jour via la release.
