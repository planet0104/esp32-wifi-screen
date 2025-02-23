//Node.js 17.5.0 或更高版本，可以使用原生的 fetch API，

const WebSocket = require('ws');

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

function formatDate(date) {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return year+'.'+month+'.'+day;
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

    // 计算时钟大小
    var diameter = Math.min(width, height);
    var clock_top = parseInt((height - diameter)/2);
    var clock_left = parseInt((width - diameter)/2);

    // 创建 WebSocket 实例
    let wsUrl = url.replace('http', 'ws')+'ws';
    console.log('websocket url:'+wsUrl);
    const socket = new WebSocket(wsUrl);
    socket.onopen = async function(event) {
        //绘制时钟
        while(true){
            var elements = [];
            //黑色背景
            elements.push({
                "Rectangle": {
                    "fill_color": "black",
                    "stroke_width": 0,
                    "height": height,
                    "width": width,
                    "left": 0,
                    "top": 0
                }
            });

            //绘制表盘
            elements.push({
                "Circle": {
                    "top_left": [clock_left, clock_top],
                    "diameter": diameter,
                    "stroke_width": 0,
                    // "fill_color": '#fec435'
                    "fill_color": '#ebff62'
                }
            });

            const now = new Date();
            const hour = now.getHours() % 12;
            const minute = now.getMinutes();
            const second = now.getSeconds();
            const cx = width/2;
            const cy = height/2;
            const r = diameter/2;

            const c1 = 16;
            elements.push({
                "Circle": {
                    "top_left": [cx-c1/2, cy-c1/2],
                    "diameter": c1,
                    "stroke_width": 0,
                    "fill_color": 'black'
                }
            });

            // 画数字
            var textAngles = [
                0,              30*Math.PI/180, 60*Math.PI/180, 90*Math.PI/180,
                120*Math.PI/180, 150*Math.PI/180, 180*Math.PI/180, 210*Math.PI/180,
                240*Math.PI/180, 270*Math.PI/180, 300*Math.PI/180, 330*Math.PI/180
            ];
            textAngles.forEach((a, id) => {
                var x = parseInt(cx + r*0.85 * Math.cos(a-Math.PI/3));
                var y = parseInt(cy + r*0.85 * Math.sin(a-Math.PI/3));
                elements.push({
                    "Text": {
                        "color": "black",
                        "size": 20,
                        "text": (id+1)+'',
                        "x": x-10,
                        "y": y-10,
                    }
                });
            });

            //日期
            var text_size = diameter/16;
            var date_str = formatDate(new Date());
            var tx = cx-text_size*3;
            var ty = parseInt(cy+diameter/4.5);
            elements.push({
                "RoundedRectangle": {
                    "fill_color": "#000080",
                    "stroke_width": 0,
                    "left": tx-10,
                    "top": ty-3,
                    "width": text_size*8,
                    "height": 20,
                    "top_left_corner": [10, 10],
                    "top_right_corner": [10, 10],
                    "bottom_right_corner": [10, 10],
                    "bottom_left_corner": [10, 10],
                    
                }
            });
            elements.push({
                "Text": {
                    "color": "white",
                    "size": text_size,
                    "text": date_str,
                    "x": tx,
                    "y": ty,
                }
            });

            // 画时针
            const hourAngle = (hour * Math.PI / 6) + (minute * Math.PI / (6 * 60)) + (second * Math.PI / (360 * 60)) - Math.PI/2;
            const hour_hand_len = r*0.7;
            const hx = parseInt(cx + hour_hand_len * Math.cos(hourAngle));
            const hy = parseInt(cy + hour_hand_len * Math.sin(hourAngle));
            const hx1 = parseInt(cx + 15 * Math.cos(hourAngle-Math.PI));
            const hy1 = parseInt(cy + 15 * Math.sin(hourAngle-Math.PI));
            elements.push({
                "Line": {
                    "color": "black",
                    "stroke_width": 7,
                    "start": [cx, cy],
                    "end": [hx, hy]
                }
            });
            elements.push({
                "Line": {
                    "color": "black",
                    "stroke_width": 7,
                    "start": [cx, cy],
                    "end": [hx1, hy1]
                }
            });

            // 画分针
            const minuteAngle = (minute * Math.PI / 30) + (second * Math.PI / (30 * 60)) - Math.PI/2;
            const minute_hand_len = r*0.8;
            const mx = parseInt(cx + minute_hand_len * Math.cos(minuteAngle));
            const my = parseInt(cy + minute_hand_len * Math.sin(minuteAngle));
            const mx1 = parseInt(cx + 15 * Math.cos(minuteAngle-Math.PI));
            const my1 = parseInt(cy + 15 * Math.sin(minuteAngle-Math.PI));
            elements.push({
                "Line": {
                    "color": "black",
                    "stroke_width": 4,
                    "start": [cx, cy],
                    "end": [mx, my]
                }
            });
            elements.push({
                "Line": {
                    "color": "black",
                    "stroke_width": 4,
                    "start": [cx, cy],
                    "end": [mx1, my1]
                }
            });

            // 画秒针
            const second_hand_len = r*0.9;
            const secondAngle = second * Math.PI / 30  - Math.PI/2;
            const sx = parseInt(cx + second_hand_len * Math.cos(secondAngle));
            const sy = parseInt(cy + second_hand_len * Math.sin(secondAngle));
            elements.push({
                "Line": {
                    "color": "black",
                    "stroke_width": 3,
                    "start": [cx, cy],
                    "end": [sx, sy]
                }
            });

            socket.send(JSON.stringify(elements));

            await sleep(1000);
        }
        socket.close();
    };

    socket.onmessage = msg =>{
        console.log(msg.data);
    };

}

start();