import { invoke } from "@tauri-apps/api/core";
import "./style.css";

type Summary = { uid: number; from: string; subject: string; date: string; seen: boolean };
type Full = { uid: number; from: string; from_addr: string; to: string; subject: string; date: string; message_id: string; text: string; html: string | null };

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const esc = (s: string) => s.replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]!));
const LABELS: Record<string, string> = { INBOX: "Boîte de réception", Sent: "Envoyés", Drafts: "Brouillons", Trash: "Corbeille", Archive: "Archives", Spam: "Indésirables", Junk: "Indésirables" };
const label = (f: string) => LABELS[f] ?? f;

let folder = "INBOX";
let current: Full | null = null;

function fmtDate(iso: string) {
  if (!iso) return "";
  const d = new Date(iso), now = new Date();
  return d.toDateString() === now.toDateString()
    ? d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleDateString("fr-FR", { day: "numeric", month: "short" });
}

async function loadFolders() {
  const folders = await invoke<{ name: string }[]>("list_folders");
  $("folders").innerHTML = folders.map(f => `<a data-f="${esc(f.name)}" class="${f.name === folder ? "on" : ""}">${esc(label(f.name))}</a>`).join("");
}

async function loadMessages() {
  $("folder-title").textContent = label(folder);
  const ul = $("messages");
  ul.innerHTML = `<li class="subj">Chargement…</li>`;
  try {
    const msgs = await invoke<Summary[]>("list_messages", { folder, limit: 50 });
    ul.innerHTML = msgs.length ? msgs.map(m => `
      <li data-uid="${m.uid}" class="${m.seen ? "" : "unread"}">
        <div class="top"><span class="from">${esc(m.from || "—")}</span><span class="date">${fmtDate(m.date)}</span></div>
        <div class="subj">${esc(m.subject)}</div></li>`).join("") : `<li class="subj">Aucun message</li>`;
  } catch (e) { ul.innerHTML = `<li class="subj">Erreur : ${esc(String(e))}</li>`; }
}

async function openMessage(uid: number) {
  const m = await invoke<Full>("get_message", { folder, uid });
  current = m;
  document.querySelectorAll("#messages li").forEach(li => li.classList.toggle("on", (li as HTMLElement).dataset.uid == String(uid)));
  document.querySelector(`#messages li[data-uid="${uid}"]`)?.classList.remove("unread");
  const r = $("reader");
  r.innerHTML = `<div class="head"><h1>${esc(m.subject)}</h1>
    <div class="meta">${esc(m.from)} &lt;${esc(m.from_addr)}&gt; · ${m.date ? new Date(m.date).toLocaleString("fr-FR") : ""}</div>
    <div class="meta">À : ${esc(m.to)}</div>
    <div class="actions"><button id="a-reply">Répondre</button><button id="a-del" class="ghost">Supprimer</button></div></div>`;
  if (m.html) {
    // iframe sandboxée sans scripts, CSP bloquant tout contenu distant (pas de pixel espion)
    const f = document.createElement("iframe");
    f.setAttribute("sandbox", "");
    f.srcdoc = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:"><base target="_blank"><style>body{font:14px system-ui;margin:20px;color:#111}</style>${m.html}`;
    r.appendChild(f);
  } else {
    const p = document.createElement("pre"); p.textContent = m.text; r.appendChild(p);
  }
  $("a-reply").onclick = () => openComposer(m.from_addr, /^re:/i.test(m.subject) ? m.subject : `Re: ${m.subject}`,
    `\n\n\nLe ${new Date(m.date).toLocaleString("fr-FR")}, ${m.from} a écrit :\n` + m.text.split("\n").map(l => "> " + l).join("\n"), m.message_id);
  $("a-del").onclick = async () => { await invoke("delete_message", { folder, uid }); r.innerHTML = `<div class="empty">Message supprimé</div>`; loadMessages(); };
}

const dlg = $<HTMLDialogElement>("composer");
let replyTo = "";
function openComposer(to = "", subject = "", body = "", inReplyTo = "") {
  $<HTMLInputElement>("c-to").value = to;
  $<HTMLInputElement>("c-subject").value = subject;
  $<HTMLTextAreaElement>("c-body").value = body;
  $("c-status").textContent = ""; replyTo = inReplyTo;
  dlg.showModal();
  (to ? $<HTMLTextAreaElement>("c-body") : $<HTMLInputElement>("c-to")).focus();
  if (to) $<HTMLTextAreaElement>("c-body").setSelectionRange(0, 0);
}

$("compose-btn").onclick = () => openComposer();
$("c-cancel").onclick = () => dlg.close();
$("refresh").onclick = loadMessages;
$("compose-form").addEventListener("submit", async ev => {
  ev.preventDefault();
  const btn = $<HTMLButtonElement>("c-send"); btn.disabled = true; $("c-status").textContent = "Envoi…";
  try {
    await invoke("send_mail", { to: $<HTMLInputElement>("c-to").value, subject: $<HTMLInputElement>("c-subject").value, body: $<HTMLTextAreaElement>("c-body").value, inReplyTo: replyTo });
    dlg.close(); if (folder === "Sent") loadMessages();
  } catch (e) { $("c-status").textContent = String(e); }
  btn.disabled = false;
});
dlg.addEventListener("keydown", e => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) $<HTMLFormElement>("compose-form").requestSubmit(); });

$("folders").addEventListener("click", e => {
  const a = (e.target as HTMLElement).closest("a"); if (!a) return;
  folder = a.dataset.f!;
  document.querySelectorAll("#folders a").forEach(x => x.classList.toggle("on", x === a));
  $("reader").innerHTML = `<div class="empty">Sélectionne un message</div>`;
  loadMessages();
});
$("messages").addEventListener("click", e => {
  const li = (e.target as HTMLElement).closest("li"); if (li?.dataset.uid) openMessage(+li.dataset.uid).catch(err => alert(err));
});

(async () => {
  try { $("account").textContent = await invoke<string>("account"); await loadFolders(); await loadMessages(); }
  catch (e) { $("messages").innerHTML = `<li class="subj">${esc(String(e))}</li>`; }
})();
