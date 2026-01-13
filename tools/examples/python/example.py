# 安装: pip install aiohttp

import asyncio
import aiohttp
import json

async def sleep(ms):
    await asyncio.sleep(ms / 1000)

url = "http://192.168.96.226/"

print('[*****本代码使用DeepSeek根据nodejs代码生成*****');
print('wifi 屏幕地址:' + url)
print('完整示例请参考' + url + 'example')

async def start():
    print('获取屏幕配置...')
    async with aiohttp.ClientSession() as session:
        # 获取屏幕参数
        async with session.get(url + 'display_config') as response:
            config = await response.json()

        width = config['rotated_width']
        height = config['rotated_height']

        print('绘制文本...')

        # FilledRect 填充黑色背景
        # Text 绘制白色文字
        json_data = [
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
            }
        ]

        async with session.post(url + 'draw_canvas', json=json_data) as response:
            result = await response.text()

        await sleep(2000)

        print('绘制形状...')

        json_data = [
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
        ]

        async with session.post(url + 'draw_canvas', json=json_data) as response:
            result = await response.text()

async def main():
    await start()

# 运行异步主函数
asyncio.run(main())