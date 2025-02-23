use anyhow::Result;
use serde_json::json;

fn main() -> Result<()> {
    const URL: &str = "http://192.168.96.226/";

    println!("上传预设图片...");
    let ret = ureq::post(format!("{URL}upload_image?key=1"))
        .send(include_bytes!("smile.png"))?
        .body_mut()
        .read_to_string();
    println!("图片上传完成:{ret:?}");

    let elements = json!([
        {
            "Rectangle": {
                "left": 0,
                "top": 0,
                "width": 240,
                "height": 240,
                "fill_color": "black",
                "stroke_width": 0,
            }
        },
        {
            "Image": {
                "x": 0,
                "y": 120,
                "key": "1"
            }
        },
        {
            "Image": {
                "x": 0,
                "y": 50,
                "base64": "R0lGODlhOwBNAPMIAAAAAB9eOX9MOTGvZcyBUv3ZXlxegaytv////wAAAAAAAAAAAAAAAAAAAAAAAAAAACH5BAEAAAkALAAAAAA7AE0AAAT+MMlJq00g58u7/xxQjOMGnugHECRppnC6EjTbvnEe1nxb4LogZkbTGI9GoQXJ1ASeg6h0KtUoMTbfiJB5QqlgMBBG1Ha94XR4fCLSXN6Aer7WuYtxup4KsPMAeXuCUX05M4Bog4qFMWeJioOMKE6PkJGTIoiVU0iWhAeSIXCBfAcIp6cAngAIoUs3pJymqKiqi7OuEyJbXLGftLSgi7UdbppfnMDAtoKsqcUCNcdyfMq0zM2muRIAAt4C02Kz1tjNrR7d3uHi5J4DbBQZ4HHU7KjC7u9t62tH+YST6P1bFNDXwDnbKjhCdhBhGzwG05SroidhPIibJOKTNZGTRW7+fyKWSiWLJB8rIIjME9mkZSeAKb+pE7kG18hWVSbJnJlRDYCNHHPKYEJTjE+hV441hKnESNFLQjTQALfQXQB4Ov90ocRwzhOsMmQu/Oqy7BVu6viFiUPk588DoMDuU4uQRBFQy+SiQ1OvYgsuoMbV0ruELx+fPu66NfKMjEBOffloAZxBGaihSg9L1FKgyM/BKNFl6GW4SmROnDsjifsRgzeMXTtWSc1DMZc2PDPKJkS7NpddrdNl1rwmNa8/rX7g1rTm9GzjiTWUaNPznXPe0P9ySd4aQ/Xd77JH5069K0XE4u1KV36BESKE6NOrTs5e4W3v18+Lkb9lfa5D+EnTpMFm/FE23RLR/JaBRPldxV8JKxyo0Gu/XUXIgu94FeGDu7CHknCacDWghg925gI3ZHFFz4r5RXFVFhzqwuKMj9ERIYzG2XDfEDT22AwLOHIGJCDuTcPicBXVlqM0V+myEo1I2ujblCE16eRYXtCFEJVUhjihgvRo6ROXtVFFJBDdSHNEjRUlSGVaBhgQ2GUK3QGbeRLtpKdLxdwRTosuprNnEoasyaZPLZ2lEIt0XNXdWUbiGaiiMtBIhaOUVurUpoRmOomccL2VCi6eNlLWoyhEAAA7",
            }
        },
        {
            "Text": {
                "x": 70,
                "y": 15,
                "text": "Hello!你好世界！",
                "size": 20,
                "color": "white"
            }
        },
        {
            "Rectangle" :{
                "left": 10,
                "top": 70,
                "width": 40,
                "height": 40,
                "fill_color": "yellow",
                "stroke_color": "blue",
                "stroke_width": 6,
            }
        },
        {
            "RoundedRectangle" :{
                "left": 30,
                "top": 70,
                "width": 30,
                "height": 30,
                "fill_color": "red",
                "stroke_color": "blue",
                "stroke_width": 3,
                "top_left_corner": [15, 15],
                "top_right_corner": [15, 15],
                "bottom_right_corner": [15, 15],
                "bottom_left_corner": [15, 15],
            }
        },
        {
            "Line" : {
                "start": [70, 70],
                "end": [100, 100],
                "color": "white",
                "stroke_width": 5,
            }
        },
        {
            "Circle" : {
                "top_left": [110, 65],
                "diameter": 20,
                "stroke_width": 2,
                "fill_color": "yellow",
                "stroke_color": "blue",
            }
        },
        {
            "Ellipse" : {
                "top_left": [70, 125],
                "size": [50, 30],
                "stroke_width": 4,
                "fill_color": "yellow",
                "stroke_color": "#00f",
            }
        },
        {
            "Polyline": {
                "points": [[160, 120], [216, 120], [182, 155]],
                "color": "#ff0",
                "stroke_width": 4,
            }
        },
        {
            "Triangle" : {
                "vertex1": [40, 10],
                "vertex2": [80, 60],
                "vertex3": [10, 60],
                "stroke_width": 4,
                "fill_color": "red",
                "stroke_color": "yellow",
            }
        },
        {
            "Arc": {
                "top_left": [110, 110],
                "diameter": 60,
                "stroke_width": 4,
                "angle_start": 0,
                "angle_sweep": 180,
                "color": "red",
            }
        },
        {
            "Sector": {
                "top_left": [150, 100],
                "diameter": 60,
                "stroke_width": 4,
                "angle_start": 0,
                "angle_sweep": 120,
                "fill_color": "red",
                "stroke_color": "blue",
            }
        }
        
    ]);

    match ureq::post(format!("{URL}draw_canvas")).send(elements.to_string().as_bytes()) {
        Ok(mut r) => {
            let ret = r.body_mut().read_to_string();
            println!("绘图完成:{ret:?}");
        }
        Err(err) => {
            println!("绘图失败:{err:?}");
        }
    }
    Ok(())
}
