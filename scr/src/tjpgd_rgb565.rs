use anyhow::{anyhow, Result};
use core::{
    ffi::{c_int, c_short, c_ulong, c_ushort, c_void},
    mem::zeroed,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut},
};
use log::error;

// 定义 JRESULT 枚举
#[allow(non_camel_case_types, unused)]
#[repr(C)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum JRESULT {
    JDR_OK = 0,
    JDR_INTR,
    JDR_INP,
    JDR_MEM1,
    JDR_MEM2,
    JDR_PAR,
    JDR_FMT1,
    JDR_FMT2,
    JDR_FMT3,
}

// 定义 JRECT 结构体
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct JRECT {
    pub left: c_ushort,
    pub right: c_ushort,
    pub top: c_ushort,
    pub bottom: c_ushort,
}

// 定义 JDEC 结构体（只列出必要的字段）
#[repr(C)]
pub struct JDEC {
    dctr: c_ulong,
    dptr: *mut u8,
    inbuf: *mut u8,
    dbit: u8,
    scale: u8,
    msx: u8,
    msy: u8,
    qtid: [u8; 3],
    ncomp: u8,
    dcv: [i16; 3],
    nrst: c_ushort,
    pub width: c_ushort,
    pub height: c_ushort,
    huffbits: [[*mut u8; 2]; 2],
    huffcode: [[*mut c_ushort; 2]; 2],
    huffdata: [[*mut u8; 2]; 2],
    pub qttbl: [*mut i32; 4],
    wreg: u32,
    marker: u8,
    workbuf: *mut c_void,
    mcubuf: *mut c_short, // 注意：原C代码中是 jd_yuv_t* 类型，这里简化为 *mut c_void
    pool: *mut c_void,
    sz_pool: c_ulong,
    infunc: Option<unsafe extern "C" fn(*mut JDEC, *mut u8, c_ulong) -> c_ulong>,
    device: *mut c_void,
    swap: u8,
}

// 外部函数声明
extern "C" {
    // 准备解码器
    fn jd_prepare(
        jd: *mut JDEC,
        infunc: Option<unsafe extern "C" fn(*mut JDEC, *mut u8, c_ulong) -> c_ulong>,
        pool: *mut c_void,
        sz_pool: c_ulong,
        dev: *mut c_void,
    ) -> JRESULT;

    // 解码图像
    fn jd_decomp(
        jd: *mut JDEC,
        outfunc: Option<unsafe extern "C" fn(*mut JDEC, *mut c_void, *const JRECT) -> c_int>,
        scale: u8,
    ) -> JRESULT;
}

// static mut JPEG_DATA:Option<(*const u8, usize)> = None;
// static mut ARRAY_INDEX: u32 = 0;
// static mut OUTPUT_RGB565: Option<Vec<u16>> = None;
// static mut WORKSPACE: Option<Vec<u8>> = None;

/*
使用jpeg-encoder库压缩可正常解析
jpeg-encoder = "0.6.0"

let img = image::open("girl.png").unwrap().to_rgb8();

let mut output_file = File::create("girl.jpg").unwrap();
let mut out = Vec::new();
let encoder = Encoder::new(&mut out, 50);
encoder.encode(&img, img.width() as u16, img.height() as u16, ColorType::Rgb).unwrap();
output_file.write_all(&out).unwrap();
 */

struct JpegDecoder {
    pub jdec: JDEC,
    pub jpeg_data: Vec<u8>,
    pub input_data_index: usize,
    pub output: Option<Box<Vec<u16>>>,
    pub workspace: Box<Vec<u8>>,
}

pub fn decode_jpg(jpeg_data: Vec<u8>) -> Result<(u32, u16, u16, Box<Vec<u16>>)> {
    let mut decoder = Box::new(JpegDecoder {
        workspace: Box::new(vec![0u8; 3500 + 6144]),
        jpeg_data,
        input_data_index: 0,
        output: None,
        jdec: unsafe { zeroed() },
    });

    let decoder_ptr = decoder.as_mut() as *mut _ as *mut c_void;

    unsafe extern "C" fn jd_input(jdec: *mut JDEC, buf: *mut u8, mut len: u32) -> u32 {
        let jdec = &*jdec;
        let deocoder = &mut *(jdec.device as *mut JpegDecoder);
        // let (ptr, data_len) = JPEG_DATA.clone().unwrap();
        // let data = &*slice_from_raw_parts(ptr, data_len);
        if deocoder.input_data_index + len as usize > deocoder.jpeg_data.len() {
            len = deocoder.jpeg_data.len() as u32 - deocoder.input_data_index as u32;
        }
        if !buf.is_null() {
            let buf_slice = &mut *slice_from_raw_parts_mut(buf, len as usize);
            buf_slice[0..len as usize].copy_from_slice(
                &deocoder.jpeg_data
                    [deocoder.input_data_index..(deocoder.input_data_index + len as usize)],
            );
        }
        deocoder.input_data_index += len as usize;
        len
    }

    unsafe extern "C" fn jd_output(
        jdec: *mut JDEC,
        bitmap: *mut c_void,
        rect: *const JRECT,
    ) -> c_int {
        let rect = &*rect;
        let jdec = &*jdec;
        let deocoder = &mut *(jdec.device as *mut JpegDecoder);
        let width = rect.right + 1 - rect.left;
        let height = rect.bottom + 1 - rect.top;
        let bitmap = &*slice_from_raw_parts(bitmap as *mut u16, (width * height) as usize);
        let output = match deocoder.output.as_mut() {
            None => {
                error!("output buffer is None!");
                return 0;
            }
            Some(o) => o,
        };
        let left = rect.left;
        let top = rect.top;
        for (id, line) in bitmap.chunks(width as usize).enumerate() {
            let start = (top as usize + id) * jdec.width as usize + left as usize;
            output[start..start + line.len()].copy_from_slice(line);
        }
        1
    }

    let (workspace_ptr, workspace_len) = (decoder.workspace.as_mut_ptr(), decoder.workspace.len());
    let jresult = unsafe {
        jd_prepare(
            &mut decoder.jdec,
            Some(jd_input),
            workspace_ptr as *mut c_void,
            workspace_len as u32,
            decoder_ptr,
        )
    };
    if jresult == JRESULT::JDR_OK {
        decoder.output.replace(Box::new(vec![
            0u16;
            decoder.jdec.width as usize
                * decoder.jdec.height as usize
        ]));

        // Extract image and render
        let jresult = unsafe { jd_decomp(&mut decoder.jdec, Some(jd_output), 0) };
        Ok((
            jresult as u32,
            decoder.jdec.width,
            decoder.jdec.height,
            decoder.output.take().unwrap()
        ))
    } else {
        Err(anyhow!("{jresult:?}"))
    }
}
