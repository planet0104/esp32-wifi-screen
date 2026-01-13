#!/usr/bin/env node
// Simple serial port test tool
// Usage: node serial_test.js --port COM6 --baud 115200 --interval 1000

const { SerialPort } = require('serialport');
const argv = require('minimist')(process.argv.slice(2), {
  string: ['port'],
  default: { baud: 115200, interval: 1000, chunks: 0 }
});

if (!argv.port) {
  console.error('Usage: node serial_test.js --port COMx [--baud 115200] [--interval 1000] [--once]');
  process.exit(2);
}

const portName = argv.port;
const baudRate = parseInt(argv.baud, 10) || 115200;
const intervalMs = parseInt(argv.interval, 10) || 1000;
const sendOnce = !!argv.once;
const chunks = parseInt(argv.chunks || 0, 10);

console.log(`Opening ${portName} at ${baudRate} baud...`);

const port = new SerialPort({ path: portName, baudRate, autoOpen: false });

port.on('error', (err) => {
  console.error('SerialPort error:', err.message);
});

port.on('open', () => {
  console.log('Port opened.');
  if (sendOnce) {
    sendPing();
  } else {
    startLoop();
  }
});

let sendTimer = null;
let seq = 0;

function toHex(buf) {
  return Array.from(buf).map(b => b.toString(16).padStart(2, '0').toUpperCase()).join(' ');
}

port.on('data', (data) => {
  const ts = new Date().toISOString();
  try {
    const txt = data.toString('utf8');
    process.stdout.write(`<- [${ts}] ascii: ${txt.replace(/\r/g, '\\r').replace(/\n/g, '\\n\n')}`);
  } catch (e) {
    // ignore
  }
  console.log(`<- [${ts}] hex: ${toHex(data)}`);
});

function sendPing() {
  const payload = `PING ${seq}\n`;
  const ts = new Date().toISOString();
  port.write(payload, (err) => {
    if (err) console.error('Write error:', err.message);
    else console.log(`-> [${ts}] ${payload.trim()}`);
  });
  seq = (seq + 1) & 0xFFFF;
}

function startLoop() {
  sendPing();
  sendTimer = setInterval(() => {
    sendPing();
  }, intervalMs);
}

function stopLoop() {
  if (sendTimer) { clearInterval(sendTimer); sendTimer = null; }
}

process.on('SIGINT', () => {
  console.log('\nCaught SIGINT, exiting...');
  stopLoop();
  port.close(() => process.exit(0));
});

port.open((err) => {
  if (err) {
    console.error('Failed to open port:', err.message);
    process.exit(1);
  }
});
