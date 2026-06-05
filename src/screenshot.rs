use base64::{engine::general_purpose::STANDARD, Engine};
use jpeg_encoder::{Encoder, ColorType};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// JPEG 编码质量: 92 兼顾文字清晰度与文件体积
const JPEG_QUALITY: u8 = 92;

/// 缩小倍数: 1 = 不缩小 (原生像素,与内置版一致), 2 = 2x box filter
const DOWNSCALE: i32 = 1;

/// 初始化 DPI 感知 (Per-Monitor V2),应在程序启动时调用一次
pub fn init_dpi_awareness() {
    unsafe {
        let result = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if result.is_err() {
            log_diag("DPI 感知设置失败,回退到系统默认");
        }
        let dpi = GetDpiForSystem();
        log_diag(&format!("系统 DPI: {} (缩放 {}%)", dpi, dpi * 100 / 96));
    }
}

/// 写入诊断日志到 server.log
fn log_diag(msg: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("D:\\project\\demo\\omkz\\server.log")
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "[DIAG] {}", msg)
        });
}

/// 获取截图缩小倍数,供坐标转换使用
pub fn get_downscale() -> i32 {
    DOWNSCALE
}

/// 全屏截取并编码 JPEG,返回 (base64, 输出宽度, 输出高度)
pub fn capture_screen() -> Result<(String, u32, u32)> {
    unsafe {
        let old_ctx = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            if !old_ctx.is_invalid() {
                let _ = SetThreadDpiAwarenessContext(old_ctx);
            }
            return Err(E_FAIL.into());
        }

        let screen_w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let screen_x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let screen_y = GetSystemMetrics(SM_YVIRTUALSCREEN);

        let out_w = screen_w / DOWNSCALE;
        let out_h = screen_h / DOWNSCALE;

        log_diag(&format!(
            "全屏捕获: {}x{} 原点: ({},{}) | /{} -> JPEG {}x{} q{}",
            screen_w, screen_h, screen_x, screen_y,
            DOWNSCALE, out_w, out_h, JPEG_QUALITY,
        ));

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbitmap = CreateCompatibleBitmap(hdc_screen, screen_w, screen_h);
        let h_old = SelectObject(hdc_mem, hbitmap.into());

        let result = BitBlt(
            hdc_mem, 0, 0, screen_w, screen_h,
            Some(hdc_screen), screen_x, screen_y, SRCCOPY,
        );

        if result.is_err() {
            SelectObject(hdc_mem, h_old);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(None, hdc_screen);
            if !old_ctx.is_invalid() {
                let _ = SetThreadDpiAwarenessContext(old_ctx);
            }
            return Err(E_FAIL.into());
        }

        // 提取原始 BGRA 像素
        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: screen_w,
            biHeight: -screen_h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };

        let mut pixels = vec![0u8; (screen_w * screen_h * 4) as usize];
        let got = GetDIBits(
            hdc_mem, hbitmap, 0, screen_h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi as *mut BITMAPINFOHEADER as *mut BITMAPINFO,
            DIB_RGB_COLORS,
        );

        // 释放 GDI 资源
        SelectObject(hdc_mem, h_old);
        let _ = DeleteObject(hbitmap.into());
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        if got == 0 {
            if !old_ctx.is_invalid() {
                let _ = SetThreadDpiAwarenessContext(old_ctx);
            }
            return Err(E_FAIL.into());
        }

        if !old_ctx.is_invalid() {
            let _ = SetThreadDpiAwarenessContext(old_ctx);
        }

        // 编码 JPEG: jpeg-encoder 支持 Bgra 输入,自动丢弃 alpha 通道
        let jpeg_data = if DOWNSCALE == 1 {
            encode_jpeg(&pixels, out_w as usize, out_h as usize)?
        } else {
            // DOWNSCALE>1 时先 box filter 缩小再编码
            let downscaled = box_filter_bgra(&pixels, screen_w as usize, screen_h as usize, DOWNSCALE as usize);
            encode_jpeg(&downscaled, out_w as usize, out_h as usize)?
        };
        let b64 = STANDARD.encode(&jpeg_data);

        log_diag(&format!(
            "JPEG 编码完成: {} 字节, base64: {} 字节",
            jpeg_data.len(), b64.len()
        ));

        Ok((b64, out_w as u32, out_h as u32))
    }
}

/// 从 BGRA 缓冲编码 JPEG (jpeg-encoder 内部处理 BGRA->YCbCr 转换)
fn encode_jpeg(pixels: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let encoder = Encoder::new(&mut buf, JPEG_QUALITY);
    encoder
        .encode(pixels, width as u16, height as u16, ColorType::Bgra)
        .map_err(|_| Error::from(E_FAIL))?;
    Ok(buf)
}

/// BGRA box filter 缩小: 每 ds*ds 个像素取均值,保留 alpha 通道
fn box_filter_bgra(pixels: &[u8], width: usize, height: usize, ds: usize) -> Vec<u8> {
    let out_w = width / ds;
    let out_h = height / ds;
    let mut result = vec![0u8; out_w * out_h * 4];

    for oy in 0..out_h {
        for ox in 0..out_w {
            let mut b_sum: u32 = 0;
            let mut g_sum: u32 = 0;
            let mut r_sum: u32 = 0;
            let mut a_sum: u32 = 0;
            for dy in 0..ds {
                for dx in 0..ds {
                    let sx = ox * ds + dx;
                    let sy = oy * ds + dy;
                    let src = (sy * width + sx) * 4;
                    b_sum += pixels[src] as u32;
                    g_sum += pixels[src + 1] as u32;
                    r_sum += pixels[src + 2] as u32;
                    a_sum += pixels[src + 3] as u32;
                }
            }
            let count = (ds * ds) as u32;
            let dst = (oy * out_w + ox) * 4;
            result[dst] = (b_sum / count) as u8;
            result[dst + 1] = (g_sum / count) as u8;
            result[dst + 2] = (r_sum / count) as u8;
            result[dst + 3] = (a_sum / count) as u8;
        }
    }
    result
}
