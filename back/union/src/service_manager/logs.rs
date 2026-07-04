//! 有界日志尾部读取。

use std::{fs, path::Path};

/// 读取日志文件最后 `max_lines` 行（类似 `tail -n` 命令的功能）。
///
/// # 为什么用 tail 而不是全量读取？
/// 日志文件可能随时间增长到数十 MB 甚至更大。如果每次查看日志都把整个文件
/// 读入内存，会造成不必要的内存压力和延迟。
/// 通常用户只关心最近发生的事情（最后几百行），所以只返回尾部即可。
///
/// # 实现方式：滑动窗口（固定容量循环缓冲区）
/// 使用 `VecDeque`（双端队列）作为固定大小的环形缓冲区：
/// - 逐行读取文件，每读一行就加入队列尾部；
/// - 如果队列已满（len >= max_lines），先从队列头部弹出最早的一行；
/// - 读完整个文件后，队列里恰好保存的是最后 max_lines 行。
///
/// 这种方式只需要 O(max_lines) 的内存，与文件总大小无关，
/// 也不需要知道文件总行数，只需顺序读一遍。
pub fn tail_lines(path: &Path, max_lines: usize) -> std::io::Result<Vec<String>> {
    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader};

    if max_lines == 0 || !path.exists() {
        return Ok(Vec::new());
    }

    // BufReader 为文件 I/O 添加用户空间缓冲区，避免每读一行都触发一次系统调用，
    // 大幅提升逐行读取的性能。
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    // with_capacity 预分配内存，避免滑动过程中频繁扩容。
    // saturation_add 防止 max_lines + 1 溢出（usize 最大值时加 1 会回绕）。
    let mut buf: VecDeque<String> = VecDeque::with_capacity(max_lines.saturating_add(1));

    for line in reader.lines() {
        if buf.len() >= max_lines {
            buf.pop_front(); // 满了就丢弃最早的一行，保持队列大小不超过 max_lines
        }
        buf.push_back(line?); // 追加新行到队尾
    }

    // VecDeque 转为 Vec，以便序列化或返回给调用方。
    Ok(buf.into_iter().collect())
}

#[cfg(test)]
mod tail_tests {
    use super::tail_lines;
    use std::fs;

    #[test]
    fn zero_lines_returns_no_log_content() {
        let path = std::env::temp_dir().join(format!("union-log-test-{}", uuid::Uuid::new_v4()));
        fs::write(&path, "one\ntwo\n").unwrap();
        assert!(tail_lines(&path, 0).unwrap().is_empty());
        fs::remove_file(path).unwrap();
    }
}
