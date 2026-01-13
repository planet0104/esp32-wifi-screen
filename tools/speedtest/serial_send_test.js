#!/usr/bin/env node
// High-speed one-way send test for ESP32-S3 USB Serial/JTAG
// Usage: node serial_send_test.js --port COM6 [--seconds 10] [--chunk 4096]

const { SerialPort } = require('serialport');
const argv = require('minimist')(process.argv.slice(2), {
    string: ['port'],
    boolean: ['list', 'help'],
    default: { baud: 115200, seconds: 10, chunk: 4096 }
});

if (argv.help) {
    console.log(`
High-speed one-way send test for ESP32-S3 USB Serial/JTAG

Usage: node serial_send_test.js --port COMx [options]

Options:
  --port COMx       Serial port (required)
  --baud N          Baud rate (default: 115200, ignored for USB)
  --seconds N       Test duration in seconds (default: 10)
  --chunk N         Chunk size in bytes (default: 4096)
  --list            List available ports
  --help            Show this help

Protocol:
  - Device sends "READY:HIGH_SPEED_RX\\n" on startup
  - Host sends raw data chunks continuously
  - Device sends "ACK:XXXXXXXX\\n" every 64KB received
  - Send "RESET\\n" to reset device counter
  - Send "STATUS\\n" to query current byte count
`);
    process.exit(0);
}

if (argv.list) {
    SerialPort.list().then(ports => {
        console.log('Available ports:');
        ports.forEach(p => console.log(`  ${p.path} - ${p.manufacturer || 'Unknown'}`));
    });
} else {
    if (!argv.port) {
        console.error('Error: --port is required. Use --list to see available ports.');
        process.exit(2);
    }
    runTest();
}

async function runTest() {
    const portName = argv.port;
    const testSeconds = parseInt(argv.seconds, 10) || 10;
    const chunkSize = parseInt(argv.chunk, 10) || 4096;
    
    console.log('High-speed one-way send test config:');
    console.log(`  port=${portName}`);
    console.log(`  seconds=${testSeconds}`);
    console.log(`  chunk=${chunkSize}`);
    
    const port = new SerialPort({ 
        path: portName, 
        baudRate: 115200,
        autoOpen: false 
    });
    
    // Generate random data chunk
    const dataChunk = Buffer.alloc(chunkSize);
    for (let i = 0; i < chunkSize; i++) {
        dataChunk[i] = Math.floor(Math.random() * 256);
    }
    
    let totalBytesSent = 0;
    let deviceAckBytes = 0;
    let deviceAckMs = 0;
    let ackCount = 0;
    let running = false;
    let startTime = 0;
    let rxBuffer = '';
    
    port.on('error', (err) => {
        if (running) {
            console.error('SerialPort error:', err.message);
        }
    });
    
    port.on('data', (data) => {
        rxBuffer += data.toString('utf8');
        
        // Parse ACK messages
        let newlineIdx;
        while ((newlineIdx = rxBuffer.indexOf('\n')) !== -1) {
            const line = rxBuffer.slice(0, newlineIdx).trim();
            rxBuffer = rxBuffer.slice(newlineIdx + 1);
            
            if (line.startsWith('READY:')) {
                console.log('Device ready:', line);
            } else if (line.startsWith('SPEEDRESULT;')) {
                // Device reports: SPEEDRESULT;<bytes>;<ms>\n
                const parts = line.split(';');
                if (parts.length >= 2) {
                    const bytes = parseInt(parts[1], 10) || 0;
                    deviceAckBytes = bytes;
                    ackCount++;
                    if (parts.length >= 3) {
                        const ms = parseInt(parts[2], 10) || 0;
                        deviceAckMs = ms;
                        // optional: log ms
                        console.log(`Device reported ${bytes} bytes in ${ms} ms`);
                    }
                }
            } else if (line.startsWith('ACK:')) {
                const hexBytes = line.slice(4);
                deviceAckBytes = parseInt(hexBytes, 16);
                ackCount++;
            } else if (line.startsWith('RESET:')) {
                console.log('Device reset:', line);
            } else if (line) {
                console.log('Device:', line);
            }
        }
    });
    
    return new Promise((resolve, reject) => {
        port.open((err) => {
            if (err) {
                console.error('Failed to open port:', err.message);
                process.exit(1);
            }
            
            console.log('Port opened, waiting for device ready...');
            
                    // Wait a bit for device ready message, then start sending
            setTimeout(() => {
                // Note: device speed test uses binary markers 'SPDTEST1' and 'SPDEND!!'.
                // Do not send textual RESET here (device does not implement it for speed tests).
                setTimeout(() => {
                    running = true;
                    startTime = Date.now();
                    // Only start high-speed send after device reports READY, or fallback after timeout
                    let deviceReady = false;
                    const readyTimeoutMs = 3000;
                    const readyStart = Date.now();

                    const waitForReady = (cb) => {
                        if (deviceReady) return cb();
                        if ((Date.now() - readyStart) > readyTimeoutMs) {
                            console.log(`[init] READY not seen within ${readyTimeoutMs}ms, proceeding anyway.`);
                            return cb();
                        }
                        setTimeout(() => waitForReady(cb), 100);
                    };
                    
                    // Progress reporting
                    const progressInterval = setInterval(() => {
                        const elapsed = (Date.now() - startTime) / 1000;
                        const txRate = (totalBytesSent / 1024) / elapsed;
                        const ackRate = (deviceAckBytes / 1024) / elapsed;
                        console.log(`Progress: t=${elapsed.toFixed(1)}s sent=${(totalBytesSent/1024).toFixed(0)}KB ` +
                            `(${txRate.toFixed(1)} KB/s) deviceAck=${(deviceAckBytes/1024).toFixed(0)}KB ackCount=${ackCount}`);
                    }, 1000);
                    
                    // Continuous send loop with backpressure via drain
                    let pendingWrites = 0;
                    const maxPending = 8; // lower concurrency to avoid overwhelming device
                    let drainScheduled = false;

                    function sendMore() {
                        if (!running) return;

                        while (pendingWrites < maxPending && running) {
                            pendingWrites++;
                            port.write(dataChunk, (err) => {
                                pendingWrites--;
                                if (err) {
                                    if (running) {
                                        console.error('Write error:', err.message);
                                    }
                                } else {
                                    totalBytesSent += chunkSize;
                                }
                                // If we dropped below threshold, schedule more sends
                                if (running && pendingWrites < maxPending) {
                                    setImmediate(sendMore);
                                }
                            });
                        }

                        // If we've hit the pending limit, wait for underlying OS buffers to flush
                        if (running && pendingWrites >= maxPending && !drainScheduled) {
                            drainScheduled = true;
                            port.drain((err) => {
                                drainScheduled = false;
                                if (err && running) console.error('Drain error:', err.message);
                                if (running) setImmediate(sendMore);
                            });
                        }
                    }

                    // Send SPDTEST1 header then start sending immediately
                    const hdr = Buffer.from('SPDTEST1');
                    port.write(hdr, (err) => {
                        if (err) console.error('Failed to send SPDTEST1 header:', err.message);
                        sendMore();
                    });
                    
                        // Stop after test duration
                    setTimeout(() => {
                        running = false;
                        clearInterval(progressInterval);

                        // Send SPDEND!! marker so device can report results
                        const endMarker = Buffer.from('SPDEND!!');
                        port.write(endMarker, (err) => {
                            if (err) console.error('Failed to send SPDEND!! marker:', err.message);
                            // Wait for device to emit SPEEDRESULT messages (device sends them twice)
                            setTimeout(() => {
                                const elapsed = (Date.now() - startTime) / 1000;
                                const txRate = (totalBytesSent / 1024) / elapsed;
                                // Prefer device-reported ms for device RX rate if available
                                const deviceSeconds = deviceAckMs > 0 ? (deviceAckMs / 1000) : elapsed;
                                const ackRate = (deviceAckBytes / 1024) / deviceSeconds;

                                console.log('\n=== Test Result ===');
                                console.log(`Duration: ${elapsed.toFixed(2)}s`);
                                console.log(`Bytes sent: ${totalBytesSent} (${(totalBytesSent/1024/1024).toFixed(2)} MB)`);
                                console.log(`TX throughput: ${txRate.toFixed(2)} KB/s`);
                                console.log(`Device ACK bytes: ${deviceAckBytes} (${(deviceAckBytes/1024/1024).toFixed(2)} MB)`);
                                console.log(`Device RX rate: ${ackRate.toFixed(2)} KB/s`);
                                console.log(`ACK count: ${ackCount}`);
                                let dataLossPct = 0;
                                if (totalBytesSent > 0) {
                                    dataLossPct = ((totalBytesSent - deviceAckBytes) / totalBytesSent * 100);
                                }
                                if (dataLossPct < 0) {
                                    console.log('Note: device reported more bytes than host sent. Showing 0% data loss.');
                                    dataLossPct = 0;
                                }
                                console.log(`Data loss: ${dataLossPct.toFixed(2)}%`);

                                port.close(() => {
                                    resolve();
                                });
                            }, 600);
                        });
                    }, testSeconds * 1000);
                    
                }, 500);
            }, 1000);
        });
    });
}
