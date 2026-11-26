//! A minimal HTTPS client on top of WinHTTP, which uses the system proxy settings.

use std::ffi::c_void;

use windows::Win32::Foundation::GetLastError;
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

use crate::util::wide;

/// A WinHTTP handle, closed on drop.
struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn handle(raw: *mut c_void, what: &str) -> Result<Handle, String> {
    if raw.is_null() { Err(format!("{what}: error {}", unsafe { GetLastError().0 })) } else { Ok(Handle(raw)) }
}

/// Status code and body of an HTTPS request. `headers` are CRLF-terminated lines.
pub fn request(method: PCWSTR, host: PCWSTR, path: PCWSTR, headers: &str, payload: &[u8]) -> Result<(u32, Vec<u8>), String> {
    let net = |e: windows::core::Error| format!("network: {}", e.message());
    let agent = wide(&format!("claude-monitor/{}", env!("CARGO_PKG_VERSION")));
    unsafe {
        let session = handle(
            WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0),
            "WinHttpOpen",
        )?;
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 15_000, 15_000);
        let connect = handle(WinHttpConnect(session.0, host, INTERNET_DEFAULT_HTTPS_PORT, 0), "WinHttpConnect")?;
        let request = handle(
            WinHttpOpenRequest(connect.0, method, path, PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "WinHttpOpenRequest",
        )?;
        let hdr: Vec<u16> = headers.encode_utf16().collect();
        let data = (!payload.is_empty()).then_some(payload.as_ptr() as *const c_void);
        let n = payload.len() as u32;
        WinHttpSendRequest(request.0, Some(&hdr), data, n, n, 0).map_err(net)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(net)?;
        let mut status = 0u32;
        let mut len = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(net)?;
        let mut body = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read).map_err(net)?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
        }
        Ok((status, body))
    }
}

pub fn get(host: PCWSTR, path: PCWSTR, headers: &str) -> Result<(u32, Vec<u8>), String> {
    request(w!("GET"), host, path, headers, &[])
}
