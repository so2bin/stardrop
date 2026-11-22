/* StarDrop QR — byte mode, EC level M, versions 1-6 (ISO/IEC 18004). */
(function (global) {
  'use strict';

  var TOTAL_CODEWORDS = [0, 26, 44, 70, 100, 134, 172, 196];
  var ECC_PER_BLOCK_M = [0, 10, 16, 26, 18, 24, 16, 18];
  var BLOCKS_M = [0, 1, 1, 1, 2, 2, 4, 4];
  var ALIGN = [null, [], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34]];
  var MAX_VERSION = 6;

  function gfMul(x, y) {
    var z = 0;
    for (var i = 7; i >= 0; i--) {
      z = (z << 1) ^ ((z >>> 7) * 0x11d);
      z ^= ((y >>> i) & 1) * x;
    }
    return z & 0xff;
  }

  function rsDivisor(degree) {
    var result = new Array(degree).fill(0);
    result[degree - 1] = 1;
    var root = 1;
    for (var i = 0; i < degree; i++) {
      for (var j = 0; j < degree; j++) {
        result[j] = gfMul(result[j], root);
        if (j + 1 < degree) result[j] ^= result[j + 1];
      }
      root = gfMul(root, 0x02);
    }
    return result;
  }

  function rsRemainder(data, divisor) {
    var result = new Array(divisor.length).fill(0);
    for (var i = 0; i < data.length; i++) {
      var factor = data[i] ^ result.shift();
      result.push(0);
      for (var j = 0; j < result.length; j++) result[j] ^= gfMul(divisor[j], factor);
    }
    return result;
  }

  function dataCapacity(ver) {
    return TOTAL_CODEWORDS[ver] - ECC_PER_BLOCK_M[ver] * BLOCKS_M[ver];
  }

  function chooseVersion(byteLen) {
    for (var v = 1; v <= MAX_VERSION; v++) {
      if (byteLen <= dataCapacity(v) - 2) return v;
    }
    return -1;
  }

  function encodeData(bytes, ver) {
    var capacityBits = dataCapacity(ver) * 8;
    var bits = [];
    function push(val, len) {
      for (var i = len - 1; i >= 0; i--) bits.push((val >>> i) & 1);
    }
    push(0b0100, 4);
    push(bytes.length, 8);
    for (var i = 0; i < bytes.length; i++) push(bytes[i], 8);
    var term = Math.min(4, capacityBits - bits.length);
    for (var t = 0; t < term; t++) bits.push(0);
    while (bits.length % 8 !== 0) bits.push(0);
    var out = [];
    for (var b = 0; b < bits.length; b += 8) {
      var val = 0;
      for (var k = 0; k < 8; k++) val = (val << 1) | bits[b + k];
      out.push(val);
    }
    var pad = 0;
    while (out.length < dataCapacity(ver)) {
      out.push(pad % 2 === 0 ? 0xec : 0x11);
      pad++;
    }
    return out;
  }

  function addEcc(data, ver) {
    var numBlocks = BLOCKS_M[ver];
    var ecLen = ECC_PER_BLOCK_M[ver];
    var raw = TOTAL_CODEWORDS[ver];
    var numShort = numBlocks - (raw % numBlocks || numBlocks);
    var shortDataLen = Math.floor(raw / numBlocks) - ecLen;
    var blocks = [];
    var offset = 0;
    for (var i = 0; i < numBlocks; i++) {
      var len = shortDataLen + (i < numShort ? 0 : 1);
      var dat = data.slice(offset, offset + len);
      offset += len;
      blocks.push({ dat: dat, ecc: rsRemainder(dat, rsDivisor(ecLen)) });
    }
    var result = [];
    for (var r = 0; r <= shortDataLen; r++) {
      for (var j = 0; j < numBlocks; j++) {
        if (r < blocks[j].dat.length) result.push(blocks[j].dat[r]);
      }
    }
    for (var e = 0; e < ecLen; e++) {
      for (var m = 0; m < numBlocks; m++) result.push(blocks[m].ecc[e]);
    }
    return result;
  }

  function maskBit(mask, x, y) {
    switch (mask) {
      case 0: return (x + y) % 2 === 0;
      case 1: return y % 2 === 0;
      case 2: return x % 3 === 0;
      case 3: return (x + y) % 3 === 0;
      case 4: return (Math.floor(y / 2) + Math.floor(x / 3)) % 2 === 0;
      case 5: return ((x * y) % 2) + ((x * y) % 3) === 0;
      case 6: return (((x * y) % 2) + ((x * y) % 3)) % 2 === 0;
      case 7: return (((x + y) % 2) + ((x * y) % 3)) % 2 === 0;
    }
    return false;
  }

  function formatWord(mask) {
    var data = mask; // EC level M -> 00
    var rem = data;
    for (var i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
    return ((data << 10) | rem) ^ 0x5412;
  }

  function drawFormat(m, size, mask, fn) {
    var bits = formatWord(mask);
    function get(i) { return ((bits >>> i) & 1) !== 0; }
    function put(x, y, v) {
      m[y][x] = v;
      if (fn) fn[y][x] = true;
    }
    for (var i = 0; i <= 5; i++) put(8, i, get(i));
    put(8, 7, get(6));
    put(8, 8, get(7));
    put(7, 8, get(8));
    for (var j = 9; j < 15; j++) put(14 - j, 8, get(j));
    for (var a = 0; a < 8; a++) put(size - 1 - a, 8, get(a));
    for (var b = 8; b < 15; b++) put(8, size - 15 + b, get(b));
    put(8, size - 8, true);
  }

  function build(text) {
    var bytes = new TextEncoder().encode(text);
    var ver = chooseVersion(bytes.length);
    if (ver < 0) throw new Error('QR content too long');
    var codewords = addEcc(encodeData(bytes, ver), ver);
    var size = ver * 4 + 17;
    var base = [];
    var fn = [];
    for (var y = 0; y < size; y++) {
      base.push(new Array(size).fill(false));
      fn.push(new Array(size).fill(false));
    }
    function set(x, y, dark) {
      if (x < 0 || y < 0 || x >= size || y >= size) return;
      base[y][x] = dark;
      fn[y][x] = true;
    }
    function finder(cx, cy) {
      for (var dy = -4; dy <= 4; dy++) {
        for (var dx = -4; dx <= 4; dx++) {
          var dist = Math.max(Math.abs(dx), Math.abs(dy));
          set(cx + dx, cy + dy, dist !== 2 && dist !== 4);
        }
      }
    }
    finder(3, 3);
    finder(size - 4, 3);
    finder(3, size - 4);

    for (var i = 8; i < size - 8; i++) {
      set(i, 6, i % 2 === 0);
      set(6, i, i % 2 === 0);
    }
    var centers = ALIGN[ver];
    for (var a = 0; a < centers.length; a++) {
      for (var b = 0; b < centers.length; b++) {
        var cx = centers[a];
        var cy = centers[b];
        var last = size - 7;
        if ((cx === 6 && cy === 6) || (cx === 6 && cy === last) || (cx === last && cy === 6)) continue;
        for (var dy2 = -2; dy2 <= 2; dy2++) {
          for (var dx2 = -2; dx2 <= 2; dx2++) {
            set(cx + dx2, cy + dy2, Math.max(Math.abs(dx2), Math.abs(dy2)) !== 1);
          }
        }
      }
    }
    drawFormat(base, size, 0, fn);

    var bitIndex = 0;
    for (var right = size - 1; right >= 1; right -= 2) {
      if (right === 6) right = 5;
      for (var vert = 0; vert < size; vert++) {
        for (var j = 0; j < 2; j++) {
          var x = right - j;
          var upward = ((right + 1) & 2) === 0;
          var yy = upward ? size - 1 - vert : vert;
          if (!fn[yy][x] && bitIndex < codewords.length * 8) {
            base[yy][x] = ((codewords[bitIndex >>> 3] >>> (7 - (bitIndex & 7))) & 1) !== 0;
            bitIndex++;
          }
        }
      }
    }

    var best = null;
    var bestScore = Infinity;
    for (var mask = 0; mask < 8; mask++) {
      var cand = base.map(function (row) { return row.slice(); });
      for (var ry = 0; ry < size; ry++) {
        for (var rx = 0; rx < size; rx++) {
          if (!fn[ry][rx] && maskBit(mask, rx, ry)) cand[ry][rx] = !cand[ry][rx];
        }
      }
      drawFormat(cand, size, mask, null);
      var score = penalty(cand, size);
      if (score < bestScore) {
        bestScore = score;
        best = cand;
      }
    }
    return { size: size, modules: best, version: ver };
  }

  function penalty(m, size) {
    var score = 0;
    var x, y, run, color;
    for (y = 0; y < size; y++) {
      run = 1;
      color = m[y][0];
      for (x = 1; x < size; x++) {
        if (m[y][x] === color) {
          run++;
          if (run === 5) score += 3;
          else if (run > 5) score += 1;
        } else {
          color = m[y][x];
          run = 1;
        }
      }
    }
    for (x = 0; x < size; x++) {
      run = 1;
      color = m[0][x];
      for (y = 1; y < size; y++) {
        if (m[y][x] === color) {
          run++;
          if (run === 5) score += 3;
          else if (run > 5) score += 1;
        } else {
          color = m[y][x];
          run = 1;
        }
      }
    }
    for (y = 0; y < size - 1; y++) {
      for (x = 0; x < size - 1; x++) {
        var c = m[y][x];
        if (c === m[y][x + 1] && c === m[y + 1][x] && c === m[y + 1][x + 1]) score += 3;
      }
    }
    var dark = 0;
    for (y = 0; y < size; y++) for (x = 0; x < size; x++) if (m[y][x]) dark++;
    var total = size * size;
    var k = Math.ceil(Math.abs(dark * 20 - total * 10) / total) - 1;
    if (k > 0) score += k * 10;
    return score;
  }

  function render(canvas, text, opts) {
    opts = opts || {};
    var qr = build(text);
    var quiet = 4;
    var total = qr.size + quiet * 2;
    var target = opts.size || 206;
    var dpr = Math.min(global.devicePixelRatio || 1, 3);
    var scale = Math.max(2, Math.floor((target * dpr) / total));
    var px = total * scale;
    canvas.width = px;
    canvas.height = px;
    canvas.style.width = Math.floor(px / dpr) + 'px';
    canvas.style.height = Math.floor(px / dpr) + 'px';
    var ctx = canvas.getContext('2d');
    ctx.fillStyle = opts.light || '#ffffff';
    ctx.fillRect(0, 0, px, px);
    ctx.fillStyle = opts.dark || '#0b0c12';
    for (var y = 0; y < qr.size; y++) {
      for (var x = 0; x < qr.size; x++) {
        if (qr.modules[y][x]) {
          ctx.fillRect((x + quiet) * scale, (y + quiet) * scale, scale, scale);
        }
      }
    }
    return qr;
  }

  global.StarQR = { build: build, render: render };
})(window);
