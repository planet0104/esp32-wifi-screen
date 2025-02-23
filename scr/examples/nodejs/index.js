//Node.js 17.5.0 或更高版本，可以使用原生的 fetch API，

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

const url = "http://192.168.96.226/";

console.log('wifi 屏幕地址:'+url);
console.warn('完整示例请查看'+url+'example');

async function start() {
    console.log('获取屏幕配置...');
    // 获取屏幕参数
    const response = await fetch(url+'display_config');
    let config = JSON.parse(await response.text());

    const width = config.rotated_width;
    const height = config.rotated_height;

    console.log('绘制文本...');

    // FilledRect 填充黑色背景
    // Text 绘制白色文字
    var jsonData = [
        {
            "Rectangle": {
                "fill_color": "black",
                "height": height,
                "width": width,
                "stroke_width": 0,
                "left": 0,
                "top": 0
            }
        },
        {
            "Text": {
                "color": "white",
                "size": 20,
                "text": "Hello!你好世界！",
                "x": 10,
                "y": 15
        }
    }];

    let result = await (await fetch(url+'draw_canvas', {
        method: 'POST',
        body: JSON.stringify(jsonData)
    })).text();

    await sleep(2000);

    console.log('绘制形状...');

    var jsonData = [
        {
            "Rectangle": {
                "fill_color": "black",
                "stroke_width": 0,
                "height": height,
                "width": width,
                "left": 0,
                "top": 0
            }
        },
        {
            "Rectangle": {
                "fill_color": "red",
                "stroke_color": "blue",
                "stroke_width": 6,
                "left": 10,
                "top": 20,
                "width": 60,
                "height": 60,
            }
        },
        {
            "Rectangle": {
                "stroke_color": "blue",
                "stroke_width": 6,
                "left": 80,
                "top": 20,
                "width": 60,
                "height": 60,
            }
        },
        {
            "Rectangle": {
                "fill_color": "red",
                "stroke_width": 0,
                "left": 10,
                "top": 90,
                "width": 60,
                "height": 60,
            }
        }
    ];

    result = await (await fetch(url+'draw_canvas', {
        method: 'POST',
        body: JSON.stringify(jsonData)
    })).text();
    
    await sleep(2000);

    console.log('绘制图像(jpg/gif/png)...');

    var jsonData = [
        {
            "Image": {
                "base64": "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01cQpzzjWmmriApSi6Nl2GazWqawQRwSZwMo1HVd6hNoLzimmgrgKUovbZFhmtFpnMEGcFGUTONZ13esTac841ppq4gFBecU00FcAoLzimmgrgFOeca001cQpzzjWmmriApURtQJljXOs8Zs04wmQjCFzrE2gvOKaaCuApk+NycDXPVONtNnROMJkYRulX9N5iZTnnGtNNXEAoLzimmgrgFBecU00FcApzzjWmmriFOeca001cQFKTnRsizTma0zmCyKKTKJnCoqrvUJtBecU00FcBSk57bLs05otU1gsyiozgZQrKu71ibTnnGtNNXEAoLzimmgrgFBecU00FcApzzjWmmriFOeca001cQGlzc26W7NSmayIlpMzNKv5F1A+ObdTdopLNZka1GRklX8j6gc3xupuzSposyNaSMjUr+RdYPj43S3aJS0WREtRERKV/I+sBmoLzimmgrgFBecU00FcApzzjWmmriFOeca001cQCgvOKaaCuApSi6Nl2GazWqawQRwSZwMo1HVd6hNpzzjWmmriKUovbZFhmtFpnMEGcFGUTONZ13esBNoLzimmgrgFBecU00FcApzzjWmmriFOeca001cQCgvOKaaCuApURtQJljXOs8Zs04wmQjCFzrE2nPONaaauIpUttQJ9kXOs8J044wmRhGNzqATaC84ppoK4BQXnFNNBXAKc841ppq4hTnnGtNNXEAoLzimmgrgKUnOjZFmnM1pnMFkUUmUTOFRVXeoTac841ppq4ilJz22XZpzRaprBZlFRnAyhWVd3rAf/9CC/Xy1yi9oxmGl+vlrlF7RjMAAAAKcqYDN2e8TBTlTAZuz3iYAAAAKfN3ePLEwU+bu8eWJgAAAApyXh83abhMFOS8Pm7TcJgAAAA0uN8ssojaIH6+WuUXtGDjfLLKI2iB+vlrlF7RgMwAAAKcqYDN2e8TBTlTAZuz3gJgAAAKfN3ePLEwU+bu8eWAmAAAApyXh83abhMFOS8Pm7TcA/9GC/Xy1yi9oxmFt7eXRLZoSnecolqiqyqKJxOuEKojPS3Po2tXwATAFOlufRtavgFLc+ja1fAAlTAZuz3iYOif3h2TYp7CfFig0+0UU1NcE1XYfW6YwUtz6NrV8AEwBTpbn0bWr4BS3Po2tXwAObu8eWJg6KkO1CnWDkWaEyyK+KZ8U67cqhc+YwUtz6NrV8AEwBTpbn0bWr4BS3Po2tXwAJLw+btNwmDonB4dlWWYwmQYrNXtFHOTVFNdyP1ukMFLc+ja1fABMAU6W59G1q+AUtz6NrV8AGVxvlllEbRA/Xy1yi9oxSdHl0U2Zkl3mqNaYKsqjgcSrhCuAPby6JbNCU7zlEtUVWVRROJ1whVEBEAU6W59G1q+AUtz6NrV8AEwU5UwGbs94Utz6NrV8Bvf3h2TYp7CfFig0+0UU1NcE1XYfW6YDnQFOlufRtavgFLc+ja1fABMFPm7vHlhS3Po2tXwG+kO1CnWDkWaEyyK+KZ8U67cqhc+YDnQFOlufRtavgFLc+ja1fABMFOS8Pm7TcFLc+ja1fAb3B4dlWWYwmQYrNXtFHOTVFNdyP1ukA//Sgv18tcovaMZhpfr5a5Re0YzAAAACnKmAzdnvEwU5UwGbs94mAAAACnzd3jyxMFPm7vHliYAAAAKcl4fN2m4TBTkvD5u03CYAAAANLjfLLKI2iB+vlrlF7Rg43yyyiNogfr5a5Re0YDMAAACnKmAzdnvEwU5UwGbs94CYAAACnzd3jyxMFPm7vHlgJgAAAKcl4fN2m4TBTkvD5u03AP/Tgv18tcovaMZhbe3Z0U2aGp4mqNaopsSjgcTqjGuAz0Rz6Tql8QEwBTojn0nVL4hRHPpOqXxAJUwGbs94mDon93dlWKe3mQYoJPs1HOTXBVVyP0ukMFEc+k6pfEBMAU6I59J1S+IURz6Tql8QDm7vHliYOio7tQptn5FmjPsavimfDNu3K43PkMFEc+k6pfEBMAU6I59J1S+IURz6Tql8QCS8Pm7TcJg6Jwd3ZNlmN58WKyV7NRTU1RVXdh9LpjBRHPpOqXxATAFOiOfSdUviFEc+k6pfEBlcb5ZZRG0QP18tcovaMUnR2dEtmZpeJyiWmCbEoonEqoxqiD27OimzQ1PE1RrVFNiUcDidUY1wARAFOiOfSdUviFEc+k6pfEBMFOVMBm7PeFEc+k6pfEb393dlWKe3mQYoJPs1HOTXBVVyP0ukA50BTojn0nVL4hRHPpOqXxATBT5u7x5YURz6Tql8Rvo7tQptn5FmjPsavimfDNu3K43PkA50BTojn0nVL4hRHPpOqXxATBTkvD5u03BRHPpOqXxG9wd3ZNlmN58WKyV7NRTU1RVXdh9LpgP/1IL9fLXKL2jGYaX6+WuUXtGMwAAAApypgM3Z7xMFOVMBm7PeJgAAAAp83d48sTBT5u7x5YmAAAACnJeHzdpuEwU5Lw+btNwmAAAADS43yyyiNogfr5a5Re0YON8ssojaIH6+WuUXtGAzAAAApypgM3Z7xMFOVMBm7PeAmAAAAp83d48sTBT5u7x5YCYAAACnJeHzdpuEwU5Lw+btNwD/1YL9fLXKL2jGYW3uSHto2aLSiKVLUZcpNwzP+wz2lfMX+SP2ATAFO0r5i/yR+wWlfMX+SP2AJUwGbs94mDon+S3ltYpiIzGKEK5SalFGJVn6DBaV8xf5I/YBMAU7SvmL/JH7BaV8xf5I/YA5u7x5YmDoykp5NyNlNgsms+bEqymzbsYf+mVz/UZ9pXzF/kj9gEwBTtK+Yv8AJH7BaV8xf5I/YAkvD5u03CYOicJLeWNlnohPYrQnlJrUcIFUfoMFpXzF/kj9gEwBTtK+Yv8AJH7BaV8xf5I/YBlcb5ZZRG0QP18tcovaMUnSSHtm2ZrUiCUrSZ8pNwjL+wPckPbRs0WlEUqWoy5Sbhmf9gEQBTtK+Yv8kfsFpXzF/kj9gEwU5UwGbs94WlfMX+SP2G9/kt5bWKYiMxihCuUmpRRiVZ+gDnQFO0r5i/yR+wWlfMX+SP2ATBT5u7x5YWlfMX+SP2G+1bzQrDM5dmnwnJ+GZCMYwu/9gOdAU7SvmL/JH7BaV8xf5I/YBMFOS8Pm7TcFpXzF/kj9hvcJLeWNlnohPYrQnlJrUcIFUfoA/9k=",
                "x": 0,
                "y": 0
            }
        },
        {
            "Image": {
                "base64": "R0lGODlhOwBNAPMIAAAAAB9eOX9MOTGvZcyBUv3ZXlxegaytv////wAAAAAAAAAAAAAAAAAAAAAAAAAAACH5BAEAAAkALAAAAAA7AE0AAAT+MMlJq00g58u7/xxQjOMGnugHECRppnC6EjTbvnEe1nxb4LogZkbTGI9GoQXJ1ASeg6h0KtUoMTbfiJB5QqlgMBBG1Ha94XR4fCLSXN6Aer7WuYtxup4KsPMAeXuCUX05M4Bog4qFMWeJioOMKE6PkJGTIoiVU0iWhAeSIXCBfAcIp6cAngAIoUs3pJymqKiqi7OuEyJbXLGftLSgi7UdbppfnMDAtoKsqcUCNcdyfMq0zM2muRIAAt4C02Kz1tjNrR7d3uHi5J4DbBQZ4HHU7KjC7u9t62tH+YST6P1bFNDXwDnbKjhCdhBhGzwG05SroidhPIibJOKTNZGTRW7+fyKWSiWLJB8rIIjME9mkZSeAKb+pE7kG18hWVSbJnJlRDYCNHHPKYEJTjE+hV441hKnESNFLQjTQALfQXQB4Ov90ocRwzhOsMmQu/Oqy7BVu6viFiUPk588DoMDuU4uQRBFQy+SiQ1OvYgsuoMbV0ruELx+fPu66NfKMjEBOffloAZxBGaihSg9L1FKgyM/BKNFl6GW4SmROnDsjifsRgzeMXTtWSc1DMZc2PDPKJkS7NpddrdNl1rwmNa8/rX7g1rTm9GzjiTWUaNPznXPe0P9ySd4aQ/Xd77JH5069K0XE4u1KV36BESKE6NOrTs5e4W3v18+Lkb9lfa5D+EnTpMFm/FE23RLR/JaBRPldxV8JKxyo0Gu/XUXIgu94FeGDu7CHknCacDWghg925gI3ZHFFz4r5RXFVFhzqwuKMj9ERIYzG2XDfEDT22AwLOHIGJCDuTcPicBXVlqM0V+myEo1I2ujblCE16eRYXtCFEJVUhjihgvRo6ROXtVFFJBDdSHNEjRUlSGVaBhgQ2GUK3QGbeRLtpKdLxdwRTosuprNnEoasyaZPLZ2lEIt0XNXdWUbiGaiiMtBIhaOUVurUpoRmOomccL2VCi6eNlLWoyhEAAA7",
                "x": 20,
                "y": 20
            }
        }
    ];

    result = await (await fetch(url+'draw_canvas', {
        method: 'POST',
        body: JSON.stringify(jsonData)
    })).text();
}

start();