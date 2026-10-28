// usage: node cdp-eval.mjs <url-substring> <js>    (needs Node 22+ for the global WebSocket; port from DESK_CDP_PORT, default 9222)
const [sub, expr] = process.argv.slice(2);
const port = process.env.DESK_CDP_PORT || 9222;
setTimeout(() => { console.error('timeout'); process.exit(1); }, 15000);
const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const t = tabs.find(x => x.type === 'page' && x.url.includes(sub));
if (!t) { console.log('NO TAB', tabs.map(x => x.url)); process.exit(1); }
const ws = new WebSocket(t.webSocketDebuggerUrl);
ws.onerror = () => { console.error('websocket error'); process.exit(1); };
ws.onopen = () => ws.send(JSON.stringify({id:1, method:'Runtime.evaluate', params:{expression:expr, returnByValue:true, awaitPromise:true}}));
ws.onmessage = e => {
  const r = JSON.parse(e.data);
  if (r.id !== 1) return;
  if (r.error) { console.error(JSON.stringify(r.error)); process.exit(1); }
  console.log(r.result?.result?.value ?? JSON.stringify(r.result));
  process.exit(0);
};
