pub const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>innernet admin</title>
<style>
  :root {
    --bg: #0d1017;
    --surface: #151922;
    --surface-2: #1c212c;
    --surface-3: #232936;
    --line: #29303e;
    --line-soft: #1f2530;
    --text: #e8ebf2;
    --text-dim: #9aa3b5;
    --text-faint: #6b7488;
    --accent: #4d8df6;
    --accent-dim: #2f5cae;
    --ok: #3fb950;
    --warn: #d29922;
    --err: #f85149;
    --radius: 10px;
    --mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
  }

  * { box-sizing: border-box; }

  html, body { height: 100%; }

  body {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font: 14px/1.55 ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    -webkit-font-smoothing: antialiased;
  }

  /* ---------- layout ---------- */

  .shell { max-width: 1120px; margin: 0 auto; padding: 0 24px 96px; }

  .masthead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 26px 0 0;
  }

  .brand { display: flex; align-items: center; gap: 11px; }

  .brand-mark {
    width: 30px; height: 30px;
    border-radius: 8px;
    background: linear-gradient(150deg, var(--accent), var(--accent-dim));
    display: grid;
    place-items: center;
    flex: none;
  }

  .brand-mark svg { width: 17px; height: 17px; display: block; }

  .brand-text { display: flex; flex-direction: column; line-height: 1.25; }

  .brand-name { font-size: 15px; font-weight: 600; letter-spacing: -.01em; }

  .brand-sub { font-size: 11.5px; color: var(--text-faint); }

  .masthead-actions { display: flex; align-items: center; gap: 8px; }

  /* ---------- tabs ---------- */

  .tabs {
    display: flex;
    gap: 2px;
    margin: 22px 0 20px;
    border-bottom: 1px solid var(--line);
  }

  .tab {
    appearance: none;
    background: none;
    border: 0;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    padding: 9px 13px;
    font: inherit;
    font-size: 13px;
    color: var(--text-faint);
    cursor: pointer;
    transition: color .12s, border-color .12s;
  }

  .tab:hover { color: var(--text-dim); }

  .tab[aria-selected="true"] {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  .tab .count {
    display: inline-block;
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 20px;
    background: var(--surface-3);
    color: var(--text-faint);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .tab[aria-selected="true"] .count { color: var(--text-dim); }

  /* ---------- section header ---------- */

  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 14px;
  }

  .section-note {
    color: var(--text-faint);
    font-size: 12.5px;
    max-width: 62ch;
    margin: 0;
  }

  /* ---------- panels & tables ---------- */

  .panel {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }

  table { width: 100%; border-collapse: collapse; }

  thead th {
    text-align: left;
    padding: 10px 16px;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: .05em;
    text-transform: uppercase;
    color: var(--text-faint);
    background: var(--surface-2);
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  tbody td {
    padding: 13px 16px;
    border-bottom: 1px solid var(--line-soft);
    vertical-align: middle;
  }

  tbody tr:last-child td { border-bottom: 0; }

  tbody tr:hover { background: rgba(255,255,255,.014); }

  .cell-name { display: flex; flex-direction: column; gap: 4px; }

  .cell-name .primary { font-weight: 500; }

  .badges { display: flex; gap: 5px; flex-wrap: wrap; }

  .mono { font-family: var(--mono); font-size: 12.5px; }

  .addr { color: var(--text-dim); }

  .faint { color: var(--text-faint); }
  .small { font-size: 12px; }
  .tiny { font-size: 11.5px; }

  .sprite { position: absolute; }

  .w-peer { width: 210px; }
  .w-addr { width: 150px; }
  .w-state { width: 120px; }
  .w-actions { width: 190px; }
  .w-name { width: 220px; }

  /* ---------- badges ---------- */

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 20px;
    font-size: 11px;
    font-weight: 500;
    line-height: 1.6;
    white-space: nowrap;
  }

  .badge::before {
    content: "";
    width: 5px; height: 5px;
    border-radius: 50%;
    background: currentColor;
  }

  .badge.plain::before { display: none; }

  .badge.ok    { background: rgba(63,185,80,.13);  color: var(--ok); }
  .badge.off   { background: rgba(154,163,181,.13); color: var(--text-dim); }
  .badge.admin { background: rgba(210,153,34,.13); color: var(--warn); }
  .badge.you   { background: rgba(77,141,246,.13); color: var(--accent); }

  /* ---------- route chips ---------- */

  .routes { display: flex; flex-direction: column; gap: 8px; }

  .route-list { display: flex; flex-wrap: wrap; gap: 6px; }

  .route {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px 3px 9px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface-2);
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
  }

  .route button {
    appearance: none;
    background: none;
    border: 0;
    padding: 0 4px;
    font-size: 14px;
    line-height: 1;
    color: var(--text-faint);
    cursor: pointer;
    border-radius: 4px;
  }

  .route button:hover { color: var(--err); background: rgba(248,81,73,.1); }

  .route-add { display: flex; gap: 6px; max-width: 260px; }

  .route-add input {
    font-family: var(--mono);
    font-size: 12px;
    padding: 5px 8px;
  }

  .route-add button { padding: 5px 11px; font-size: 12px; }

  /* ---------- controls ---------- */

  input, select {
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--text);
    border-radius: 7px;
    padding: 8px 10px;
    font: inherit;
    font-size: 13px;
    width: 100%;
    transition: border-color .12s;
  }

  input::placeholder { color: var(--text-faint); }

  input:focus, select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px rgba(77,141,246,.12);
  }

  select { cursor: pointer; }

  button {
    appearance: none;
    background: var(--accent);
    color: #fff;
    border: 0;
    border-radius: 7px;
    padding: 8px 14px;
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: filter .12s, background .12s, border-color .12s;
  }

  button:hover { filter: brightness(1.12); }
  button:active { filter: brightness(.95); }
  button:disabled { opacity: .45; cursor: default; filter: none; }

  button.ghost {
    background: transparent;
    border: 1px solid var(--line);
    color: var(--text-dim);
  }

  button.ghost:hover {
    background: var(--surface-2);
    color: var(--text);
    filter: none;
  }

  button.subtle {
    background: none;
    border: 0;
    color: var(--text-faint);
    padding: 5px 7px;
    font-size: 12px;
  }

  button.subtle:hover { color: var(--text); background: var(--surface-3); filter: none; }

  button.subtle.danger:hover { color: var(--err); background: rgba(248,81,73,.1); }

  button.danger { background: var(--err); }

  .actions-cell { display: flex; gap: 2px; justify-content: flex-end; }

  /* ---------- empty state ---------- */

  .empty {
    padding: 44px 20px;
    text-align: center;
    color: var(--text-faint);
    font-size: 13px;
  }

  .empty strong { display: block; color: var(--text-dim); font-weight: 500; margin-bottom: 4px; }

  /* ---------- modal ---------- */

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(5,7,11,.74);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    padding: 24px;
    overflow: auto;
  }

  .dialog {
    width: 100%;
    max-width: 500px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0,0,0,.5);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
  }

  .dialog.wide { max-width: 620px; }

  .dialog-head {
    padding: 18px 20px 14px;
    border-bottom: 1px solid var(--line-soft);
  }

  .dialog-title { font-size: 14.5px; font-weight: 600; margin: 0 0 3px; }

  .dialog-note { font-size: 12.5px; color: var(--text-faint); margin: 0; }

  .dialog-body { padding: 18px 20px; overflow: auto; }

  .dialog-foot {
    padding: 14px 20px;
    border-top: 1px solid var(--line-soft);
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .field { margin-bottom: 15px; }

  .field:last-child { margin-bottom: 0; }

  .field > label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 6px;
  }

  .field .optional { color: var(--text-faint); font-weight: 400; }

  .check {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    font-size: 13px;
    color: var(--text-dim);
    cursor: pointer;
  }

  .check input {
    width: 15px; height: 15px;
    margin: 2px 0 0;
    accent-color: var(--accent);
    flex: none;
    cursor: pointer;
  }

  .check-text { display: flex; flex-direction: column; gap: 1px; }

  .check-text small { color: var(--text-faint); font-size: 11.5px; }

  /* ---------- invitation output ---------- */

  .notice {
    display: flex;
    gap: 9px;
    padding: 11px 13px;
    border-radius: 8px;
    font-size: 12.5px;
    line-height: 1.5;
    margin-bottom: 15px;
  }

  .notice.warn {
    background: rgba(210,153,34,.09);
    border: 1px solid rgba(210,153,34,.28);
    color: #e8c06a;
  }

  .notice svg { width: 15px; height: 15px; flex: none; margin-top: 1px; }

  .invite {
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 13px 15px;
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.65;
    color: var(--text-dim);
    white-space: pre;
    overflow: auto;
    max-height: 300px;
    tab-size: 2;
  }

  /* ---------- login ---------- */

  .login-wrap {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px;
  }

  .login-card { width: 100%; max-width: 360px; }

  .login-head { text-align: center; margin-bottom: 26px; }

  .login-head .brand-mark {
    width: 40px; height: 40px;
    border-radius: 11px;
    margin: 0 auto 14px;
  }

  .login-head .brand-mark svg { width: 22px; height: 22px; }

  .login-title { font-size: 17px; font-weight: 600; margin: 0 0 5px; }

  .login-sub { font-size: 13px; color: var(--text-faint); margin: 0; }

  .login-card .field { margin-bottom: 13px; }

  .login-card button { width: 100%; padding: 10px; }

  /* ---------- toast ---------- */

  #toast {
    position: fixed;
    left: 50%;
    bottom: 26px;
    transform: translate(-50%, 12px);
    z-index: 90;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 11px 16px;
    border-radius: 9px;
    font-size: 13px;
    background: var(--surface-3);
    border: 1px solid var(--line);
    color: var(--text);
    box-shadow: 0 10px 32px rgba(0,0,0,.45);
    opacity: 0;
    pointer-events: none;
    transition: opacity .18s, transform .18s;
    max-width: min(420px, calc(100vw - 40px));
  }

  #toast.show { opacity: 1; transform: translate(-50%, 0); }

  #toast::before {
    content: "";
    width: 7px; height: 7px;
    border-radius: 50%;
    flex: none;
    background: var(--ok);
  }

  #toast.err { border-color: rgba(248,81,73,.5); }
  #toast.err::before { background: var(--err); }

  .hidden { display: none !important; }

  @media (max-width: 720px) {
    .shell { padding: 0 14px 72px; }
    .masthead { padding-top: 18px; }
    thead th, tbody td { padding-left: 12px; padding-right: 12px; }
    .section-head { flex-direction: column; gap: 10px; }
    .hide-sm { display: none; }
  }
</style>
</head>
<body>

<svg width="0" height="0" class="sprite" aria-hidden="true">
  <symbol id="i-shield" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z"/>
  </symbol>
  <symbol id="i-alert" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <path d="M12 9v4M12 17h.01M10.3 3.9L2.4 17a2 2 0 001.7 3h15.8a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z"/>
  </symbol>
</svg>

<!-- ============ login ============ -->
<div id="loginView" class="login-wrap">
  <div class="login-card">
    <div class="login-head">
      <div class="brand-mark"><svg><use href="#i-shield"/></svg></div>
      <h1 class="login-title">innernet admin</h1>
      <p class="login-sub">Sign in to manage this network.</p>
    </div>
    <div class="field">
      <label for="loginUser">Username</label>
      <input id="loginUser" autocomplete="username" value="admin" spellcheck="false">
    </div>
    <div class="field">
      <label for="loginPass">Password</label>
      <input id="loginPass" type="password" autocomplete="current-password">
    </div>
    <button id="loginBtn">Sign in</button>
  </div>
</div>

<!-- ============ app ============ -->
<div id="appView" class="shell hidden">
  <header class="masthead">
    <div class="brand">
      <div class="brand-mark"><svg><use href="#i-shield"/></svg></div>
      <div class="brand-text">
        <span class="brand-name" id="networkName">innernet</span>
        <span class="brand-sub">network administration</span>
      </div>
    </div>
    <div class="masthead-actions">
      <button class="ghost" id="refreshBtn">Refresh</button>
      <button class="subtle" id="logoutBtn">Sign out</button>
    </div>
  </header>

  <nav class="tabs" role="tablist">
    <button class="tab" role="tab" data-tab="peers" aria-selected="true">
      Peers<span class="count" id="countPeers">0</span>
    </button>
    <button class="tab" role="tab" data-tab="cidrs" aria-selected="false">
      CIDRs<span class="count" id="countCidrs">0</span>
    </button>
    <button class="tab" role="tab" data-tab="assoc" aria-selected="false">
      Associations<span class="count" id="countAssoc">0</span>
    </button>
  </nav>

  <!-- peers -->
  <section id="panePeers" role="tabpanel">
    <div class="section-head">
      <p class="section-note">
        Every peer is a device holding one address. Subnet routes are pushed as
        WireGuard allowed-ips and take effect immediately.
      </p>
      <button id="inviteBtn">Invite a peer</button>
    </div>
    <div class="panel">
      <table>
        <thead>
          <tr>
            <th class="w-peer">Peer</th>
            <th class="w-addr">Address</th>
            <th>Subnet routes</th>
            <th class="w-state">State</th>
            <th class="w-actions"></th>
          </tr>
        </thead>
        <tbody id="peerRows"></tbody>
      </table>
    </div>
  </section>

  <!-- cidrs -->
  <section id="paneCidrs" role="tabpanel" class="hidden">
    <div class="section-head">
      <p class="section-note">
        CIDRs divide the address space into groups. Peers live in a leaf CIDR;
        a CIDR that has children cannot hold peers directly.
      </p>
      <button id="addCidrBtn">Add a CIDR</button>
    </div>
    <div class="panel">
      <table>
        <thead>
          <tr>
            <th class="w-name">Name</th>
            <th>Range</th>
            <th class="w-actions"></th>
          </tr>
        </thead>
        <tbody id="cidrRows"></tbody>
      </table>
    </div>
  </section>

  <!-- associations -->
  <section id="paneAssoc" role="tabpanel" class="hidden">
    <div class="section-head">
      <p class="section-note">
        Peers can only reach peers in their own CIDR. An association opens a pair
        of CIDRs to each other in both directions.
      </p>
      <button id="addAssocBtn">Add an association</button>
    </div>
    <div class="panel">
      <table>
        <thead>
          <tr>
            <th>CIDR</th>
            <th>CIDR</th>
            <th class="w-actions"></th>
          </tr>
        </thead>
        <tbody id="assocRows"></tbody>
      </table>
    </div>
  </section>
</div>

<!-- ============ dialogs ============ -->

<div id="dlgInvite" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog">
    <div class="dialog-head">
      <h2 class="dialog-title">Invite a peer</h2>
      <p class="dialog-note">
        The invitation carries the device's private key and is shown only once.
      </p>
    </div>
    <div class="dialog-body">
      <div class="field">
        <label for="invName">Name</label>
        <input id="invName" placeholder="laptop" spellcheck="false" autocomplete="off">
      </div>
      <div class="field">
        <label for="invCidr">CIDR</label>
        <select id="invCidr"></select>
      </div>
      <div class="field">
        <label for="invIp">Address <span class="optional">— leave blank for the next free one</span></label>
        <input id="invIp" placeholder="auto" spellcheck="false" autocomplete="off">
      </div>
      <div class="field">
        <label for="invExpires">Invitation expires in hours <span class="optional">— blank means never</span></label>
        <input id="invExpires" type="number" min="1" placeholder="168">
      </div>
      <div class="field">
        <label class="check">
          <input id="invAdmin" type="checkbox">
          <span class="check-text">
            Grant admin rights
            <small>Admins can create, rename and revoke other peers.</small>
          </span>
        </label>
      </div>
    </div>
    <div class="dialog-foot">
      <button class="ghost" data-close="dlgInvite">Cancel</button>
      <button id="inviteSubmit">Create invitation</button>
    </div>
  </div>
</div>

<div id="dlgResult" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog wide">
    <div class="dialog-head">
      <h2 class="dialog-title">Invitation ready</h2>
      <p class="dialog-note" id="resultFor"></p>
    </div>
    <div class="dialog-body">
      <div class="notice warn">
        <svg><use href="#i-alert"/></svg>
        <span>Save this now. The server stores only the public key, so the private
        key below cannot be recovered once you close this dialog.</span>
      </div>
      <div class="invite" id="inviteText"></div>
    </div>
    <div class="dialog-foot">
      <button class="ghost" id="copyInvite">Copy</button>
      <button class="ghost" id="downloadInvite">Download .toml</button>
      <button data-close="dlgResult">Done</button>
    </div>
  </div>
</div>

<div id="dlgRenamePeer" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog">
    <div class="dialog-head">
      <h2 class="dialog-title">Rename peer</h2>
      <p class="dialog-note" id="renamePeerFor"></p>
    </div>
    <div class="dialog-body">
      <div class="field">
        <label for="rnPeer">Name</label>
        <input id="rnPeer" spellcheck="false" autocomplete="off">
      </div>
    </div>
    <div class="dialog-foot">
      <button class="ghost" data-close="dlgRenamePeer">Cancel</button>
      <button id="renamePeerSubmit">Rename</button>
    </div>
  </div>
</div>

<div id="dlgCidr" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog">
    <div class="dialog-head">
      <h2 class="dialog-title">Add a CIDR</h2>
      <p class="dialog-note">Must sit inside the parent's range.</p>
    </div>
    <div class="dialog-body">
      <div class="field">
        <label for="cName">Name</label>
        <input id="cName" placeholder="servers" spellcheck="false" autocomplete="off">
      </div>
      <div class="field">
        <label for="cRange">Range</label>
        <input id="cRange" placeholder="10.60.3.0/24" spellcheck="false" autocomplete="off">
      </div>
      <div class="field">
        <label for="cParent">Parent <span class="optional">— blank for top level</span></label>
        <select id="cParent"></select>
      </div>
    </div>
    <div class="dialog-foot">
      <button class="ghost" data-close="dlgCidr">Cancel</button>
      <button id="cidrSubmit">Add</button>
    </div>
  </div>
</div>

<div id="dlgAssoc" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog">
    <div class="dialog-head">
      <h2 class="dialog-title">Add an association</h2>
      <p class="dialog-note">Peers in these two CIDRs will be able to reach each other.</p>
    </div>
    <div class="dialog-body">
      <div class="field">
        <label for="asA">CIDR</label>
        <select id="asA"></select>
      </div>
      <div class="field">
        <label for="asB">CIDR</label>
        <select id="asB"></select>
      </div>
    </div>
    <div class="dialog-foot">
      <button class="ghost" data-close="dlgAssoc">Cancel</button>
      <button id="assocSubmit">Add</button>
    </div>
  </div>
</div>

<div id="dlgConfirm" class="scrim hidden" role="dialog" aria-modal="true">
  <div class="dialog">
    <div class="dialog-head">
      <h2 class="dialog-title" id="confirmTitle">Are you sure?</h2>
      <p class="dialog-note" id="confirmNote"></p>
    </div>
    <div class="dialog-foot">
      <button class="ghost" data-close="dlgConfirm">Cancel</button>
      <button class="danger" id="confirmYes">Confirm</button>
    </div>
  </div>
</div>

<div id="toast"></div>

<script>
"use strict";

const $ = (id) => document.getElementById(id);

const state = {
  peers: [],
  cidrs: [],
  assoc: [],
  tab: "peers",
  busy: false,
};

/* ------------------------------------------------------------------ utils */

function esc(value) {
  return String(value).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]);
}

let toastTimer = null;
function toast(message, kind) {
  const el = $("toast");
  el.textContent = message;
  el.className = kind === "err" ? "show err" : "show";
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.className = ""; }, kind === "err" ? 6000 : 3000);
}

async function api(path, options) {
  const opts = Object.assign({ headers: {} }, options || {});
  if (opts.body !== undefined && typeof opts.body !== "string") {
    opts.body = JSON.stringify(opts.body);
  }
  if (opts.body !== undefined) opts.headers["Content-Type"] = "application/json";

  const res = await fetch("/web" + path, Object.assign(opts, { credentials: "same-origin" }));

  if (res.status === 401) {
    const err = new Error("unauthorised");
    err.unauthorised = true;
    throw err;
  }

  const text = await res.text();
  let data = null;
  if (text) {
    try { data = JSON.parse(text); } catch { data = text; }
  }

  if (!res.ok) {
    const message = data && data.error ? data.error : (res.status + " " + res.statusText);
    throw new Error(message);
  }
  return data;
}

/* ------------------------------------------------------------------ dialogs */

let openDialogs = [];

function openDialog(id) {
  const el = $(id);
  el.classList.remove("hidden");
  openDialogs.push(id);
  const focusable = el.querySelector("input:not([type=checkbox]), select");
  if (focusable) focusable.focus();
}

function closeDialog(id) {
  $(id).classList.add("hidden");
  openDialogs = openDialogs.filter((d) => d !== id);
}

function closeTopDialog() {
  if (openDialogs.length) closeDialog(openDialogs[openDialogs.length - 1]);
}

document.querySelectorAll("[data-close]").forEach((btn) => {
  btn.addEventListener("click", () => closeDialog(btn.dataset.close));
});

document.querySelectorAll(".scrim").forEach((scrim) => {
  scrim.addEventListener("mousedown", (e) => {
    if (e.target === scrim) closeDialog(scrim.id);
  });
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeTopDialog();
});

/** Ask before a destructive action; resolves true when confirmed. */
function confirmAction(title, note, confirmLabel) {
  return new Promise((resolve) => {
    $("confirmTitle").textContent = title;
    $("confirmNote").textContent = note;
    $("confirmYes").textContent = confirmLabel || "Confirm";
    $("confirmYes").className = "danger";

    const yes = $("confirmYes");
    const done = (value) => {
      yes.removeEventListener("click", onYes);
      closeDialog("dlgConfirm");
      resolve(value);
    };
    const onYes = () => done(true);

    yes.addEventListener("click", onYes);
    openDialog("dlgConfirm");
  });
}

/* ------------------------------------------------------------------ lookups */

function cidrById(id) {
  return state.cidrs.find((c) => c.id === id);
}

function cidrLabel(id) {
  const c = cidrById(id);
  return c ? c.name : ("#" + id);
}

function peersInCidr(cidrId) {
  return state.peers.filter((p) => cidrById(cidrId) && p.ip.startsWith(cidrBase(cidrById(cidrId))));
}

/** Everything up to the last dot, for grouping peers under a CIDR. */
function cidrBase(cidr) {
  const slash = cidr.cidr.indexOf("/");
  const prefix = Number(cidr.cidr.slice(slash + 1));
  const addr = cidr.cidr.slice(0, slash);
  if (!addr.includes(".") || prefix >= 24) {
    const cut = addr.lastIndexOf(".");
    return cut > 0 ? addr.slice(0, cut + 1) : addr;
  }
  const octets = addr.split(".");
  const keep = Math.floor(prefix / 8);
  return octets.slice(0, keep).join(".") + (keep ? "." : "");
}

/* ------------------------------------------------------------------ render */

function setTab(tab) {
  state.tab = tab;
  document.querySelectorAll(".tab").forEach((btn) => {
    btn.setAttribute("aria-selected", String(btn.dataset.tab === tab));
  });
  $("panePeers").classList.toggle("hidden", tab !== "peers");
  $("paneCidrs").classList.toggle("hidden", tab !== "cidrs");
  $("paneAssoc").classList.toggle("hidden", tab !== "assoc");
}

function emptyRow(colspan, title, detail) {
  const tr = document.createElement("tr");
  const td = document.createElement("td");
  td.colSpan = colspan;
  td.className = "empty";
  td.innerHTML = "<strong>" + esc(title) + "</strong>" + esc(detail);
  tr.appendChild(td);
  return tr;
}

function renderPeers() {
  const tbody = $("peerRows");
  tbody.innerHTML = "";
  $("countPeers").textContent = String(state.peers.length);

  if (state.peers.length === 0) {
    tbody.appendChild(emptyRow(5, "No peers yet", "Invite a device to join this network."));
    return;
  }

  for (const peer of state.peers) {
    const tr = document.createElement("tr");

    // name + badges
    const nameCell = document.createElement("td");
    const wrap = document.createElement("div");
    wrap.className = "cell-name";
    const primary = document.createElement("span");
    primary.className = "primary";
    primary.textContent = peer.name;
    wrap.appendChild(primary);

    const badges = document.createElement("div");
    badges.className = "badges";
    if (peer.is_admin) {
      const b = document.createElement("span");
      b.className = "badge plain admin";
      b.textContent = "admin";
      badges.appendChild(b);
    }
    if (!peer.is_redeemed) {
      const b = document.createElement("span");
      b.className = "badge plain off";
      b.textContent = "invited";
      badges.appendChild(b);
    }
    if (badges.children.length) wrap.appendChild(badges);
    nameCell.appendChild(wrap);
    tr.appendChild(nameCell);

    // address
    const addrCell = document.createElement("td");
    addrCell.className = "mono addr";
    addrCell.textContent = peer.ip;
    tr.appendChild(addrCell);

    // routes
    const routeCell = document.createElement("td");
    const routes = document.createElement("div");
    routes.className = "routes";

    const list = document.createElement("div");
    list.className = "route-list";
    if (peer.allowed_subnets.length === 0) {
      const none = document.createElement("span");
      none.className = "faint";
      none.className = "faint small";
      none.textContent = "None";
      list.appendChild(none);
    }
    for (const subnet of peer.allowed_subnets) {
      const chip = document.createElement("span");
      chip.className = "route";
      chip.appendChild(document.createTextNode(subnet));
      const x = document.createElement("button");
      x.type = "button";
      x.innerHTML = "&times;";
      x.title = "Remove this route";
      x.setAttribute("aria-label", "Remove route " + subnet);
      x.addEventListener("click", () => {
        setSubnets(peer, peer.allowed_subnets.filter((s) => s !== subnet));
      });
      chip.appendChild(x);
      list.appendChild(chip);
    }
    routes.appendChild(list);

    const addRow = document.createElement("div");
    addRow.className = "route-add";
    const input = document.createElement("input");
    input.placeholder = "192.168.1.0/24";
    input.spellcheck = false;
    input.setAttribute("aria-label", "Subnet to advertise for " + peer.name);
    const addBtn = document.createElement("button");
    addBtn.type = "button";
    addBtn.className = "ghost";
    addBtn.textContent = "Add";
    const submit = () => {
      const value = input.value.trim();
      if (!value) return;
      if (!/^[0-9a-fA-F:.]+\/\d{1,3}$/.test(value)) {
        toast("That is not a CIDR: " + value, "err");
        return;
      }
      if (peer.allowed_subnets.includes(value)) {
        toast("That route is already advertised", "err");
        return;
      }
      setSubnets(peer, peer.allowed_subnets.concat([value]));
    };
    addBtn.addEventListener("click", submit);
    input.addEventListener("keydown", (e) => { if (e.key === "Enter") submit(); });
    addRow.appendChild(input);
    addRow.appendChild(addBtn);
    routes.appendChild(addRow);

    routeCell.appendChild(routes);
    tr.appendChild(routeCell);

    // state
    const stateCell = document.createElement("td");
    const badge = document.createElement("span");
    badge.className = "badge " + (peer.is_disabled ? "off" : "ok");
    badge.textContent = peer.is_disabled ? "disabled" : "active";
    stateCell.appendChild(badge);
    tr.appendChild(stateCell);

    // actions
    const actionCell = document.createElement("td");
    const actions = document.createElement("div");
    actions.className = "actions-cell";

    const rename = document.createElement("button");
    rename.type = "button";
    rename.className = "subtle";
    rename.textContent = "Rename";
    rename.addEventListener("click", () => {
      $("rnPeer").value = peer.name;
      $("renamePeerFor").textContent = peer.name + " · " + peer.ip;
      $("renamePeerSubmit").dataset.id = String(peer.id);
      openDialog("dlgRenamePeer");
      $("rnPeer").select();
    });
    actions.appendChild(rename);

    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "subtle";
    toggle.textContent = peer.is_disabled ? "Enable" : "Disable";
    toggle.addEventListener("click", () => setDisabled(peer, !peer.is_disabled));
    actions.appendChild(toggle);

    if (peer.id !== 1) {
      const del = document.createElement("button");
      del.type = "button";
      del.className = "subtle danger";
      del.textContent = "Delete";
      del.addEventListener("click", async () => {
        const ok = await confirmAction(
          "Delete " + peer.name + "?",
          "The device loses access immediately and its address is freed. This cannot be undone.",
          "Delete peer",
        );
        if (!ok) return;
        try {
          await api("/peers/" + peer.id, { method: "DELETE" });
          toast("Deleted " + peer.name);
          await load();
        } catch (e) {
          toast(e.message, "err");
        }
      });
      actions.appendChild(del);
    }

    actionCell.appendChild(actions);
    tr.appendChild(actionCell);

    tbody.appendChild(tr);
  }
}

function renderCidrs() {
  const tbody = $("cidrRows");
  tbody.innerHTML = "";
  $("countCidrs").textContent = String(state.cidrs.length);

  if (state.cidrs.length === 0) {
    tbody.appendChild(emptyRow(3, "No CIDRs", "Add one to start grouping peers."));
    return;
  }

  const ordered = state.cidrs.slice().sort((a, b) => a.cidr.localeCompare(b.cidr));

  for (const cidr of ordered) {
    const tr = document.createElement("tr");

    const nameCell = document.createElement("td");
    const wrap = document.createElement("div");
    wrap.className = "cell-name";
    const primary = document.createElement("span");
    primary.className = "primary";
    primary.textContent = cidr.name;
    wrap.appendChild(primary);
    if (cidr.parent) {
      const child = document.createElement("span");
      child.className = "faint";
      child.className = "faint tiny";
      child.textContent = "child of " + cidrLabel(cidr.parent);
      wrap.appendChild(child);
    }
    nameCell.appendChild(wrap);
    tr.appendChild(nameCell);

    const rangeCell = document.createElement("td");
    rangeCell.className = "mono addr";
    rangeCell.textContent = cidr.cidr;
    tr.appendChild(rangeCell);

    const actionCell = document.createElement("td");
    const actions = document.createElement("div");
    actions.className = "actions-cell";

    const rename = document.createElement("button");
    rename.type = "button";
    rename.className = "subtle";
    rename.textContent = "Rename";
    rename.addEventListener("click", async () => {
      const next = window.prompt("New name for " + cidr.name, cidr.name);
      if (next === null || !next.trim() || next === cidr.name) return;
      try {
        await api("/cidrs/" + cidr.id + "/name", {
          method: "PUT",
          body: { name: next.trim() },
        });
        toast("Renamed to " + next.trim());
        await load();
      } catch (e) {
        toast(e.message, "err");
      }
    });
    actions.appendChild(rename);

    const del = document.createElement("button");
    del.type = "button";
    del.className = "subtle danger";
    del.textContent = "Delete";
    del.addEventListener("click", async () => {
      const ok = await confirmAction(
        "Delete CIDR " + cidr.name + "?",
        "This fails if the CIDR still holds peers or child CIDRs.",
        "Delete CIDR",
      );
      if (!ok) return;
      try {
        await api("/cidrs/" + cidr.id, { method: "DELETE" });
        toast("Deleted " + cidr.name);
        await load();
      } catch (e) {
        toast(e.message, "err");
      }
    });
    actions.appendChild(del);

    actionCell.appendChild(actions);
    tr.appendChild(actionCell);
    tbody.appendChild(tr);
  }
}

function renderAssoc() {
  const tbody = $("assocRows");
  tbody.innerHTML = "";
  $("countAssoc").textContent = String(state.assoc.length);

  if (state.assoc.length === 0) {
    tbody.appendChild(emptyRow(
      3,
      "No associations",
      "Peers stay restricted to their own CIDR until you add one.",
    ));
    return;
  }

  for (const assoc of state.assoc) {
    const tr = document.createElement("tr");

    const left = document.createElement("td");
    left.textContent = cidrLabel(assoc.cidr_a);
    tr.appendChild(left);

    const right = document.createElement("td");
    right.textContent = cidrLabel(assoc.cidr_b);
    tr.appendChild(right);

    const actionCell = document.createElement("td");
    const actions = document.createElement("div");
    actions.className = "actions-cell";
    const del = document.createElement("button");
    del.type = "button";
    del.className = "subtle danger";
    del.textContent = "Remove";
    del.addEventListener("click", async () => {
      const ok = await confirmAction(
        "Remove this association?",
        cidrLabel(assoc.cidr_a) + " and " + cidrLabel(assoc.cidr_b) + " will no longer reach each other.",
        "Remove",
      );
      if (!ok) return;
      try {
        await api("/associations/" + assoc.id, { method: "DELETE" });
        toast("Association removed");
        await load();
      } catch (e) {
        toast(e.message, "err");
      }
    });
    actions.appendChild(del);
    actionCell.appendChild(actions);
    tr.appendChild(actionCell);

    tbody.appendChild(tr);
  }
}

function fillSelect(select, items, blankLabel) {
  const previous = select.value;
  select.innerHTML = "";
  if (blankLabel !== undefined) {
    const opt = document.createElement("option");
    opt.value = "";
    opt.textContent = blankLabel;
    select.appendChild(opt);
  }
  for (const item of items) {
    const opt = document.createElement("option");
    opt.value = String(item.value);
    opt.textContent = item.label;
    select.appendChild(opt);
  }
  if (previous && Array.from(select.options).some((o) => o.value === previous)) {
    select.value = previous;
  }
}

function renderSelects() {
  const cidrItems = state.cidrs
    .slice()
    .sort((a, b) => a.cidr.localeCompare(b.cidr))
    .map((c) => ({ value: c.id, label: c.name + " — " + c.cidr }));

  fillSelect($("invCidr"), cidrItems);
  fillSelect($("cParent"), cidrItems, "(top level)");
  fillSelect($("asA"), cidrItems);
  fillSelect($("asB"), cidrItems);
}

function render() {
  renderPeers();
  renderCidrs();
  renderAssoc();
  renderSelects();
}

/* ------------------------------------------------------------------ actions */

async function setSubnets(peer, subnets) {
  try {
    const updated = await api("/peers/" + peer.id + "/subnets", {
      method: "PUT",
      body: { allowed_subnets: subnets },
    });
    Object.assign(peer, updated);
    render();
    toast("Routes updated");
  } catch (e) {
    toast(e.message, "err");
  }
}

async function setDisabled(peer, disabled) {
  try {
    const updated = await api("/peers/" + peer.id + "/disabled", {
      method: "PUT",
      body: { is_disabled: disabled },
    });
    Object.assign(peer, updated);
    render();
    toast(disabled ? "Peer disabled" : "Peer enabled");
  } catch (e) {
    toast(e.message, "err");
  }
}

async function load() {
  const data = await Promise.all([
    api("/peers"),
    api("/cidrs"),
    api("/associations"),
  ]);
  state.peers = data[0] || [];
  state.cidrs = data[1] || [];
  state.assoc = data[2] || [];
  render();
}

/* ------------------------------------------------------------------ wiring */

document.querySelectorAll(".tab").forEach((btn) => {
  btn.addEventListener("click", () => setTab(btn.dataset.tab));
});

$("refreshBtn").addEventListener("click", async () => {
  try {
    await load();
    toast("Refreshed");
  } catch (e) {
    toast(e.message, "err");
  }
});

$("logoutBtn").addEventListener("click", async () => {
  try { await api("/logout", { method: "POST" }); } catch { /* ignore */ }
  showLogin();
});

$("loginBtn").addEventListener("click", doLogin);
$("loginPass").addEventListener("keydown", (e) => { if (e.key === "Enter") doLogin(); });
$("loginUser").addEventListener("keydown", (e) => { if (e.key === "Enter") $("loginPass").focus(); });

async function doLogin() {
  const username = $("loginUser").value.trim();
  const password = $("loginPass").value;
  if (!username || !password) {
    toast("Enter a username and password", "err");
    return;
  }
  const btn = $("loginBtn");
  btn.disabled = true;
  try {
    await api("/login", { method: "POST", body: { username, password } });
    $("loginPass").value = "";
    await enterApp();
  } catch (e) {
    toast(e.message === "unauthorised" ? "Wrong username or password" : e.message, "err");
  } finally {
    btn.disabled = false;
  }
}

// invite
$("inviteBtn").addEventListener("click", () => openDialog("dlgInvite"));

$("inviteSubmit").addEventListener("click", async () => {
  const name = $("invName").value.trim();
  if (!name) { toast("A name is required", "err"); return; }
  const cidr = $("invCidr").value;
  if (!cidr) { toast("Pick a CIDR", "err"); return; }

  const body = { name, cidr_id: Number(cidr), is_admin: $("invAdmin").checked };
  const ip = $("invIp").value.trim();
  if (ip) body.ip = ip;
  const expires = $("invExpires").value.trim();
  if (expires) body.invite_expires_hours = Number(expires);

  const btn = $("inviteSubmit");
  btn.disabled = true;
  try {
    const result = await api("/peers", { method: "POST", body });
    closeDialog("dlgInvite");
    $("invName").value = "";
    $("invIp").value = "";
    $("invExpires").value = "";
    $("invAdmin").checked = false;

    $("resultFor").textContent = result.peer.name + " · " + result.peer.ip;
    $("inviteText").textContent = result.invitation;
    $("dlgResult").dataset.name = result.peer.name;
    openDialog("dlgResult");
    await load();
  } catch (e) {
    toast(e.message, "err");
  } finally {
    btn.disabled = false;
  }
});

$("copyInvite").addEventListener("click", async () => {
  const text = $("inviteText").textContent;
  try {
    await navigator.clipboard.writeText(text);
    toast("Copied to clipboard");
  } catch {
    // Clipboard API needs a secure context; fall back to a selection.
    const range = document.createRange();
    range.selectNodeContents($("inviteText"));
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
    toast("Select-all done — press Cmd/Ctrl+C");
  }
});

$("downloadInvite").addEventListener("click", () => {
  const name = $("dlgResult").dataset.name || "invitation";
  const blob = new Blob([$("inviteText").textContent], { type: "application/toml" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name + ".toml";
  a.click();
  URL.revokeObjectURL(url);
});

// rename peer
$("renamePeerSubmit").addEventListener("click", async () => {
  const id = $("renamePeerSubmit").dataset.id;
  const name = $("rnPeer").value.trim();
  if (!name) { toast("A name is required", "err"); return; }
  try {
    await api("/peers/" + id + "/name", { method: "PUT", body: { name } });
    closeDialog("dlgRenamePeer");
    toast("Renamed to " + name);
    await load();
  } catch (e) {
    toast(e.message, "err");
  }
});

// add cidr
$("addCidrBtn").addEventListener("click", () => openDialog("dlgCidr"));

$("cidrSubmit").addEventListener("click", async () => {
  const name = $("cName").value.trim();
  const range = $("cRange").value.trim();
  if (!name || !range) { toast("Name and range are required", "err"); return; }

  const body = { name, cidr: range };
  const parent = $("cParent").value;
  if (parent) body.parent = Number(parent);

  try {
    await api("/cidrs", { method: "POST", body });
    closeDialog("dlgCidr");
    $("cName").value = "";
    $("cRange").value = "";
    toast("Added " + name);
    await load();
  } catch (e) {
    toast(e.message, "err");
  }
});

// add association
$("addAssocBtn").addEventListener("click", () => openDialog("dlgAssoc"));

$("assocSubmit").addEventListener("click", async () => {
  const a = $("asA").value;
  const b = $("asB").value;
  if (!a || !b) { toast("Pick two CIDRs", "err"); return; }
  if (a === b) { toast("Pick two different CIDRs", "err"); return; }
  try {
    await api("/associations", {
      method: "POST",
      body: { cidr_a: Number(a), cidr_b: Number(b) },
    });
    closeDialog("dlgAssoc");
    toast("Association added");
    await load();
  } catch (e) {
    toast(e.message, "err");
  }
});

/* ------------------------------------------------------------------ boot */

function showLogin() {
  $("appView").classList.add("hidden");
  $("loginView").classList.remove("hidden");
  $("loginPass").value = "";
  $("loginPass").focus();
}

async function enterApp() {
  $("loginView").classList.add("hidden");
  $("appView").classList.remove("hidden");
  setTab(state.tab);
  try {
    await load();
  } catch (e) {
    if (e.unauthorised) {
      showLogin();
    } else {
      toast(e.message, "err");
    }
  }
}

(async () => {
  try {
    await api("/me");
    await enterApp();
  } catch {
    showLogin();
  }
})();
</script>
</body>
</html>
"##;
