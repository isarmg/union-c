//! 系统资源采集。
//!
//! 当前读取 CPU、内存、磁盘和网络吞吐信息，不修改系统状态。数据来自 `sysinfo` crate。
//!
//! # 网络吞吐量计算原理
//!
//! 网络吞吐量（每秒传输字节数）不能直接读取，需要通过差值计算：
//! - 第一次采样：记录已接收/已发送的累计字节数 + 时间戳
//! - 第二次采样：再次读取累计字节数 + 当前时间
//! - 速率 = （新累计值 - 旧累计值）/ 经过的秒数
//!
//! 这就是为什么需要用全局静态变量 `NETWORK_SAMPLE` 保存上次采样结果。

use std::{
    sync::{Mutex, OnceLock},
    time::Instant,
};

use sysinfo::{Disks, Networks, System};

use crate::domain::{DiskInfo, DiskThroughput, NetworkThroughput, SystemResources};

// `OnceLock<Mutex<NetworkSample>>` 这个组合类型需要拆开理解：
//
// - `OnceLock<T>`：全局单例容器，只能被初始化一次，之后只读。
//   类似其他语言的 "懒加载单例"，在第一次调用 `get_or_init` 时完成初始化。
//   这里用它确保 `NetworkSample` 在整个程序生命周期中只创建一次。
//
// - `Mutex<T>`：互斥锁，同一时刻只允许一个线程持有锁并访问内部数据。
//   因为这是 async 服务器，多个请求可能并发执行，需要用 Mutex 防止同时修改
//   `NetworkSample`（否则可能读到部分更新的数据，产生错误的速率计算结果）。
//
// 组合起来：`OnceLock` 保证只初始化一次，`Mutex` 保证并发安全的读写访问。
static NETWORK_SAMPLE: OnceLock<Mutex<NetworkSample>> = OnceLock::new();

/// 存储网络采样数据，用于下次调用时计算速率差。
struct NetworkSample {
    networks: Networks, // sysinfo 提供的网络接口数据（包含累计字节计数器）
    last_seen: Instant, // 上次采样时的时间点（Instant 是单调递增时钟，不受系统时间调整影响）
}

/// 采集当前系统资源快照。
///
/// 每次调用都会即时刷新（而非缓存），调用方（SSE 循环等）自己控制采集频率。
pub fn collect_resources() -> SystemResources {
    // `System::new_all()` 创建一个 sysinfo System 实例并立刻刷新所有数据。
    // sysinfo 的设计是：先创建实例，再调用 refresh 方法更新数据，
    // 这样可以选择性地只刷新需要的部分（省略不需要的开销）。
    let mut system = System::new_all();
    system.refresh_all(); // 一次性刷新所有指标：CPU、内存、进程等

    // `Disks::new_with_refreshed_list()` 枚举当前挂载的所有磁盘/分区。
    // `.list()` 返回磁盘切片，`.iter().map(...)` 转换为我们的 DiskInfo 类型。
    // `to_string_lossy()` 将路径转为字符串（遇到非 UTF-8 字符时用替代符，不会 panic）
    let disks = Disks::new_with_refreshed_list()
        .list()
        .iter()
        .map(|disk| DiskInfo {
            name: disk.name().to_string_lossy().to_string(),
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
        })
        .collect();

    SystemResources {
        cpu_usage_percent: system.global_cpu_usage(), // 全局 CPU 使用率百分比（0.0 - 100.0）
        memory_total_kib: system.total_memory() / 1024, // sysinfo 返回字节，除以 1024 转为 KiB
        memory_used_kib: system.used_memory() / 1024,
        network: collect_network_throughput(),
        disk_throughput: collect_disk_throughput(),
        disks,
    }
}

/// 计算网络接口的实时吞吐量（接收和发送的字节/秒）。
///
/// # 为什么需要两次采样？
///
/// 操作系统内核维护的是网络接口的"累计字节计数器"（自系统启动以来的总量），
/// 不是"当前速率"。要得到速率，必须：
/// 1. 记录 T1 时刻的累计值
/// 2. 等待一段时间（elapsed）
/// 3. 读取 T2 时刻的累计值
/// 4. 速率 = (T2累计值 - T1累计值) / elapsed
///
/// 这里用全局变量保存 T1 的数据，每次调用相当于"读取 T2 并更新 T1"。
fn collect_network_throughput() -> NetworkThroughput {
    // `get_or_init` 如果 OnceLock 还未初始化，就用闭包初始化，然后返回引用。
    // 如果已经初始化，直接返回已有值的引用。
    // 这样无论调用多少次，NetworkSample 只会创建一次。
    let sample = NETWORK_SAMPLE.get_or_init(|| {
        Mutex::new(NetworkSample {
            networks: Networks::new_with_refreshed_list(), // 初始化时做第一次采样
            last_seen: Instant::now(),
        })
    });

    // `.lock()` 获取互斥锁，返回 `Result<MutexGuard, PoisonError>`。
    // 正常情况返回 `Ok(guard)`，当持有锁的线程 panic 时才会返回 `Err(poisoned)`。
    // `into_inner()` 从 PoisonError 中取回数据——即使之前有 panic，数据本身通常还是可用的。
    // 这里选择"容忍毒化错误"而不是直接 unwrap，避免因一次 panic 导致所有后续请求都失败。
    let mut sample = match sample.lock() {
        Ok(sample) => sample,
        Err(poisoned) => poisoned.into_inner(), // 毒化时仍然尝试使用数据
    };

    let now = Instant::now();

    // 计算距离上次采样的秒数，用于除法得到"每秒速率"。
    // `.max(1.0)` 是关键的安全保护：防止除以零。
    // 如果两次调用之间时间极短（如并发请求），elapsed 可能接近 0，
    // 直接相除会产生非常大的错误值甚至除以零（浮点数除以 0.0 得到 infinity）。
    // 限制最小值为 1.0 秒，使极短时间内的速率估计也保持合理。
    let elapsed_seconds = now.duration_since(sample.last_seen).as_secs_f64().max(1.0);

    // sysinfo 0.39+ 的 `refresh` 接受一个参数：是否移除已消失的网络接口。
    // `true` = 移除；如果一个网卡被拔除，下次刷新后从列表中删除它。
    sample.networks.refresh(true);

    // 遍历所有网络接口，累加自上次刷新以来（sysinfo 内部做了差值）的字节数。
    // `data.received()` 返回上次 refresh 到本次 refresh 之间接收的字节数
    // `data.transmitted()` 返回同一时间段内发送的字节数
    let received = (&sample.networks)
        .into_iter()
        .map(|(_, data)| data.received())
        .sum::<u64>();
    let transmitted = (&sample.networks)
        .into_iter()
        .map(|(_, data)| data.transmitted())
        .sum::<u64>();

    // 更新时间戳，供下次调用时计算 elapsed
    sample.last_seen = now;

    // 将 sysinfo 提供的"本次刷新间隔内的总字节"除以"实际经过秒数"，得到每秒速率
    // `.round() as u64` 四舍五入取整，保持输出为整数字节数
    let received_bytes_per_second = (received as f64 / elapsed_seconds).round() as u64;
    let transmitted_bytes_per_second = (transmitted as f64 / elapsed_seconds).round() as u64;

    NetworkThroughput {
        received_bytes_per_second,
        transmitted_bytes_per_second,
        total_bytes_per_second: received_bytes_per_second + transmitted_bytes_per_second,
    }
}

// ─── 磁盘 IO 吞吐量（读取 /proc/diskstats） ──────────────────────────────────

struct DiskSample {
    read_sectors: u64,
    write_sectors: u64,
    last_seen: Instant,
}

static DISK_SAMPLE: OnceLock<Mutex<DiskSample>> = OnceLock::new();

/// 从 `/proc/diskstats` 读取所有物理磁盘的累计扇区数，返回 (read_sectors, write_sectors)。
/// 仅统计名称以字母开头且不以数字结尾的设备（即整盘，排除分区）。
fn read_diskstats() -> (u64, u64) {
    let Ok(content) = std::fs::read_to_string("/proc/diskstats") else {
        return (0, 0);
    };
    let mut read_total: u64 = 0;
    let mut write_total: u64 = 0;
    for line in content.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 14 {
            continue;
        }
        let dev = fields[2];
        if is_whole_block_device(dev) {
            read_total += fields[5].parse::<u64>().unwrap_or(0); // 已读扇区数
            write_total += fields[9].parse::<u64>().unwrap_or(0); // 已写扇区数
        }
    }
    (read_total, write_total)
}

fn is_whole_block_device(name: &str) -> bool {
    if name.starts_with("loop") || name.starts_with("ram") {
        return false;
    }
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        // 这两类整盘名称以数字结尾，分区才带 pN。
        return !name.rsplit_once('p').is_some_and(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        });
    }
    if name.starts_with("md") || name.starts_with("dm-") {
        return true;
    }
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
        && !name.chars().last().is_some_and(|ch| ch.is_ascii_digit())
}

fn collect_disk_throughput() -> DiskThroughput {
    const SECTOR_BYTES: u64 = 512;

    let sample = DISK_SAMPLE.get_or_init(|| {
        let (r, w) = read_diskstats();
        Mutex::new(DiskSample {
            read_sectors: r,
            write_sectors: w,
            last_seen: Instant::now(),
        })
    });

    let mut sample = match sample.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    let now = Instant::now();
    let elapsed = now.duration_since(sample.last_seen).as_secs_f64().max(1.0);

    let (cur_read, cur_write) = read_diskstats();
    let read_sectors_delta = cur_read.saturating_sub(sample.read_sectors);
    let write_sectors_delta = cur_write.saturating_sub(sample.write_sectors);

    sample.read_sectors = cur_read;
    sample.write_sectors = cur_write;
    sample.last_seen = now;

    let read_bps = ((read_sectors_delta * SECTOR_BYTES) as f64 / elapsed).round() as u64;
    let write_bps = ((write_sectors_delta * SECTOR_BYTES) as f64 / elapsed).round() as u64;

    DiskThroughput {
        read_bytes_per_second: read_bps,
        write_bytes_per_second: write_bps,
        total_bytes_per_second: read_bps + write_bps,
    }
}
