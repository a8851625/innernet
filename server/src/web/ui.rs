pub const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>innernet admin</title>
<style>
  :root {
    --bg: #0f1115;
    --panel: #171a21;
    --panel-2: #1e222b;
    --border: #2a2f3a;
    --text: #e6e8ee;
    --muted: #949bab;
    --accent: #4f9cf9;
    --ok: #3fb950;
    --warn: #d29922;
    --err: #f85149;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: var(--bg); color: var(--text);
    font: 14px/1.5 ui-sans-serif, -apple-system, "Segoe UI", sans-serif;
  }
  .wrap { max-width: 1100px; margin: 0 auto; padding: 32px 20px 80px; }
  h1 { font-size: 20px; font-weight: 600; margin: 0 0 4px; }
  .sub { color: var(--muted); font-size: 13px; margin-bottom: 28px; }
  .card {
    background: var(--panel); border: 1px solid var(--border);
    border-radius: 10px; padding: 20px; margin-bottom: 20px;
  }
  .row { display: flex; gap: 10px; align-items: center; }
  input {
    background: var(--panel-2); border: 1px solid var(--border);
    color: var(--text); border-radius: 7px; padding: 9px 11px;
    font: inherit; width: 100%;
  }
  input:focus { outline: none; border-color: var(--accent); }
  button {
    background: var(--accent); color: #fff; border: 0; border-radius: 7px;
    padding: 9px 16px; font: inherit; font-weight: 500; cursor: pointer;
    white-space: nowrap;
  }
  button:hover { filter: brightness(1.1); }
  button:disabled { opacity: .5; cursor: default; }
  button.ghost {
    background: transparent; border: 1px solid var(--border); color: var(--text);
  }
  button.danger { background: var(--err); }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 11px 12px; border-bottom: 1px solid var(--border); vertical-align: top; }
  th { color: var(--muted); font-weight: 500; font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
  tr:last-child td { border-bottom: 0; }
  .mono { font-family: ui-monospace, "SF Mono", Menlo, monospace; font-size: 12.5px; }
  .tag {
    display: inline-block; padding: 2px 8px; border-radius: 20px;
    font-size: 11.5px; font-weight: 500;
  }
  .tag.ok { background: rgba(63,185,80,.15); color: var(--ok); }
  .tag.off { background: rgba(148,155,171,.15); color: var(--muted); }
  .tag.admin { background: rgba(210,153,34,.15); color: var(--warn); }
  .subnets { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .chip {
    background: var(--panel-2); border: 1px solid var(--border);
    border-radius: 6px; padding: 3px 9px; font-size: 12px;
    display: inline-flex; gap: 7px; align-items: center;
  }
  .chip button {
    background: none; border: 0; color: var(--muted); cursor: pointer;
    padding: 0; font-size: 15px; line-height: 1;
  }
  .chip button:hover { color: var(--err); }
  .addrow { display: flex; gap: 7px; }
  .addrow input { font-size: 12.5px; padding: 6px 9px; }
  .addrow button { padding: 6px 12px; font-size: 12.5px; }
  .hint { color: var(--muted); font-size: 12px; margin-top: 7px; }
  #msg {
    position: fixed; right: 20px; bottom: 20px; max-width: 380px;
    padding: 12px 16px; border-radius: 9px; font-size: 13px;
    opacity: 0; transform: translateY(8px); transition: .18s; pointer-events: none;
  }
  #msg.show { opacity: 1; transform: none; }
  #msg.err { background: #3d1518; border: 1px solid var(--err); color: #ffb4ae; }
  #msg.ok { background: #132b18; border: 1px solid var(--ok); color: #a5e8b0; }
  .hide { display: none !important; }

  .tabs { display: flex; gap: 4px; margin: 14px 0 12px; border-bottom: 1px solid #242a36; }
  .tab {
    background: none; border: none; color: #8b94a7; padding: 8px 14px;
    font-size: 13px; cursor: pointer; border-bottom: 2px solid transparent;
    font-family: inherit;
  }
  .tab:hover { color: #d6dbe6; }
  .tab.on { color: #d6dbe6; border-bottom-color: #4f7cff; }

  /* Modal */
  .modal {
    position: fixed; inset: 0; background: rgba(4,6,10,.72);
    display: flex; align-items: center; justify-content: center; z-index: 50;
    padding: 20px;
  }
  .modal.hide { display: none !important; }
  .modal .card {
    width: 100%; max-width: 520px; max-height: 88vh; overflow: auto;
    background: #14171d; border: 1px solid #242a36; border-radius: 10px;
    padding: 20px;
  }
  .modal h2 { margin: 0 0 4px; font-size: 15px; font-weight: 600; }
  .modal .sub { margin-bottom: 16px; }
  .field { margin-bottom: 14px; }
  .field label { display: block; font-size: 12px; color: #8b94a7; margin-bottom: 5px; }
  .field input[type=text], .field input[type=number], .field select {
    width: 100%; box-sizing: border-box;
  }
  .check { display: flex; align-items: center; gap: 8px; font-size: 13px; color: #b6bdcc; }
  .check input { width: auto; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }

  /* Invitation output */
  .invite-out {
    background: #0b0d11; border: 1px solid #242a36; border-radius: 8px;
    padding: 12px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px; line-height: 1.5; color: #b6bdcc;
    white-space: pre; overflow: auto; max-height: 320px; margin-bottom: 12px;
  }
  .warn {
    background: rgba(255,176,32,.08); border: 1px solid rgba(255,176,32,.3);
    color: #ffb020; border-radius: 8px; padding: 10px 12px; font-size: 12px;
    margin-bottom: 14px; line-height: 1.5;
  }
  .rowbtns { display: flex; gap: 6px; }
</style>
</head>
<body>
<div class="wrap">

  <div id="login">
    <h1>innernet admin</h1>
    <div class="sub">Sign in to manage peers and subnet routes.</div>
    <div class="card" style="max-width:380px">
      <div style="margin-bottom:12px">
        <input id="u" placeholder="username" autocomplete="username" value="admin">
      </div>
      <div style="margin-bottom:16px">
        <input id="p" type="password" placeholder="password" autocomplete="current-password">
      </div>
      <button id="loginBtn" style="width:100%">Sign in</button>
    </div>
  </div>

  <div id="app" class="hide">
    <div class="row" style="justify-content:space-between;margin-bottom:6px">
      <h1 style="margin:0">innernet admin</h1>
      <button class="ghost" id="logoutBtn">Sign out</button>
    </div>

    <div class="tabs">
      <button class="tab on" data-tab="peers">Peers</button>
      <button class="tab" data-tab="cidrs">CIDRs</button>
      <button class="tab" data-tab="assoc">Associations</button>
    </div>

    <!-- Peers -->
    <section id="tab-peers">
      <div class="row" style="justify-content:space-between;margin-bottom:8px">
        <div class="sub" style="margin:0">Subnet routes are advertised as WireGuard allowed-ips and applied immediately.</div>
        <button id="inviteBtn">Invite peer</button>
      </div>
      <div class="card" style="padding:0">
        <table>
          <thead>
            <tr>
              <th style="width:180px">Peer</th>
              <th style="width:130px">Address</th>
              <th>Subnet routes</th>
              <th style="width:230px">State</th>
            </tr>
          </thead>
          <tbody id="peers"></tbody>
        </table>
      </div>
    </section>

    <!-- CIDRs -->
    <section id="tab-cidrs" class="hide">
      <div class="row" style="justify-content:space-between;margin-bottom:8px">
        <div class="sub" style="margin:0">CIDRs carve the network into groups. Only CIDRs with no children can hold peers.</div>
        <button id="addCidrBtn">Add CIDR</button>
      </div>
      <div class="card" style="padding:0">
        <table>
          <thead>
            <tr><th style="width:200px">Name</th><th>Range</th><th style="width:160px"></th></tr>
          </thead>
          <tbody id="cidrs"></tbody>
        </table>
      </div>
    </section>

    <!-- Associations -->
    <section id="tab-assoc" class="hide">
      <div class="row" style="justify-content:space-between;margin-bottom:8px">
        <div class="sub" style="margin:0">By default peers can only reach peers in their own CIDR. An association opens a pair of CIDRs to each other.</div>
        <button id="addAssocBtn">Add association</button>
      </div>
      <div class="card" style="padding:0">
        <table>
          <thead><tr><th>CIDR A</th><th>CIDR B</th><th style="width:120px"></th></tr></thead>
          <tbody id="assoc"></tbody>
        </table>
      </div>
    </section>
  </div>
</div>

<div id="msg"></div>

<!-- Invite dialog -->
<div id="inviteModal" class="modal hide">
  <div class="card">
    <h2>Invite a peer</h2>
    <div class="sub">Creates the peer and shows its invitation once. The private key is not stored on the server.</div>
    <div class="field">
      <label for="invName">Name</label>
      <input id="invName" type="text" placeholder="laptop" autocomplete="off">
    </div>
    <div class="field">
      <label for="invCidr">CIDR</label>
      <select id="invCidr"></select>
    </div>
    <div class="field">
      <label for="invIp">Address (blank = next free)</label>
      <input id="invIp" type="text" placeholder="auto" autocomplete="off">
    </div>
    <div class="field">
      <label for="invExpires">Invitation expires in (hours, blank = never)</label>
      <input id="invExpires" type="number" min="1" placeholder="168" autocomplete="off">
    </div>
    <div class="field">
      <label class="check"><input id="invAdmin" type="checkbox"> Make this peer an admin</label>
    </div>
    <div class="actions">
      <button class="ghost" data-close="inviteModal">Cancel</button>
      <button id="inviteSubmit">Create invitation</button>
    </div>
  </div>
</div>

<!-- Invitation result -->
<div id="resultModal" class="modal hide">
  <div class="card">
    <h2>Invitation ready</h2>
    <div class="sub" id="resultFor"></div>
    <div class="warn">
      This is the only time the private key is shown. Save it now — the server
      keeps only the public half, so this cannot be recovered.
    </div>
    <div class="invite-out" id="inviteText"></div>
    <div class="actions">
      <button class="ghost" id="copyInvite">Copy</button>
      <button class="ghost" id="downloadInvite">Download</button>
      <button data-close="resultModal">Done</button>
    </div>
  </div>
</div>

<!-- Rename peer -->
<div id="renamePeerModal" class="modal hide">
  <div class="card">
    <h2>Rename peer</h2>
    <div class="field">
      <label for="rnName">New name</label>
      <input id="rnName" type="text" autocomplete="off">
    </div>
    <div class="actions">
      <button class="ghost" data-close="renamePeerModal">Cancel</button>
      <button id="renamePeerSubmit">Rename</button>
    </div>
  </div>
</div>

<!-- Add CIDR -->
<div id="cidrModal" class="modal hide">
  <div class="card">
    <h2>Add CIDR</h2>
    <div class="field">
      <label for="cName">Name</label>
      <input id="cName" type="text" placeholder="servers" autocomplete="off">
    </div>
    <div class="field">
      <label for="cRange">Range</label>
      <input id="cRange" type="text" placeholder="10.60.3.0/24" autocomplete="off">
    </div>
    <div class="field">
      <label for="cParent">Parent (blank = top level)</label>
      <select id="cParent"></select>
    </div>
    <div class="actions">
      <button class="ghost" data-close="cidrModal">Cancel</button>
      <button id="cidrSubmit">Add</button>
    </div>
  </div>
</div>

<!-- Add association -->
<div id="assocModal" class="modal hide">
  <div class="card">
    <h2>Add association</h2>
    <div class="sub">Peers in these two CIDRs will be able to reach each other.</div>
    <div class="field">
      <label for="asA">CIDR A</label>
      <select id="asA"></select>
    </div>
    <div class="field">
      <label for="asB">CIDR B</label>
      <select id="asB"></select>
    </div>
    <div class="actions">
      <button class="ghost" data-close="assocModal">Cancel</button>
      <button id="assocSubmit">Add</button>
    </div>
  </div>
</div>

<script>
const $ = (s) => document.querySelector(s);
let state = { peers: [], cidrs: [], assoc: [], tab: 'peers' };

function toast(text, kind = 'ok') {
  const el = $('#msg');
  el.textContent = text;
  el.className = 'show ' + kind;
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.className = ''; }, 3200);
}

async function api(path, opts = {}) {
  const res = await fetch('/web' + path, {
    credentials: 'same-origin',
    headers: opts.body ? { 'Content-Type': 'application/json' } : {},
    ...opts,
  });
  if (res.status === 401) { showLogin(); throw new Error('unauthorised'); }
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = text; }
  if (!res.ok) throw new Error((data && data.error) || 'request failed');
  return data;
}

function showLogin() {
  $('#login').classList.remove('hide');
  $('#app').classList.add('hide');
}

function showApp() {
  $('#login').classList.add('hide');
  $('#app').classList.remove('hide');
}

$('#loginBtn').onclick = async () => {
  const btn = $('#loginBtn');
  btn.disabled = true;
  try {
    await api('/login', {
      method: 'POST',
      body: JSON.stringify({ username: $('#u').value, password: $('#p').value }),
    });
    $('#p').value = '';
    showApp();
    await load();
  } catch (e) {
    toast(e.message === 'unauthorised' ? 'Invalid credentials' : e.message, 'err');
  } finally {
    btn.disabled = false;
  }
};

$('#p').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('#loginBtn').click(); });

$('#logoutBtn').onclick = async () => {
  try { await api('/logout', { method: 'POST' }); } catch {}
  showLogin();
};

function openModal(id) { document.getElementById(id).classList.remove('hide'); }
function closeModal(id) { document.getElementById(id).classList.add('hide'); }

document.querySelectorAll('[data-close]').forEach((b) => {
  b.onclick = () => closeModal(b.dataset.close);
});
// Click on the backdrop closes, click inside the card does not.
document.querySelectorAll('.modal').forEach((m) => {
  m.addEventListener('click', (e) => { if (e.target === m) m.classList.add('hide'); });
});

function cidrName(id) {
  const c = state.cidrs.find((x) => x.id === id);
  return c ? c.name : ('#' + id);
}

function renderTabs() {
  document.querySelectorAll('.tab').forEach((b) => {
    b.classList.toggle('on', b.dataset.tab === state.tab);
  });
  for (const t of ['peers', 'cidrs', 'assoc']) {
    document.getElementById('tab-' + t).classList.toggle('hide', state.tab !== t);
  }
}

function renderCidrs() {
  const tbody = $('#cidrs');
  tbody.innerHTML = '';
  for (const c of state.cidrs) {
    const tr = document.createElement('tr');

    const name = document.createElement('td');
    name.textContent = c.name;
    name.style.fontWeight = '500';
    tr.appendChild(name);

    const range = document.createElement('td');
    range.className = 'mono';
    range.textContent = c.cidr + (c.parent ? ('  (child of ' + cidrName(c.parent) + ')') : '');
    tr.appendChild(range);

    const ops = document.createElement('td');
    const btns = document.createElement('div');
    btns.className = 'rowbtns';

    const ren = document.createElement('button');
    ren.className = 'ghost';
    ren.textContent = 'Rename';
    ren.onclick = () => {
      const next = prompt('New name for ' + c.name, c.name);
      if (!next || next === c.name) return;
      api('/cidrs/' + c.id + '/name', { method: 'PUT', body: JSON.stringify({ name: next }) })
        .then(() => { toast('CIDR renamed'); return loadAll(); })
        .catch((e) => toast(e.message, 'err'));
    };
    btns.appendChild(ren);

    const del = document.createElement('button');
    del.className = 'ghost';
    del.textContent = 'Delete';
    del.onclick = () => {
      if (!confirm('Delete CIDR ' + c.name + '? This fails if it still holds peers.')) return;
      api('/cidrs/' + c.id, { method: 'DELETE' })
        .then(() => { toast('CIDR deleted'); return loadAll(); })
        .catch((e) => toast(e.message, 'err'));
    };
    btns.appendChild(del);

    ops.appendChild(btns);
    tr.appendChild(ops);
    tbody.appendChild(tr);
  }
}

function renderAssoc() {
  const tbody = $('#assoc');
  tbody.innerHTML = '';
  if (state.assoc.length === 0) {
    const tr = document.createElement('tr');
    const td = document.createElement('td');
    td.colSpan = 3;
    td.className = 'hint';
    td.textContent = 'No associations. Peers are restricted to their own CIDR.';
    tr.appendChild(td);
    tbody.appendChild(tr);
    return;
  }
  for (const a of state.assoc) {
    const tr = document.createElement('tr');
    const l = document.createElement('td');
    l.textContent = cidrName(a.cidr_a);
    tr.appendChild(l);
    const r = document.createElement('td');
    r.textContent = cidrName(a.cidr_b);
    tr.appendChild(r);
    const ops = document.createElement('td');
    const del = document.createElement('button');
    del.className = 'ghost';
    del.textContent = 'Remove';
    del.onclick = () => {
      api('/associations/' + a.id, { method: 'DELETE' })
        .then(() => { toast('Association removed'); return loadAll(); })
        .catch((e) => toast(e.message, 'err'));
    };
    ops.appendChild(del);
    tr.appendChild(ops);
    tbody.appendChild(tr);
  }
}

function fillCidrSelects() {
  const leaves = state.cidrs;
  for (const id of ['invCidr', 'cParent']) {
    const sel = document.getElementById(id);
    const keep = sel.value;
    sel.innerHTML = '';
    if (id === 'cParent') {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '(top level)';
      sel.appendChild(opt);
    }
    for (const c of leaves) {
      const opt = document.createElement('option');
      opt.value = c.id;
      opt.textContent = c.name + ' \u2014 ' + c.cidr;
      sel.appendChild(opt);
    }
    if (keep) sel.value = keep;
  }
  for (const id of ['asA', 'asB']) {
    const sel = document.getElementById(id);
    const keep = sel.value;
    sel.innerHTML = '';
    for (const c of leaves) {
      const opt = document.createElement('option');
      opt.value = c.id;
      opt.textContent = c.name;
      sel.appendChild(opt);
    }
    if (keep) sel.value = keep;
  }
}

function render() {
  const tbody = $('#peers');
  tbody.innerHTML = '';

  for (const p of state.peers) {
    const tr = document.createElement('tr');

    const name = document.createElement('td');
    name.innerHTML = '<div style="font-weight:500">' + esc(p.name) + '</div>'
      + (p.is_admin ? '<span class="tag admin">admin</span>' : '');
    tr.appendChild(name);

    const addr = document.createElement('td');
    addr.className = 'mono';
    addr.textContent = p.ip;
    tr.appendChild(addr);

    const subnets = document.createElement('td');
    const chips = document.createElement('div');
    chips.className = 'subnets';
    if (p.allowed_subnets.length === 0) {
      const none = document.createElement('span');
      none.className = 'hint';
      none.style.margin = '0';
      none.textContent = 'no subnet routes';
      chips.appendChild(none);
    }
    for (const s of p.allowed_subnets) {
      const chip = document.createElement('span');
      chip.className = 'chip mono';
      chip.textContent = s;
      const x = document.createElement('button');
      x.innerHTML = '&times;';
      x.title = 'remove';
      x.onclick = () => setSubnets(p, p.allowed_subnets.filter((v) => v !== s));
      chip.appendChild(x);
      chips.appendChild(chip);
    }
    subnets.appendChild(chips);

    const add = document.createElement('div');
    add.className = 'addrow';
    const inp = document.createElement('input');
    inp.placeholder = '192.168.1.0/24';
    inp.className = 'mono';
    const btn = document.createElement('button');
    btn.textContent = 'Add';
    btn.className = 'ghost';
    btn.onclick = () => {
      const v = inp.value.trim();
      if (!v) return;
      if (!/^[0-9a-fA-F:.]+\/\d{1,3}$/.test(v)) { toast('Not a CIDR: ' + v, 'err'); return; }
      if (p.allowed_subnets.includes(v)) { toast('Already present', 'err'); return; }
      setSubnets(p, [...p.allowed_subnets, v]);
    };
    inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') btn.click(); });
    add.appendChild(inp);
    add.appendChild(btn);
    subnets.appendChild(add);
    tr.appendChild(subnets);

    const st = document.createElement('td');
    const badge = document.createElement('span');
    badge.className = 'tag ' + (p.is_disabled ? 'off' : 'ok');
    badge.textContent = p.is_disabled ? 'disabled' : 'active';
    st.appendChild(badge);
    const tog = document.createElement('button');
    tog.className = 'ghost';
    tog.style.marginLeft = '8px';
    tog.textContent = p.is_disabled ? 'Enable' : 'Disable';
    tog.onclick = () => setDisabled(p, !p.is_disabled);
    st.appendChild(tog);

    const ren = document.createElement('button');
    ren.className = 'ghost';
    ren.style.marginLeft = '6px';
    ren.textContent = 'Rename';
    ren.onclick = () => {
      document.getElementById('rnName').value = p.name;
      document.getElementById('renamePeerSubmit').dataset.id = p.id;
      openModal('renamePeerModal');
      document.getElementById('rnName').focus();
    };
    st.appendChild(ren);

    const del = document.createElement('button');
    del.className = 'ghost';
    del.style.marginLeft = '6px';
    del.textContent = 'Delete';
    del.onclick = () => {
      if (!confirm('Delete peer ' + p.name + '? It will be disabled and its routes revoked.')) return;
      api('/peers/' + p.id, { method: 'DELETE' })
        .then(() => { toast('Peer deleted'); return loadAll(); })
        .catch((e) => toast(e.message, 'err'));
    };
    st.appendChild(del);

    tr.appendChild(st);

    tbody.appendChild(tr);
  }
}

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

async function setSubnets(peer, subnets) {
  try {
    const updated = await api('/peers/' + peer.id + '/subnets', {
      method: 'PUT',
      body: JSON.stringify({ allowed_subnets: subnets }),
    });
    Object.assign(peer, updated);
    render();
    toast('Routes applied to WireGuard');
  } catch (e) {
    toast(e.message, 'err');
  }
}

async function setDisabled(peer, disabled) {
  try {
    const updated = await api('/peers/' + peer.id + '/disabled', {
      method: 'PUT',
      body: JSON.stringify({ is_disabled: disabled }),
    });
    Object.assign(peer, updated);
    render();
    toast(disabled ? 'Peer disabled, routes revoked' : 'Peer enabled');
  } catch (e) {
    toast(e.message, 'err');
  }
}

async function loadAll() {
  try {
    const [peers, cidrs, assoc] = await Promise.all([
      api('/peers'), api('/cidrs'), api('/associations'),
    ]);
    state.peers = peers;
    state.cidrs = cidrs;
    state.assoc = assoc;
    render();
    renderCidrs();
    renderAssoc();
    fillCidrSelects();
  } catch (e) {
    if (e.message !== 'unauthorised') toast(e.message, 'err');
  }
}

// ---- tab switching -------------------------------------------------------
document.querySelectorAll('.tab').forEach((b) => {
  b.onclick = () => { state.tab = b.dataset.tab; renderTabs(); };
});

// ---- invite --------------------------------------------------------------
$('#inviteBtn').onclick = () => { openModal('inviteModal'); $('#invName').focus(); };

$('#inviteSubmit').onclick = async () => {
  const name = $('#invName').value.trim();
  if (!name) { toast('Name is required', 'err'); return; }
  const cidr = $('#invCidr').value;
  if (!cidr) { toast('Pick a CIDR first', 'err'); return; }
  const body = {
    name,
    cidr_id: Number(cidr),
    is_admin: $('#invAdmin').checked,
  };
  const ip = $('#invIp').value.trim();
  if (ip) body.ip = ip;
  const exp = $('#invExpires').value.trim();
  if (exp) body.invite_expires_hours = Number(exp);

  try {
    const res = await api('/peers', { method: 'POST', body: JSON.stringify(body) });
    closeModal('inviteModal');
    $('#invName').value = '';
    $('#invIp').value = '';
    $('#invExpires').value = '';
    $('#invAdmin').checked = false;

    $('#resultFor').textContent = res.peer.name + ' \u00b7 ' + res.peer.ip;
    $('#inviteText').textContent = res.invitation;
    $('#resultModal').dataset.name = res.peer.name;
    openModal('resultModal');
    await loadAll();
  } catch (e) {
    toast(e.message, 'err');
  }
};

$('#copyInvite').onclick = async () => {
  try {
    await navigator.clipboard.writeText($('#inviteText').textContent);
    toast('Copied to clipboard');
  } catch {
    toast('Copy failed \u2014 select the text manually', 'err');
  }
};

$('#downloadInvite').onclick = () => {
  const name = $('#resultModal').dataset.name || 'invitation';
  const blob = new Blob([$('#inviteText').textContent], { type: 'text/plain' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name + '.toml';
  a.click();
  URL.revokeObjectURL(a.href);
};

// ---- rename peer ---------------------------------------------------------
$('#renamePeerSubmit').onclick = async () => {
  const id = $('#renamePeerSubmit').dataset.id;
  const name = $('#rnName').value.trim();
  if (!name) { toast('Name is required', 'err'); return; }
  try {
    await api('/peers/' + id + '/name', { method: 'PUT', body: JSON.stringify({ name }) });
    closeModal('renamePeerModal');
    toast('Peer renamed');
    await loadAll();
  } catch (e) {
    toast(e.message, 'err');
  }
};

// ---- add CIDR ------------------------------------------------------------
$('#addCidrBtn').onclick = () => { openModal('cidrModal'); $('#cName').focus(); };

$('#cidrSubmit').onclick = async () => {
  const name = $('#cName').value.trim();
  const range = $('#cRange').value.trim();
  if (!name || !range) { toast('Name and range are required', 'err'); return; }
  const body = { name, cidr: range };
  const parent = $('#cParent').value;
  if (parent) body.parent = Number(parent);
  try {
    await api('/cidrs', { method: 'POST', body: JSON.stringify(body) });
    closeModal('cidrModal');
    $('#cName').value = '';
    $('#cRange').value = '';
    toast('CIDR added');
    await loadAll();
  } catch (e) {
    toast(e.message, 'err');
  }
};

// ---- add association -----------------------------------------------------
$('#addAssocBtn').onclick = () => { openModal('assocModal'); };

$('#assocSubmit').onclick = async () => {
  const a = $('#asA').value;
  const b = $('#asB').value;
  if (!a || !b) { toast('Pick two CIDRs', 'err'); return; }
  if (a === b) { toast('Pick two different CIDRs', 'err'); return; }
  try {
    await api('/associations', {
      method: 'POST',
      body: JSON.stringify({ cidr_a: Number(a), cidr_b: Number(b) }),
    });
    closeModal('assocModal');
    toast('Association added');
    await loadAll();
  } catch (e) {
    toast(e.message, 'err');
  }
};

(async () => {
  try {
    await api('/me');
    showApp();
    renderTabs();
    await loadAll();
  } catch {
    showLogin();
  }
})();
</script>
</body>
</html>
"#;
