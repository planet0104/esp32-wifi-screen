#!/usr/bin/env node
// ESP32 echo throughput test tool (host side)
// For current firmware behavior: device echoes back all received bytes.
//
// Usage examples:
// node serial_speed_test.js --port COM6
// node serial_speed_test.js --port COM6 --seconds 10 --frame 1024 --inflight 64
// node serial_speed_test.js --list
//
// Notes:
// 1. "baud" is required by OS APIs, but for USB Serial/JTAG it usually does not limit throughput.
// 2. To measure higher throughput, increase --frame and --inflight, and avoid heavy --verify-bytes.

const { SerialPort } = require('serialport');
const minimist = require('minimist');

const argv = minimist(process.argv.slice(2), {
  string: ['port'],
  boolean: ['list'],
  default: {
    baud: 115200,
    seconds: 10,
    frame: 1024,
    inflight: 64,
    'verify-bytes': 64,
    interval: 1000
  }
});

async function listPortsAndExit() {
  const ports = await SerialPort.list();
  if (!ports || ports.length === 0) {
    console.log('No serial ports found');
    process.exit(0);
  }
  console.log('Available serial ports:');
  for (const p of ports) {
    const vidpid = (p.vendorId && p.productId) ? ` vid=${p.vendorId} pid=${p.productId}` : '';
    const mfg = p.manufacturer ? ` mfg=${p.manufacturer}` : '';
    const sn = p.serialNumber ? ` sn=${p.serialNumber}` : '';
    console.log(`- ${p.path}${vidpid}${mfg}${sn}`);
  }
  process.exit(0);
}

function nowNs() {
  return process.hrtime.bigint();
}

function nsToMs(ns) {
  return Number(ns) / 1e6;
}

function formatBps(bytesPerSec) {
  if (!isFinite(bytesPerSec)) return 'n/a';
  if (bytesPerSec >= 1024 * 1024) return `${(bytesPerSec / (1024 * 1024)).toFixed(2)} MB/s`;
  if (bytesPerSec >= 1024) return `${(bytesPerSec / 1024).toFixed(2)} KB/s`;
  return `${bytesPerSec.toFixed(2)} B/s`;
}

function buildFrame(seq, size) {
  const buf = Buffer.allocUnsafe(size);
  buf.writeUInt32LE(seq >>> 0, 0);
  for (let i = 4; i < size; i++) {
    buf[i] = (seq + i) & 0xff;
  }
  return buf;
}

function verifyFrame(buf, expectSeq, verifyBytes) {
  if (buf.length < 4) return { ok: false, reason: 'frame_too_small' };
  const seq = buf.readUInt32LE(0) >>> 0;
  if (seq !== (expectSeq >>> 0)) return { ok: false, reason: `seq_mismatch expect=${expectSeq} got=${seq}` };
  const n = Math.min(buf.length, Math.max(4, verifyBytes | 0));
  for (let i = 4; i < n; i++) {
    const exp = (expectSeq + i) & 0xff;
    if (buf[i] !== exp) return { ok: false, reason: `data_mismatch at=${i} expect=${exp} got=${buf[i]}` };
  }
  return { ok: true };
}

class ChunkQueue {
  constructor() {
    this.chunks = [];
    this.len = 0;
  }
  push(buf) {
    if (!buf || buf.length === 0) return;
    this.chunks.push(buf);
    this.len += buf.length;
  }
  take(n) {
    if (n <= 0 || this.len < n) return null;
    const out = Buffer.allocUnsafe(n);
    let outOff = 0;
    while (outOff < n) {
      const head = this.chunks[0];
      const need = n - outOff;
      if (head.length <= need) {
        head.copy(out, outOff);
        outOff += head.length;
        this.chunks.shift();
      } else {
        head.copy(out, outOff, 0, need);
        this.chunks[0] = head.subarray(need);
        outOff += need;
      }
    }
    this.len -= n;
    return out;
  }
}

async function main() {
  if (argv.list) await listPortsAndExit();

  const portName = argv.port;
  if (!portName) {
    console.error('Usage: node serial_speed_test.js --port COMx [--seconds 10] [--frame 1024] [--inflight 64] [--verify-bytes 64] [--interval 1000]');
    console.error('       node serial_speed_test.js --list');
    process.exit(2);
  }

  const baudRate = parseInt(argv.baud, 10) || 115200;
  const seconds = Math.max(1, parseInt(argv.seconds, 10) || 10);
  const frameSize = Math.max(16, parseInt(argv.frame, 10) || 1024);
  const inFlightMax = Math.max(1, parseInt(argv.inflight, 10) || 64);
  const verifyBytes = Math.max(4, parseInt(argv['verify-bytes'], 10) || 64);
  const intervalMs = Math.max(200, parseInt(argv.interval, 10) || 1000);

  console.log('Serial speed test config:');
  console.log(`- port=${portName}`);
  console.log(`- baud=${baudRate}`);
  console.log(`- seconds=${seconds}`);
  console.log(`- frame=${frameSize}`);
  console.log(`- inflight=${inFlightMax}`);
  console.log(`- verifyBytes=${verifyBytes}`);

  const port = new SerialPort({
    path: portName,
    baudRate,
    autoOpen: false,
    highWaterMark: 1024 * 1024
  });

  port.on('error', (err) => {
    console.error('SerialPort error:', err && err.message ? err.message : err);
  });

  const rxQ = new ChunkQueue();

  let startNs = 0n;
  let endNs = 0n;
  let lastReportNs = 0n;

  let txBytes = 0;
  let rxBytes = 0;
  let okFrames = 0;
  let badFrames = 0;

  let nextSeq = 1;
  const pendingSeq = [];
  const sendTimeBySeq = new Map();

  let rttCount = 0;
  let rttSumNs = 0n;
  let rttMaxNs = 0n;

  let stopped = false;

  function trySendMore() {
    if (stopped) return;
    while (pendingSeq.length < inFlightMax) {
      const seq = nextSeq >>> 0;
      nextSeq = (nextSeq + 1) >>> 0;
      const frame = buildFrame(seq, frameSize);
      pendingSeq.push(seq);
      sendTimeBySeq.set(seq, nowNs());
      port.write(frame);
      txBytes += frame.length;
    }
  }

  function handleFrame(frame) {
    const expectSeq = pendingSeq.length > 0 ? pendingSeq[0] : null;
    if (expectSeq === null) {
      badFrames += 1;
      return;
    }
    const v = verifyFrame(frame, expectSeq, verifyBytes);
    if (!v.ok) {
      badFrames += 1;
      // best-effort resync: drop one expected seq
      pendingSeq.shift();
      sendTimeBySeq.delete(expectSeq);
      return;
    }

    pendingSeq.shift();
    okFrames += 1;

    const t0 = sendTimeBySeq.get(expectSeq);
    if (t0) {
      const rtt = nowNs() - t0;
      rttCount += 1;
      rttSumNs += rtt;
      if (rtt > rttMaxNs) rttMaxNs = rtt;
      sendTimeBySeq.delete(expectSeq);
    }
  }

  port.on('data', (data) => {
    rxBytes += data.length;
    rxQ.push(data);
    while (rxQ.len >= frameSize) {
      const frame = rxQ.take(frameSize);
      if (!frame) break;
      handleFrame(frame);
    }
  });

  function reportProgress(now) {
    const elapsedSec = Number(now - startNs) / 1e9;
    const txBps = txBytes / Math.max(0.001, elapsedSec);
    const rxBps = rxBytes / Math.max(0.001, elapsedSec);
    const avgRttMs = rttCount > 0 ? nsToMs(rttSumNs / BigInt(rttCount)) : 0;
    const maxRttMs = rttCount > 0 ? nsToMs(rttMaxNs) : 0;
    console.log(`Progress: t=${elapsedSec.toFixed(1)}s tx=${formatBps(txBps)} rx=${formatBps(rxBps)} okFrames=${okFrames} badFrames=${badFrames} avgRttMs=${avgRttMs.toFixed(2)} maxRttMs=${maxRttMs.toFixed(2)}`);
  }

  function finishAndPrint() {
    stopped = true;
    endNs = nowNs();
    const elapsedSec = Number(endNs - startNs) / 1e9;
    const txBps = txBytes / Math.max(0.001, elapsedSec);
    const rxBps = rxBytes / Math.max(0.001, elapsedSec);
    const avgRttMs = rttCount > 0 ? nsToMs(rttSumNs / BigInt(rttCount)) : 0;
    const maxRttMs = rttCount > 0 ? nsToMs(rttMaxNs) : 0;

    console.log('Test result:');
    console.log(`- durationSec=${elapsedSec.toFixed(3)}`);
    console.log(`- txBytes=${txBytes}`);
    console.log(`- rxBytes=${rxBytes}`);
    console.log(`- txThroughput=${formatBps(txBps)}`);
    console.log(`- rxThroughput=${formatBps(rxBps)}`);
    console.log(`- okFrames=${okFrames}`);
    console.log(`- badFrames=${badFrames}`);
    console.log(`- avgRttMs=${avgRttMs.toFixed(3)}`);
    console.log(`- maxRttMs=${maxRttMs.toFixed(3)}`);
  }

  function shutdown(code) {
    try {
      port.close(() => process.exit(code));
    } catch (e) {
      process.exit(code);
    }
  }

  process.on('SIGINT', () => {
    console.log('Caught SIGINT, stopping...');
    stopped = true;
    finishAndPrint();
    shutdown(0);
  });

  await new Promise((resolve, reject) => {
    port.open((err) => {
      if (err) reject(err);
      else resolve();
    });
  }).catch((err) => {
    console.error('Failed to open port:', err && err.message ? err.message : err);
    process.exit(1);
  });

  console.log('Port opened, starting test...');
  startNs = nowNs();
  lastReportNs = startNs;

  const reportTimer = setInterval(() => {
    if (!startNs) return;
    const t = nowNs();
    if (t - lastReportNs >= BigInt(intervalMs) * 1000000n) {
      lastReportNs = t;
      reportProgress(t);
    }
  }, intervalMs);

  trySendMore();

  const pumpTimer = setInterval(() => {
    trySendMore();
  }, 0);

  setTimeout(() => {
    clearInterval(pumpTimer);
    clearInterval(reportTimer);
    stopped = true;
    finishAndPrint();
    shutdown(0);
  }, seconds * 1000);
}

main().catch((e) => {
  console.error('Fatal:', e && e.message ? e.message : e);
  process.exit(1);
});

