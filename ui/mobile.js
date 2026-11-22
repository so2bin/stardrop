(function () {
  'use strict';

  var $ = function (s) { return document.querySelector(s); };
  var KEY_STORE = 'stardrop.key';
  var params = new URLSearchParams(location.search);
  var KEY = params.get('k') || localStorage.getItem(KEY_STORE) || '';
  if (params.get('k')) {
    try { localStorage.setItem(KEY_STORE, params.get('k')); } catch (e) { /* private mode */ }
  }

  var MAX_PARALLEL = 3;
  var jobs = [];
  var active = 0;
  var totalCount = 0;
  var doneCount = 0;
  var totalBytes = 0;

  function esc(v) {
    return String(v == null ? '' : v).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }
  function fmtSize(n) {
    n = Number(n) || 0;
    if (n < 1024) return n + ' B';
    if (n < 1048576) return (n / 1024).toFixed(0) + ' KB';
    if (n < 1073741824) return (n / 1048576).toFixed(1) + ' MB';
    return (n / 1073741824).toFixed(2) + ' GB';
  }
  function icon(name) { return '<svg><use href="#i-' + name + '"/></svg>'; }

  var toastTimer = null;
  function toast(message, bad) {
    var el = $('#toast');
    el.textContent = message;
    el.classList.toggle('bad', !!bad);
    el.classList.add('on');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { el.classList.remove('on'); }, 2600);
  }
  function buzz() { try { navigator.vibrate && navigator.vibrate(8); } catch (e) { /* ignore */ } }

  /* ---------------- pin sheet ---------------- */
  function showPin(message) {
    $('#pinErr').textContent = message || '';
    $('#pinSheet').classList.add('on');
    $('#pinSheet').setAttribute('aria-hidden', 'false');
    document.body.style.overflow = 'hidden';
    setTimeout(function () { $('#pinInput').focus(); }, 260);
  }
  function hidePin() {
    $('#pinSheet').classList.remove('on');
    $('#pinSheet').setAttribute('aria-hidden', 'true');
    document.body.style.overflow = '';
  }
  function applyKey(key) {
    KEY = key;
    try { localStorage.setItem(KEY_STORE, key); } catch (e) { /* ignore */ }
    var url = new URL(location.href);
    url.searchParams.set('k', key);
    history.replaceState(null, '', url.toString());
  }
  async function submitPin() {
    var value = ($('#pinInput').value || '').replace(/\D/g, '').slice(0, 4);
    if (value.length !== 4) {
      $('#pinErr').textContent = '请输入 4 位数字';
      return;
    }
    $('#pinGo').disabled = true;
    $('#pinGo').textContent = '连接中…';
    try {
      var res = await fetch('/api/outbox?k=' + encodeURIComponent(value), { cache: 'no-store' });
      if (res.ok) {
        applyKey(value);
        hidePin();
        $('#pinInput').value = '';
        $('#pinErr').textContent = '';
        toast('已连接到电脑');
        loadRecv();
      } else {
        $('#pinErr').textContent = '配对码不对，再看看电脑上的数字';
      }
    } catch (err) {
      $('#pinErr').textContent = '连接失败，请确认和电脑在同一个网络';
    } finally {
      $('#pinGo').disabled = false;
      $('#pinGo').textContent = '连接';
    }
  }

  /* ---------------- uploader ---------------- */
  function setProgress(job, ratio) {
    var pct = Math.max(0, Math.min(100, Math.round(ratio * 100)));
    job.card.querySelector('.q-bar i').style.width = pct + '%';
    if (pct < 100) job.card.querySelector('.q-state').textContent = pct + '%';
  }

  function updateSummary() {
    var sum = $('#queueSum');
    if (!totalCount) { sum.textContent = ''; return; }
    sum.textContent = doneCount + ' / ' + totalCount + ' · ' + fmtSize(totalBytes);
    if (doneCount === totalCount) {
      $('#queueTitle').textContent = '已全部送达电脑';
    } else {
      $('#queueTitle').textContent = '正在发送';
    }
  }

  function createCard(file) {
    var isImage = /^image\//.test(file.type) || /\.(jpe?g|png|webp|gif|bmp|heic|heif)$/i.test(file.name);
    var isVideo = /^video\//.test(file.type);
    var card = document.createElement('div');
    card.className = 'q-card';
    var thumbInner;
    if (isImage && /^image\/(jpeg|png|webp|gif|bmp)$/.test(file.type)) {
      thumbInner = '<img alt="">';
    } else {
      thumbInner = icon(isVideo ? 'video' : isImage ? 'image' : 'file');
    }
    card.innerHTML =
      '<div class="q-thumb">' + thumbInner +
      '<span class="badge">' + icon('check') + '</span></div>' +
      '<div class="q-main">' +
      '<div class="q-top"><span class="q-name"></span><span class="q-state">准备中</span></div>' +
      '<div class="q-meta"><span class="q-size">' + fmtSize(file.size) + '</span>' +
      '<span class="q-bar"><i></i></span></div></div>';
    card.querySelector('.q-name').textContent = file.name;
    var img = card.querySelector('img');
    if (img) img.src = URL.createObjectURL(file);
    $('#queue').appendChild(card);
    $('#queueWrap').hidden = false;
    return card;
  }

  function upload(job) {
    return new Promise(function (resolve) {
      var file = job.file;
      var card = job.card;
      var xhr = new XMLHttpRequest();
      xhr.open('POST', '/api/upload?name=' + encodeURIComponent(file.name));
      xhr.setRequestHeader('X-Key', KEY);
      xhr.upload.onprogress = function (event) {
        if (event.lengthComputable) setProgress(job, event.loaded / event.total);
      };
      xhr.onload = function () {
        if (xhr.status === 200) {
          card.classList.add('done');
          card.classList.remove('error');
          card.querySelector('.q-state').textContent = '已送达';
          setProgress(job, 1);
          doneCount++;
          updateSummary();
          buzz();
        } else if (xhr.status === 403) {
          card.classList.add('error');
          card.querySelector('.q-state').textContent = '配对码错误';
          showPin('配对码已失效，请重新输入');
        } else {
          card.classList.add('error');
          card.querySelector('.q-state').textContent = '失败';
        }
        resolve();
      };
      xhr.onerror = function () {
        card.classList.add('error');
        card.querySelector('.q-state').textContent = '网络中断';
        resolve();
      };
      xhr.ontimeout = xhr.onerror;
      card.querySelector('.q-state').textContent = '上传中';
      xhr.send(file);
    });
  }

  function pump() {
    while (active < MAX_PARALLEL && jobs.length) {
      var job = jobs.shift();
      active++;
      upload(job).then(function () {
        active--;
        pump();
      });
    }
  }

  function enqueue(files) {
    Array.prototype.forEach.call(files, function (file) {
      var card = createCard(file);
      totalCount++;
      totalBytes += file.size || 0;
      jobs.push({ file: file, card: card });
    });
    updateSummary();
    pump();
  }

  /* ---------------- receive list ---------------- */
  var recvBusy = false;
  async function loadRecv() {
    if (recvBusy) return;
    recvBusy = true;
    try {
      var res = await fetch('/api/outbox?k=' + encodeURIComponent(KEY), { cache: 'no-store' });
      if (res.status === 403) {
        showPin('输入电脑上显示的 4 位配对码');
        return;
      }
      var data = await res.json();
      renderRecv(data.items || []);
      $('#netWarn').classList.remove('on');
      $('#pcChip').classList.add('ok');
    } catch (err) {
      $('#netWarn').classList.add('on');
    } finally {
      recvBusy = false;
    }
  }

  function renderRecv(items) {
    var list = $('#recvList');
    if (!items.length) {
      list.innerHTML = '<p class="muted">电脑还没有发送文件</p>';
      return;
    }
    list.innerHTML = items.map(function (item) {
      var thumb = item.kind === 'image'
        ? '<img src="/thumb?id=' + encodeURIComponent(item.id) + '&k=' + encodeURIComponent(KEY) + '" alt="">'
        : icon(item.kind === 'video' ? 'video' : 'file');
      var href = '/download?id=' + encodeURIComponent(item.id) + '&k=' + encodeURIComponent(KEY);
      return '<div class="recv-row">' +
        '<div class="recv-thumb">' + thumb + '</div>' +
        '<div class="recv-info"><span class="n">' + esc(item.name) + '</span>' +
        '<span class="m">' + fmtSize(item.size) + (item.at ? ' · ' + esc(item.at) : '') + '</span></div>' +
        '<a class="dl" href="' + href + '" download="' + esc(item.name) + '" rel="noopener">' + icon('download') + '</a>' +
        '</div>';
    }).join('');
    Array.prototype.forEach.call(list.querySelectorAll('img'), function (img) {
      img.addEventListener('error', function () { img.style.display = 'none'; }, { once: true });
    });
  }

  /* ---------------- text ---------------- */
  async function sendText() {
    var input = $('#textInput');
    var value = (input.value || '').trim();
    if (!value) { input.focus(); return; }
    var btn = $('#sendText');
    btn.classList.add('sending');
    try {
      var res = await fetch('/api/text?k=' + encodeURIComponent(KEY), {
        method: 'POST',
        headers: { 'X-Key': KEY, 'Content-Type': 'text/plain; charset=utf-8' },
        body: value
      });
      if (res.status === 403) {
        showPin('配对码已失效，请重新输入');
        return;
      }
      if (!res.ok) throw new Error('HTTP ' + res.status);
      input.value = '';
      input.blur();
      toast('已发到电脑');
      buzz();
    } catch (err) {
      toast('发送失败，检查一下网络', true);
    } finally {
      btn.classList.remove('sending');
    }
  }

  /* ---------------- boot ---------------- */
  async function boot() {
    try {
      var info = await fetch('/api/info', { cache: 'no-store' }).then(function (r) { return r.json(); });
      if (info && info.device) {
        $('#deviceName').textContent = info.device;
        document.title = '星传 · ' + info.device;
      }
    } catch (err) {
      $('#deviceName').textContent = '电脑';
      $('#netWarn').classList.add('on');
    }

    $('#pick').addEventListener('change', function (event) {
      var files = event.target.files;
      if (files && files.length) enqueue(files);
      event.target.value = '';
    });
    $('#camera').addEventListener('change', function (event) {
      var files = event.target.files;
      if (files && files.length) enqueue(files);
      event.target.value = '';
    });
    $('#sendText').addEventListener('click', sendText);
    $('#textInput').addEventListener('keydown', function (event) {
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') sendText();
    });
    $('#refreshRecv').addEventListener('click', function () { loadRecv(); });
    $('#pinGo').addEventListener('click', submitPin);
    $('#pinInput').addEventListener('input', function (event) {
      event.target.value = event.target.value.replace(/\D/g, '').slice(0, 4);
      $('#pinErr').textContent = '';
      if (event.target.value.length === 4) submitPin();
    });
    $('#pinInput').addEventListener('keydown', function (event) {
      if (event.key === 'Enter') submitPin();
    });
    Array.prototype.forEach.call(document.querySelectorAll('[data-close-pin]'), function (el) {
      el.addEventListener('click', hidePin);
    });
    $('#changePin').addEventListener('click', function () {
      $('#pinInput').value = '';
      showPin('');
    });

    document.addEventListener('visibilitychange', function () {
      if (!document.hidden) loadRecv();
    });
    setInterval(function () {
      if (!document.hidden) loadRecv();
    }, 5000);

    if (!KEY) {
      showPin('第一次连接，需要电脑上的 4 位配对码');
    } else {
      loadRecv();
    }
  }

  boot();
})();
