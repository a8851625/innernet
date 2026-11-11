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
      <h1 style="margin:0">Peers</h1>
      <button class="ghost" id="logoutBtn">Sign out</button>
    </div>
    <div class="sub">Subnet routes are advertised as WireGuard allowed-ips and applied immediately.</div>

    <div class="card" style="padding:0">
      <table>
        <thead>
          <tr>
            <th style="width:180px">Peer</th>
            <th style="width:130px">Address</th>
            <th>Subnet routes</th>
            <th style="width:150px">State</th>
          </tr>
        </thead>
        <tbody id="peers"></tbody>
      </table>
    </div>
  </div>
</div>

<div id="msg"></div>

<script>
const $ = (s) => document.querySelector(s);
let state = { peers: [] };

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

async function load() {
  try {
    state.peers = await api('/peers');
    render();
  } catch (e) {
    if (e.message !== 'unauthorised') toast(e.message, 'err');
  }
}

(async () => {
  try {
    await api('/me');
    showApp();
    await load();
  } catch {
    showLogin();
  }
})();
</script>
</body>
</html>
"#;
