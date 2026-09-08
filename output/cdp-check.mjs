// 通过 CDP 检查真实 WebView2 中「添加/编辑启动项」弹窗的渲染情况
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = () => rej(new Error('ws connect failed: ' + wsUrl));
  });
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  return {
    send(method, params = {}) {
      const msgId = ++id;
      ws.send(JSON.stringify({ id: msgId, method, params }));
      return new Promise((res) => pending.set(msgId, res));
    },
    close: () => ws.close(),
  };
}

const evalIn = async (conn, expression) => {
  const r = await conn.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) {
    return { __error: r.exceptionDetails.text + ' :: ' + (r.exceptionDetails.exception?.description || '') };
  }
  return r.result?.value;
};

const version = await (await fetch('http://127.0.0.1:9222/json/version')).json();
console.log('WebView2:', version.Browser);

const targets = (await (await fetch('http://127.0.0.1:9222/json')).json()).filter((t) => t.type === 'page');
let settingsConn = null;
const info = [];
for (const t of targets) {
  const conn = await connect(t.webSocketDebuggerUrl);
  const r = await evalIn(
    conn,
    `JSON.stringify({ settings: !!document.querySelector('.settings-root'), launcher: !!document.querySelector('.launcher'), url: location.href })`,
  );
  let parsed = {};
  try { parsed = JSON.parse(r || '{}'); } catch { parsed = { raw: String(r) }; }
  info.push({ id: t.id.slice(0, 12), ...parsed });
  if (parsed.settings) settingsConn = { conn, id: t.id };
  else conn.close();
}
console.log('targets:', JSON.stringify(info));
if (!settingsConn) {
  console.log('NO SETTINGS TARGET FOUND');
  process.exit(1);
}

const conn = settingsConn.conn;
await evalIn(conn, `(() => {
  window.__errs = [];
  window.addEventListener('error', e => window.__errs.push('error: ' + ((e.error && e.error.stack) || e.message)));
  window.addEventListener('unhandledrejection', e => window.__errs.push('rejection: ' + ((e.reason && e.reason.stack) || String(e.reason))));
  const oe = console.error.bind(console);
  console.error = (...a) => { window.__errs.push('console.error: ' + a.map(x => String((x && x.stack) || x)).join(' | ').slice(0, 800)); oe(...a); };
  return 'hooks ok';
})()`);

const shown = await evalIn(conn, `window.__TAURI_INTERNALS__.invoke('show_settings_window').then(() => 'shown').catch(e => 'show err: ' + String(e))`);
console.log('settings window:', shown);
await sleep(600);

const clicked = await evalIn(
  conn,
  `([...document.querySelectorAll('.primary-btn')].find(b => b.textContent.includes('添加')) || {}).click ? ([...document.querySelectorAll('.primary-btn')].find(b => b.textContent.includes('添加')).click(), 'clicked') : 'add button not found'`,
);
console.log('add click:', clicked);
await sleep(900);

const dump = await evalIn(
  conn,
  `JSON.stringify({
    labels: [...document.querySelectorAll('.item-modal .n-form-item-label')].map(l => l.textContent.trim()),
    formItems: document.querySelectorAll('.item-modal .n-form-item').length,
    pathBrowse: !!document.querySelector('.item-modal .path-browse'),
    suffixHtml: (document.querySelector('.item-modal .n-input .n-input__suffix') || {}).innerHTML || null,
    errs: (window.__errs || []).slice(0, 6),
  })`,
);
console.log('dump:', dump);
conn.close();
