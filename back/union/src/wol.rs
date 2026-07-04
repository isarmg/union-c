//! Wake-on-LAN 唤醒逻辑。
//!
//! Wake-on-LAN（WOL）是一种网络协议，允许通过局域网远程唤醒处于待机/休眠状态的计算机。
//! 原理：向局域网广播一个"魔术包"（Magic Packet），目标主机的网卡识别后将系统唤醒。
//! 前提条件：目标主机的网卡和主板 BIOS/UEFI 需要开启 WOL 支持。

use tokio::net::UdpSocket;

use crate::{
    app_config::SunshineHostConfig,
    database::{self, DbPool},
    domain::WakeResponse,
    error::{AppError, AppResult},
};

/// 向指定 Sunshine 主机发送 Wake-on-LAN 魔术包。
///
/// # WOL 魔术包结构
///
/// 标准魔术包格式固定为 102 字节：
/// - 前 6 字节：全部为 `0xFF`（同步头，告知网卡"这是一个魔术包"）
/// - 后 96 字节：目标主机 MAC 地址连续重复 16 次（16 × 6 字节 = 96 字节）
///
/// 网卡收到后，识别出这个特殊结构，然后给主板发送唤醒信号。
///
/// # 为什么用 UDP 广播？
///
/// 目标主机处于休眠状态，无法建立 TCP 连接。UDP 广播不需要对方响应，
/// 只要把数据包发往整个局域网（广播地址，如 `255.255.255.255:9`），
/// 网卡即使在系统休眠时也能持续监听并识别魔术包。
/// 端口 9（discard 端口）是 WOL 的惯用端口，也可以使用 7 或其他端口。
pub async fn wake_host(host: &SunshineHostConfig, db: &DbPool) -> AppResult<WakeResponse> {
    // `as_deref()` 将 `Option<String>` 转为 `Option<&str>`，避免不必要的克隆。
    // `let Some(...) else { return ... }` 是 Rust 的 "let-else" 语法：
    // 如果 Option 是 None，就执行 else 分支（提前返回错误），否则解包值。
    let Some(mac_address) = host.mac_address.as_deref() else {
        return Err(AppError::BadRequest("该主机未配置 mac_address".to_string()));
    };

    // 解析 MAC 地址字符串为 6 字节数组
    let mac = parse_mac(mac_address)?;

    let packet = magic_packet(mac);

    // 绑定到本机任意可用 UDP 端口（"0.0.0.0:0" 表示操作系统自动分配端口）
    let socket = UdpSocket::bind("0.0.0.0:0").await?;

    // 必须启用广播权限，否则操作系统会拒绝向广播地址发送数据
    socket.set_broadcast(true)?;

    // 向广播地址发送魔术包（如 "255.255.255.255:9" 会覆盖整个局域网）
    socket.send_to(&packet, &host.broadcast_addr).await?;

    // 记录审计日志：谁、对谁、发送了什么操作
    database::insert_audit(
        db,
        "sunshine.wake",
        "sunshine",
        Some(&format!(
            "sent magic packet to {} ({})",
            host.broadcast_addr, host.name
        )),
    )
    .await?;

    Ok(WakeResponse {
        ok: true,
        target: mac_address.to_string(),
        broadcast_addr: host.broadcast_addr.clone(),
    })
}

/// 将 MAC 地址字符串解析为 6 字节数组。
///
/// # 为什么 MAC 地址是 `[u8; 6]`？
///
/// MAC 地址（Media Access Control Address）由 48 位（6 字节）组成，
/// 用于唯一标识网络接口。`[u8; 6]` 就是一个固定长度为 6 的字节数组：
/// - `u8`：无符号 8 位整数，范围 0-255，对应一个字节
/// - `; 6`：数组长度为 6
///
/// # 解析流程
///
/// 1. 去除分隔符（`:` 或 `-`），将 `"AA:BB:CC:DD:EE:FF"` 变为 `"AABBCCDDEEFF"`
/// 2. 校验长度必须为 12（6 字节 × 每字节 2 个十六进制字符）
/// 3. 每 2 个字符解析为一个十六进制数（`u8::from_str_radix(&s, 16)`）
///    - 16 进制中每个字符代表 4 位，两个字符合起来代表 8 位（1 字节）
fn parse_mac(input: &str) -> AppResult<[u8; 6]> {
    // 去掉常见的 MAC 地址分隔符，支持 "AA:BB:CC:DD:EE:FF" 和 "AA-BB-CC-DD-EE-FF" 两种格式
    let compact = input.replace([':', '-'], "");

    // 12 个十六进制字符 = 6 字节（每字节 2 个十六进制字符）
    if compact.len() != 12 {
        return Err(AppError::BadRequest(
            "MAC address must contain 6 octets".to_string(),
        ));
    }

    // `[0u8; 6]` 初始化 6 个字节全为 0 的数组，后续逐字节填入解析结果
    let mut output = [0u8; 6];
    for (index, byte) in output.iter_mut().enumerate() {
        let start = index * 2; // 每个字节占 2 个字符，第 n 个字节从位置 n*2 开始
        // `u8::from_str_radix(..., 16)` 把十六进制字符串解析为 u8
        // 例如 "FF" → 255，"0A" → 10
        // `map_err` 将 parse 失败的错误转换为自定义 AppError
        *byte = u8::from_str_radix(&compact[start..start + 2], 16)
            .map_err(|_| AppError::BadRequest("MAC address contains invalid hex".to_string()))?;
    }
    Ok(output)
}

fn magic_packet(mac: [u8; 6]) -> [u8; 102] {
    let mut packet = [0_u8; 102];
    packet[..6].fill(0xff);
    for chunk in packet[6..].chunks_exact_mut(6) {
        chunk.copy_from_slice(&mac);
    }
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mac_and_builds_standard_magic_packet() {
        let mac = parse_mac("AA:BB:CC:DD:EE:FF").unwrap();
        let packet = magic_packet(mac);
        assert_eq!(&packet[..6], &[0xff; 6]);
        assert!(packet[6..].chunks_exact(6).all(|chunk| chunk == mac));
    }

    #[test]
    fn rejects_invalid_mac() {
        assert!(parse_mac("AA:BB:CC").is_err());
        assert!(parse_mac("GG:BB:CC:DD:EE:FF").is_err());
    }
}
