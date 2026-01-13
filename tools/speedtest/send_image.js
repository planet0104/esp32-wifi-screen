#!/usr/bin/env node
// send_image.js
// Read a BMP (rgb24) file, convert to RGB565, LZ4-compress with size prepended,
// and send using the same binary image protocol expected by esp32 usb_reader.rs

const fs = require('fs');
const path = require('path');
const SerialPort = require('serialport');
const sharp = require('sharp');
const lz4 = require('lz4');
const argv = require('minimist')(process.argv.slice(2), {
  string: ['port','file'],
  default: { file: path.join(__dirname, 'rgb24.bmp'), port: null, x: 0, y: 0 }
});

if (!argv.port) {
  console.error('Usage: node send_image.js --port COMx [--file path] [--x 0] [--y 0]');
  process.exit(2);
}

async function bmpToRgb565(buf) {
  // Try sharp first (fast native implementation)
  try {
    const img = sharp(buf);
    const meta = await img.metadata();
    const width = meta.width;
    const height = meta.height;
    const raw = await img.raw().toBuffer(); // RGBRGB...

    const out = Buffer.alloc(width * height * 2);
    let ri = 0, wi = 0;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const r = raw[ri++];
        const g = raw[ri++];
        const b = raw[ri++];
        // convert to RGB565 (R:5 G:6 B:5)
        const r5 = (r >> 3) & 0x1f;
        const g6 = (g >> 2) & 0x3f;
        const b5 = (b >> 3) & 0x1f;
        const v = (r5 << 11) | (g6 << 5) | b5;
        // write as little-endian u16
        out[wi++] = v & 0xff;
        out[wi++] = (v >> 8) & 0xff;
      }
    }
    return { width, height, data: out };
  } catch (err) {
    // Fall back to Jimp which supports more BMP variants (pure JS)
    try {
      const Jimp = require('jimp');
      const image = await Jimp.read(buf);
      const width = image.bitmap.width;
      const height = image.bitmap.height;
      const raw = image.bitmap.data; // RGBA

      const out = Buffer.alloc(width * height * 2);
      let ri = 0, wi = 0;
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const r = raw[ri++];
          const g = raw[ri++];
          const b = raw[ri++];
          ri++; // skip alpha
          const r5 = (r >> 3) & 0x1f;
          const g6 = (g >> 2) & 0x3f;
          const b5 = (b >> 3) & 0x1f;
          const v = (r5 << 11) | (g6 << 5) | b5;
          out[wi++] = v & 0xff;
          out[wi++] = (v >> 8) & 0xff;
        }
      }
      return { width, height, data: out };
    } catch (e2) {
      console.error('bmpToRgb565: sharp failed with:', err.message);
      console.error('bmpToRgb565: jimp fallback failed with:', e2 && e2.message ? e2.message : e2);
      console.error('Install jimp with: npm install jimp');
      throw new Error('Input buffer contains unsupported image format');
    }
  }
}

function u64ToBeBuffer(n) {
  const buf = Buffer.alloc(8);
  // n is JS number may be > 2^32; use BigInt
  const bn = BigInt(n);
  for (let i = 7; i >= 0; i--) {
    buf[i] = Number(bn & 0xffn);
    bn >>= 8n;
  }
  return buf;
}

async function run() {
  const portName = argv.port;
  const filePath = argv.file;
  const x = parseInt(argv.x, 10) || 0;
  const y = parseInt(argv.y, 10) || 0;

  if (!fs.existsSync(filePath)) {
    console.error('File not found:', filePath);
    process.exit(1);
  }

  const buf = fs.readFileSync(filePath);
  const img = await bmpToRgb565(buf);
  const width = img.width;
  const height = img.height;
  const rgb565 = img.data;

  // lz4 compress with size prepended (4-byte little-endian)
  const maxCompressed = lz4.encodeBound(rgb565.length);
  const compressed = Buffer.allocUnsafe(maxCompressed);
  const compressedSize = lz4.encodeBlock(rgb565, compressed);
  const compressedBuf = compressed.slice(0, compressedSize);

  // prepend little-endian size (u32)
  const sizeBuf = Buffer.alloc(4);
  sizeBuf.writeUInt32LE(rgb565.length, 0);
  const payload = Buffer.concat([sizeBuf, compressedBuf]);

  // IMAGE_AA and IMAGE_BB as in usb_reader.rs
  const IMAGE_AA = 7596835243154170209n; // matches Rust constant
  const IMAGE_BB = 7596835243154170466n;
  const aa = u64ToBeBuffer(IMAGE_AA);
  const bb = u64ToBeBuffer(IMAGE_BB);

  // header: aa (8) + width BE (2) + height BE (2) + x BE (2) + y BE (2)
  const header = Buffer.alloc(16);
  aa.copy(header, 0);
  header.writeUInt16BE(width & 0xffff, 8);
  header.writeUInt16BE(height & 0xffff, 10);
  header.writeUInt16BE(x & 0xffff, 12);
  header.writeUInt16BE(y & 0xffff, 14);

  const toSend = Buffer.concat([header, payload, bb]);

  console.log(`Prepared image ${filePath}: ${width}x${height}, rgb565 ${rgb565.length} bytes, compressed ${payload.length} bytes`);

  const port = new SerialPort({ path: portName, baudRate: 115200, autoOpen: false });
  port.open((err) => {
    if (err) {
      console.error('Failed to open port:', err.message);
      process.exit(1);
    }
    console.log('Port opened, sending image...');
    // send in chunks to avoid overwhelming USB stack
    const chunk = 4096;
    let offset = 0;
    function writeNext() {
      if (offset >= toSend.length) {
        console.log('Image sent, closing port.');
        setTimeout(() => port.close(), 200);
        return;
      }
      const end = Math.min(offset + chunk, toSend.length);
      const slice = toSend.slice(offset, end);
      port.write(slice, (e) => {
        if (e) console.error('Write error:', e.message);
        offset = end;
        // use drain occasionally
        if (offset % (chunk * 8) === 0) {
          port.drain(() => setImmediate(writeNext));
        } else {
          setImmediate(writeNext);
        }
      });
    }
    writeNext();
  });
}

run().catch((e) => { console.error('Fatal:', e); process.exit(1); });
