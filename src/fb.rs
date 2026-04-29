// Direct mxc_epdc_fb framebuffer painter for Kindle (PW2 era).
// Mmaps /dev/fb0, copies grayscale pixels, triggers MXCFB_SEND_UPDATE_V1_NTX.

#![cfg(target_os = "linux")]

use std::fs::OpenOptions;
use std::io;
use std::os::unix::io::AsRawFd;
use std::ptr;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Default)]
struct FbVarScreenInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: FbBitfield,
    green: FbBitfield,
    blue: FbBitfield,
    transp: FbBitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

#[repr(C)]
#[derive(Default)]
struct FbFixScreenInfo {
    id: [u8; 16],
    smem_start: usize,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: usize,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct MxcfbRect {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Default)]
struct MxcfbAltBufferDataV1Ntx {
    phys_addr: u32,
    width: u32,
    height: u32,
    alt_update_region: MxcfbRect,
}

// Amazon's Lab126 mxcfb_update_data with hist_*_waveform_mode fields. Size = 72.
#[repr(C)]
#[derive(Default)]
struct MxcfbUpdateData {
    update_region: MxcfbRect,
    waveform_mode: u32,
    update_mode: u32,
    update_marker: u32,
    hist_bw_waveform_mode: u32,
    hist_gray_waveform_mode: u32,
    temp: i32,
    flags: u32,
    alt_buffer_data: MxcfbAltBufferDataV1Ntx,
}

const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;
// _IOW('F', 0x2E, sizeof(MxcfbUpdateData)=72) — matches Kindle PW2 mxcfb driver
const MXCFB_SEND_UPDATE: libc::Ioctl = 0x4048_462E;

const WAVEFORM_MODE_GC16: u32 = 2;
const UPDATE_MODE_FULL: u32 = 1;
const TEMP_USE_AUTO: i32 = 0x1001;

pub fn paint_grayscale(gray: &[u8], img_w: u32, img_h: u32) -> io::Result<()> {
    let fb = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/fb0")?;
    let fd = fb.as_raw_fd();

    let mut vinfo = FbVarScreenInfo::default();
    if unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut vinfo as *mut _) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut finfo = FbFixScreenInfo::default();
    if unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut finfo as *mut _) } < 0 {
        return Err(io::Error::last_os_error());
    }

    eprintln!(
        "fb: {}x{} bpp={} rotate={} stride={} mem={}",
        vinfo.xres, vinfo.yres, vinfo.bits_per_pixel, vinfo.rotate, finfo.line_length, finfo.smem_len
    );

    let stride = finfo.line_length as usize;
    let fb_w = vinfo.xres as usize;
    let fb_h = vinfo.yres as usize;
    let fb_size = finfo.smem_len as usize;

    let map = unsafe {
        libc::mmap(
            ptr::null_mut(),
            fb_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if map == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    let buf = map as *mut u8;

    let copy_w = (img_w as usize).min(fb_w);
    let copy_h = (img_h as usize).min(fb_h);
    for iy in 0..copy_h {
        let src_row_start = iy * img_w as usize;
        let dst_row_start = iy * stride;
        unsafe {
            ptr::copy_nonoverlapping(
                gray.as_ptr().add(src_row_start),
                buf.add(dst_row_start),
                copy_w,
            );
        }
    }

    let update = MxcfbUpdateData {
        update_region: MxcfbRect {
            top: 0,
            left: 0,
            width: fb_w as u32,
            height: fb_h as u32,
        },
        waveform_mode: WAVEFORM_MODE_GC16,
        update_mode: UPDATE_MODE_FULL,
        update_marker: 1,
        hist_bw_waveform_mode: 0,
        hist_gray_waveform_mode: 0,
        temp: TEMP_USE_AUTO,
        flags: 0,
        alt_buffer_data: MxcfbAltBufferDataV1Ntx::default(),
    };

    let r = unsafe { libc::ioctl(fd, MXCFB_SEND_UPDATE, &update as *const _) };
    let result = if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    };

    unsafe {
        libc::munmap(map, fb_size);
    }
    result
}
