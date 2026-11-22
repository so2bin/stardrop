(function () {
  'use strict';

  var TAURI = window.__TAURI__;
  var isApp = !!TAURI;
  var $ = function (sel) { return document.querySelector(sel); };
  var invoke = isApp ? TAURI.core.invoke : mockInvoke;

  var S = {
    pin: '----', port: 0, urls: [], device: '', saveDir: '',
    inbox: [], outbox: [], lan: null, version: '1.0.0'
  };
  var previewItem = null;
  var activeTab = 'inbox';
  var netIndex = 0;
  var peerTimer = null;
  var manualOpen = false;

  function currentUrl() {
    if (!S.urls || !S.urls.length) return '';
    return S.urls[netIndex % S.urls.length];
  }

  /* ---------------- helpers ---------------- */
  function esc(value) {
    return String(value == null ? '' : value).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }
  function fmtSize(n) {
    n = Number(n) || 0;
    if (n < 1024) return n + ' B';
    if (n < 1048576) return (n / 1024).toFixed(1) + ' KB';
    if (n < 1073741824) return (n / 1048576).toFixed(1) + ' MB';
    return (n / 1073741824).toFixed(2) + ' GB';
  }
  function thumbUrl(item) {
    if (item.demo) return item.demo;
    return 'http://127.0.0.1:' + S.port + '/thumb?id=' + encodeURIComponent(item.id) + '&k=' + encodeURIComponent(S.pin);
  }
  function fileUrl(item) {
    if (item.demo) return item.demo;
    return 'http://127.0.0.1:' + S.port + '/file?id=' + encodeURIComponent(item.id) + '&k=' + encodeURIComponent(S.pin);
  }
  function icon(name, cls) {
    return '<svg' + (cls ? ' class="' + cls + '"' : '') + '><use href="#i-' + name + '"/></svg>';
  }

  /* ---------------- rendering ---------------- */
  function renderAll() {
    renderInbox();
    renderOutbox();
    renderConnect();
  }

  function tileHTML(item) {
    var ext = (item.ext || '').toUpperCase();
    var actions;
    if (item.kind === 'text') {
      actions = '<div class="tile-actions">' +
        '<button class="ta" data-act="copy" title="复制文字">' + icon('copy') + '</button>' +
        '<button class="ta danger" data-act="remove" title="从列表移除">' + icon('x') + '</button>' +
        '</div>';
      return '<article class="tile text-tile" data-id="' + esc(item.id) + '">' +
        '<div class="thumb"><div class="text-inner">' +
        '<div class="quote"><span class="qmark">“</span></div>' +
        '<div class="text-body">' + esc(item.text || '') + '</div>' +
        '</div>' + actions + '</div>' +
        '<div class="meta"><span class="name">文字消息</span><span class="sub">' + esc(item.at) + ' · 来自手机</span></div>' +
        '</article>';
    }

    actions = '<div class="tile-actions">' +
      '<button class="ta" data-act="open" title="打开">' + icon('external') + '</button>' +
      '<button class="ta" data-act="reveal" title="在文件夹中显示">' + icon('folder') + '</button>' +
      '<button class="ta danger" data-act="remove" title="从列表移除（不删除文件）">' + icon('x') + '</button>' +
      '</div>';

    var media = '';
    if (item.kind === 'image') {
      media = '<div class="ph file"><span>' + esc(ext || 'IMG') + '</span></div>' +
        '<img src="' + thumbUrl(item) + '" alt="" loading="lazy">';
    } else if (item.kind === 'video') {
      media = '<div class="ph video">' + icon('play') + '<span>' + esc(ext || 'VIDEO') + '</span></div>';
    } else {
      media = '<div class="ph file">' + icon('file') + '<span>' + esc(ext || 'FILE') + '</span></div>';
    }

    var sub = fmtSize(item.size) + ' · ' + (item.at || '');
    return '<article class="tile" data-id="' + esc(item.id) + '">' +
      '<div class="thumb">' + media + actions + '</div>' +
      '<div class="meta"><span class="name" title="' + esc(item.name) + '">' + esc(item.name) + '</span>' +
      '<span class="sub">' + esc(sub) + '</span></div>' +
      '</article>';
  }

  function renderInbox() {
    var items = S.inbox || [];
    $('#inboxCount').textContent = items.length;
    var grid = $('#grid');
    grid.classList.toggle('hidden', items.length === 0);
    grid.innerHTML = items.map(tileHTML).join('');
    Array.prototype.forEach.call(grid.querySelectorAll('img'), function (img) {
      img.addEventListener('error', function () {
        var thumb = img.closest('.thumb');
        if (thumb) thumb.classList.add('noimg');
      }, { once: true });
    });
    $('#emptyInbox').classList.toggle('hidden', items.length > 0);
    updateStatline();
  }

  function updateStatline() {
    var items = activeTab === 'send' ? (S.outbox || []) : (S.inbox || []);
    var total = items.reduce(function (a, b) { return a + (Number(b.size) || 0); }, 0);
    if (!items.length) {
      $('#statLine').innerHTML = '';
    } else if (activeTab === 'send') {
      $('#statLine').innerHTML = '<b>' + items.length + '</b> 个文件待手机下载 · <b>' + fmtSize(total) + '</b>';
    } else {
      $('#statLine').innerHTML = '今日已接收 <b>' + items.length + '</b> 个文件 · <b>' + fmtSize(total) + '</b>';
    }
  }

  function sendRowHTML(item) {
    var ext = (item.ext || '').toUpperCase();
    var thumb = item.kind === 'image'
      ? '<img src="' + thumbUrl(item) + '" alt="">'
      : icon(item.kind === 'video' ? 'play' : 'file');
    return '<div class="send-row" data-id="' + esc(item.id) + '">' +
      '<div class="send-thumb">' + thumb + '</div>' +
      '<div class="send-info"><span class="name">' + esc(item.name) + '</span>' +
      '<span class="sub">' + fmtSize(item.size) + (ext ? ' · ' + esc(ext) : '') + '</span></div>' +
      '<span class="send-tag">手机可下载</span>' +
      '<div class="send-actions">' +
      '<button class="ta" data-act="reveal" title="在文件夹中显示">' + icon('folder') + '</button>' +
      '<button class="ta danger" data-act="remove" title="移出待发送列表">' + icon('x') + '</button>' +
      '</div></div>';
  }

  function renderOutbox() {
    var items = S.outbox || [];
    $('#outboxCount').textContent = items.length;
    $('#sendList').innerHTML = items.map(sendRowHTML).join('');
    $('#emptySend').classList.toggle('hidden', items.length > 0);
    updateStatline();
  }

  function renderConnect() {    var url = currentUrl();
    var many = (S.urls || []).length > 1;
    $('#netCount').classList.toggle('hidden', !many);
    $('#cycleNet').classList.toggle('hidden', !many);
    if (many) $('#netCount').textContent = S.urls.length + ' 个地址';
    var display = url ? url.replace(/\?.*$/, '').replace(/\/$/, '') : '未检测到局域网地址';
    $('#urlText').textContent = display;
    $('#pinText').textContent = S.pin || '----';
    $('#statusPort').textContent = S.port ? ':' + S.port : '';
    $('#statusText').textContent = S.port ? (url ? '服务运行中' : '未连接局域网') : '服务启动失败';
    $('#statusPill').classList.toggle('warn', !url);
    $('#saveDirText').textContent = S.saveDir || '图片\\星传';
    setConnected(!!S.connected, true);

    var canvas = $('#qrCanvas');
    if (url && window.StarQR) {
      try {
        window.StarQR.render(canvas, url, { size: 190 });
        canvas.classList.remove('hidden');
        $('#qrEmpty').classList.add('hidden');
      } catch (err) {
        canvas.classList.add('hidden');
        $('#qrEmpty').classList.remove('hidden');
      }
    } else {
      canvas.classList.add('hidden');
      $('#qrEmpty').classList.remove('hidden');
    }
  }

  /* ---------------- connection panel ---------------- */
  function setConnected(on, collapse) {
    var wasConnected = !!S.connected;
    S.connected = on;
    var card = $('#connectCard');
    var pill = $('#livePill');
    if (!card || !pill) return;
    pill.classList.toggle('idle', !on);
    pill.innerHTML = '<i></i>' + (on ? '已连接' : '等待扫码');
    if (!on) {
      manualOpen = false;
      clearTimeout(peerTimer);
      card.classList.remove('collapsed');
      return;
    }
    if (!wasConnected) manualOpen = false;
    if (!S.lastPeer) S.lastPeer = Date.now();
    if (collapse === false) return;
    if (!manualOpen && !card.classList.contains('collapsed')) {
      clearTimeout(peerTimer);
      peerTimer = setTimeout(function () {
        if (S.connected && !manualOpen) card.classList.add('collapsed');
      }, 2600);
    }
  }

  function markPeer() {
    S.lastPeer = Date.now();
    if (!S.connected) setConnected(true);
  }

  /* ---------------- toasts ---------------- */
  function toast(message, type) {
    var host = $('#toasts');
    var el = document.createElement('div');
    el.className = 'toast' + (type ? ' ' + type : '');
    var ico = type === 'err' ? 'x' : type === 'warn' ? 'shield' : 'check';
    el.innerHTML = '<span class="ticon">' + icon(ico) + '</span><span>' + esc(message) + '</span>';
    host.appendChild(el);
    setTimeout(function () {
      el.classList.add('out');
      setTimeout(function () { el.remove(); }, 260);
    }, type === 'err' ? 4600 : 3000);
  }

  async function copyText(text, okMessage) {
    try {
      if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(text);
      } else {
        await invoke('copy_text', { text: text });
      }
      toast(okMessage || '已复制');
    } catch (err) {
      try {
        await invoke('copy_text', { text: text });
        toast(okMessage || '已复制');
      } catch (err2) {
        toast('复制失败', 'err');
      }
    }
  }

  /* ---------------- actions ---------------- */
  async function doAction(act, item) {
    if (!item) return;
    try {
      if (act === 'open') {
        await invoke('open_path', { path: item.path });
      } else if (act === 'reveal') {
        await invoke('reveal_path', { path: item.path });
      } else if (act === 'copy') {
        await copyText(item.text || '', '文字已复制');
      } else if (act === 'remove') {
        var list = activeTab === 'send' ? 'outbox' : 'inbox';
        if (list === 'inbox') S.inbox = S.inbox.filter(function (i) { return i.id !== item.id; });
        else S.outbox = S.outbox.filter(function (i) { return i.id !== item.id; });
        renderInbox();
        renderOutbox();
        await invoke('remove_item', { id: item.id, list: list });
        toast('已从列表移除，文件仍保留在磁盘');
      }
    } catch (err) {
      toast(String(err), 'err');
    }
  }

  function openPreview(item) {
    if (!item) return;
    previewItem = item;
    $('#modalTitle').textContent = item.kind === 'text' ? '文字消息' : item.name;
    var sub = item.kind === 'text'
      ? (item.at || '') + ' · 来自手机'
      : fmtSize(item.size) + ' · ' + (item.at || '') + ' · ' + item.path;
    $('#modalSub').textContent = sub;

    var body = '';
    if (item.kind === 'image') body = '<img src="' + fileUrl(item) + '" alt="">';
    else if (item.kind === 'video') body = '<video src="' + fileUrl(item) + '" controls autoplay></video>';
    else if (item.kind === 'text') body = '<pre>' + esc(item.text || '') + '</pre>';
    else body = '<pre>' + esc(item.path) + '</pre>';
    $('#modalBody').innerHTML = body;
    $('#modal').classList.add('on');

    $('#modalOpen').style.display = item.kind === 'text' ? 'none' : '';
    $('#modalReveal').style.display = item.kind === 'text' ? 'none' : '';
    $('#modalCopy').innerHTML = icon('copy') + (item.kind === 'text' ? ' 复制文字' : ' 复制路径');
  }

  function closePreview() {
    $('#modal').classList.remove('on');
    $('#modalBody').innerHTML = '';
    previewItem = null;
  }

  /* ---------------- tabs ---------------- */
  function setTab(name) {
    activeTab = name;
    Array.prototype.forEach.call(document.querySelectorAll('.tab'), function (tab) {
      tab.classList.toggle('active', tab.dataset.tab === name);
    });
    $('#panel-inbox').classList.toggle('hidden', name !== 'inbox');
    $('#panel-send').classList.toggle('hidden', name !== 'send');
    updateStatline();
  }

  /* ---------------- tauri wiring ---------------- */
  async function refreshState() {
    S = await invoke('get_state');
    renderAll();
  }

  async function addPaths(paths) {
    try {
      var added = await invoke('add_paths', { paths: paths });
      if (added && added.length) {
        setTab('send');
        toast('已加入 ' + added.length + ' 个文件，手机页面可下载');
        await refreshState();
      } else {
        toast('没有可发送的新文件', 'warn');
      }
    } catch (err) {
      toast(String(err), 'err');
    }
  }

  async function uploadToOutbox(files) {
    if (!files.length) return;
    var ok = 0;
    toast('正在发送 ' + files.length + ' 个文件…');
    for (var i = 0; i < files.length; i++) {
      var file = files[i];
      try {
        var res = await fetch(
          'http://127.0.0.1:' + S.port + '/api/outbox?name=' + encodeURIComponent(file.name) + '&k=' + encodeURIComponent(S.pin),
          { method: 'POST', body: file }
        );
        if (!res.ok) throw new Error('HTTP ' + res.status);
        ok++;
      } catch (err) {
        toast('发送失败：' + file.name, 'err');
      }
    }
    if (ok) {
      setTab('send');
      toast('已发送 ' + ok + ' 个文件，手机页面可下载');
      await refreshState();
    }
  }

  function bindEvents() {
    Array.prototype.forEach.call(document.querySelectorAll('.tab'), function (tab) {
      tab.addEventListener('click', function () { setTab(tab.dataset.tab); });
    });

    $('#grid').addEventListener('click', function (event) {
      var tile = event.target.closest('.tile');
      if (!tile) return;
      var item = (S.inbox || []).filter(function (i) { return i.id === tile.dataset.id; })[0];
      if (!item) return;
      var btn = event.target.closest('[data-act]');
      if (btn) {
        event.stopPropagation();
        doAction(btn.dataset.act, item);
      } else {
        openPreview(item);
      }
    });
    $('#grid').addEventListener('dblclick', function (event) {
      var tile = event.target.closest('.tile');
      if (!tile) return;
      var item = (S.inbox || []).filter(function (i) { return i.id === tile.dataset.id; })[0];
      if (item && item.kind !== 'text') doAction('open', item);
    });

    $('#sendList').addEventListener('click', function (event) {
      var row = event.target.closest('.send-row');
      if (!row) return;
      var item = (S.outbox || []).filter(function (i) { return i.id === row.dataset.id; })[0];
      var btn = event.target.closest('[data-act]');
      if (btn && item) doAction(btn.dataset.act, item);
    });

    $('#copyLink').addEventListener('click', function () {
      var url = currentUrl();
      if (!url) { toast('还没有可用的局域网地址', 'warn'); return; }
      copyText(url, '链接已复制，发到手机即可打开');
    });

    $('#cycleNet').addEventListener('click', function () {
      if (!S.urls || S.urls.length < 2) return;
      netIndex = (netIndex + 1) % S.urls.length;
      renderConnect();
      toast('已切换到 ' + S.urls[netIndex].replace(/\?.*$/, '').replace(/\/$/, ''));
    });

    $('#connectHead').addEventListener('click', function () {
      var card = $('#connectCard');
      card.classList.toggle('collapsed');
      manualOpen = !card.classList.contains('collapsed');
      if (manualOpen) clearTimeout(peerTimer);
    });
    $('#netCount').addEventListener('click', function () {
      if (!S.urls || S.urls.length < 2) return;
      netIndex = (netIndex + 1) % S.urls.length;
      renderConnect();
    });

    $('#refreshNet').addEventListener('click', async function () {
      try {
        var net = await invoke('refresh_net');
        S.urls = net.urls;
        S.lan = net.lan;
        S.port = net.port;
        netIndex = 0;
        renderConnect();
        toast(net.urls && net.urls.length ? '已刷新网络地址' : '未找到局域网地址', net.urls && net.urls.length ? 'ok' : 'warn');
      } catch (err) {
        toast(String(err), 'err');
      }
    });

    $('#fixFirewall').addEventListener('click', async function () {
      try {
        var msg = await invoke('open_firewall');
        toast(msg, 'warn');
      } catch (err) {
        toast(String(err), 'err');
      }
    });

    $('#openSaveFolder').addEventListener('click', function () {
      invoke('open_folder', { path: S.saveDir }).catch(function (e) { toast(String(e), 'err'); });
    });
    $('#saveNote').addEventListener('click', function () {
      invoke('open_folder', { path: S.saveDir }).catch(function (e) { toast(String(e), 'err'); });
    });

    $('#quitBtn').addEventListener('click', async function () {
      if (window.confirm('退出星传？手机页面会立即断开。')) {
        try { await invoke('quit_app'); } catch (err) { window.close(); }
      }
    });

    $('#dropzone').addEventListener('click', function () { $('#fileInput').click(); });
    $('#pickFiles').addEventListener('click', function (event) {
      event.stopPropagation();
      $('#fileInput').click();
    });
    $('#fileInput').addEventListener('change', async function (event) {
      var files = Array.prototype.slice.call(event.target.files || []);
      event.target.value = '';
      await uploadToOutbox(files);
    });

    Array.prototype.forEach.call(document.querySelectorAll('[data-close]'), function (el) {
      el.addEventListener('click', closePreview);
    });
    $('#modalOpen').addEventListener('click', function () { if (previewItem) doAction('open', previewItem); });
    $('#modalReveal').addEventListener('click', function () { if (previewItem) doAction('reveal', previewItem); });
    $('#modalCopy').addEventListener('click', function () {
      if (!previewItem) return;
      if (previewItem.kind === 'text') copyText(previewItem.text || '', '文字已复制');
      else copyText(previewItem.path || '', '路径已复制');
    });
    document.addEventListener('keydown', function (event) {
      if (event.key === 'Escape' && $('#modal').classList.contains('on')) closePreview();
    });
  }

  async function bindTauriEvents() {
    await TAURI.event.listen('inbox://new', async function (event) {
      var item = event.payload || {};
      toast(item.kind === 'text' ? '收到一条文字消息' : '收到 ' + item.name);
      try { await refreshState(); } catch (err) { /* ignore */ }
    });
    await TAURI.event.listen('inbox://change', refreshState);
    await TAURI.event.listen('outbox://change', refreshState);
    await TAURI.event.listen('peer://seen', markPeer);
  }

  async function bindDrag() {
    if (!TAURI.webview) return;
    try {
      var view = TAURI.webview.getCurrentWebview();
      await view.onDragDropEvent(function (event) {
        var payload = event.payload || {};
        if (payload.type === 'enter' || payload.type === 'over') {
          $('#dropOverlay').classList.add('on');
        } else if (payload.type === 'drop') {
          $('#dropOverlay').classList.remove('on');
          if (payload.paths && payload.paths.length) addPaths(payload.paths);
        } else {
          $('#dropOverlay').classList.remove('on');
        }
      });
    } catch (err) {
      /* older runtime: ignore */
    }
  }

  /* ---------------- mock (browser preview only) ---------------- */
  function demoImage(a, b, label) {
    var svg = "<svg xmlns='http://www.w3.org/2000/svg' width='560' height='560'>" +
      "<defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>" +
      "<stop offset='0' stop-color='" + a + "'/><stop offset='1' stop-color='" + b + "'/></linearGradient></defs>" +
      "<rect width='560' height='560' fill='url(#g)'/>" +
      "<circle cx='150' cy='150' r='110' fill='rgba(255,255,255,.16)'/>" +
      "<path d='M0 430 L170 280 L320 410 L440 320 L560 410 L560 560 L0 560Z' fill='rgba(0,0,0,.24)'/>" +
      "<text x='42' y='86' font-family='sans-serif' font-size='32' fill='rgba(255,255,255,.92)'>" + label + "</text>" +
      "<text x='42' y='126' font-family='sans-serif' font-size='17' fill='rgba(255,255,255,.55)'>StarDrop preview</text>" +
      "</svg>";
    return 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  }

  function mockState() {
    var t = ['14:32', '14:28', '14:19', '13:52', '13:47', '12:30', '11:08', '10:41'];
    var demos = [
      demoImage('#7c5cff', '#2b1d63', '海边落日'),
      demoImage('#22d3ee', '#0e4a63', '窗边的猫'),
      demoImage('#f472b6', '#5b1d47', '咖啡与书'),
      demoImage('#45e0a8', '#0f3d33', '山顶云海'),
      demoImage('#f5b845', '#5c3710', '夜色街灯'),
      demoImage('#8b6bff', '#101a4a', '合照 2026')
    ];
    var inbox = demos.map(function (demo, i) {
      return {
        id: 'demo' + i, name: ['IMG_20261122_143201.jpg', 'PXL_20261122_142833.jpg', 'IMG_1002.HEIC',
          'mmexport1732.jpg', 'Screenshot_1122.png', 'IMG_20261122_104155.jpg'][i],
        path: 'C:\\Users\\you\\Pictures\\星传\\2026-11-22\\photo' + i + '.jpg',
        size: [2483100, 1864300, 3120000, 4280000, 842000, 2054000][i],
        kind: 'image', ext: i === 2 ? 'heic' : 'jpg', at: t[i], demo: demo, text: null, source: 'phone'
      };
    });
    inbox.unshift({
      id: 'demo-text', name: '今晚 7 点老地方见，我把地址发你', path: '', size: 86,
      kind: 'text', ext: 'txt', at: '14:35', text: '今晚 7 点老地方见，我把地址发你：\n滨江路 88 号 · 三楼露台', source: 'phone'
    });
    var outbox = [
      { id: 'out1', name: '行程单.pdf', path: 'C:\\Users\\you\\Documents\\行程单.pdf', size: 284000, kind: 'file', ext: 'pdf', at: '14:30' },
      { id: 'out2', name: 'DSC_8842.jpg', path: 'C:\\Users\\you\\Pictures\\DSC_8842.jpg', size: 5240000, kind: 'image', ext: 'jpg', at: '14:31', demo: demoImage('#5b6cff', '#1b2250', 'DSC_8842') }
    ];
    return {
      pin: '4821', port: 47823, device: 'DESKTOP-STAR', urls: ['http://192.168.1.23:47823/?k=4821', 'http://192.168.43.1:47823/?k=4821'],
      saveDir: 'C:\\Users\\you\\Pictures\\星传', inbox: inbox, outbox: outbox, lan: '192.168.1.23', version: '1.0.0'
    };
  }

  async function mockInvoke(cmd, args) {
    if (cmd === 'get_state') return S;
    if (cmd === 'refresh_net') return { urls: S.urls, lan: S.lan, port: S.port };
    if (cmd === 'remove_item') {
      if (args.list === 'inbox') S.inbox = S.inbox.filter(function (i) { return i.id !== args.id; });
      else S.outbox = S.outbox.filter(function (i) { return i.id !== args.id; });
      return true;
    }
    if (cmd === 'add_paths') {
      var added = (args.paths || []).map(function (p, i) {
        var name = p.split('\\').pop();
        var item = { id: 'mock' + Date.now() + i, name: name, path: p, size: 1200000, kind: 'file', ext: 'jpg', at: '14:40' };
        S.outbox.unshift(item);
        return item;
      });
      return added;
    }
    if (cmd === 'open_path' || cmd === 'reveal_path' || cmd === 'open_folder') return true;
    if (cmd === 'copy_text') return true;
    if (cmd === 'open_firewall') return '已弹出授权窗口';
    if (cmd === 'quit_app') return true;
    return null;
  }

  /* ---------------- boot ---------------- */
  async function boot() {
    bindEvents();
    if (isApp) {
      try {
        S = await invoke('get_state');
        await bindTauriEvents();
        bindDrag();
      } catch (err) {
        console.error(err);
      }
    } else {
      S = mockState();
    }
    renderAll();
    setInterval(function () {
      if (S.connected && Date.now() - (S.lastPeer || 0) > 24000) setConnected(false);
    }, 4000);
  }

  boot();
})();
