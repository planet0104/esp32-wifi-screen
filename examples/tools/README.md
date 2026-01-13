send_image.js
=================

This script converts a 24-bit BMP into RGB565, compresses it with LZ4 (size prepended),
and sends it to the ESP32 using the binary image protocol supported by the firmware
(`IMAGE_AA` header, width/height/x/y, compressed payload, `IMAGE_BB` trailer).

Dependencies (install in this folder or globally):

- sharp (preferred) or jimp (fallback for some BMP variants)
- serialport
- lz4
- minimist

If `sharp` fails to parse your BMP file, the script will try `jimp`. To preinstall the fallback:

```powershell
npm install jimp
```

Install example:

```powershell
cd tools
npm install sharp serialport lz4 minimist
```

Run:

```powershell
node send_image.js --port COM6 --file rgb24.bmp --x 0 --y 0
```

Defaults: `--file` defaults to `tools/rgb24.bmp` and `x,y` default to 0.
