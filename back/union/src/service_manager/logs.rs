//! 有界日志尾部读取。

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// 读取日志文件最后 `max_lines` 行（类似 `tail -n` 命令的功能）。
///
/// # 为什么用 tail 而不是全量读取？
/// 日志文件可能随时间增长到数十 MB 甚至更大。如果每次查看日志都把整个文件
/// 读入内存，会造成不必要的内存压力和延迟。
/// 通常用户只关心最近发生的事情（最后几百行），所以只返回尾部即可。
///
/// # 实现方式：从文件尾部反向读取
/// 按 8 KiB 块从末尾向前读取，直到找到足够多的换行符，再只解析尾部片段。
///
/// 这种方式的内存和耗时主要与需要返回的尾部内容相关，不随日志总大小线性增长。
pub fn tail_lines(path: &Path, max_lines: usize) -> std::io::Result<Vec<String>> {
    if max_lines == 0 || !path.exists() {
        return Ok(Vec::new());
    }

    const CHUNK_SIZE: u64 = 8 * 1024;

    let mut file = fs::File::open(path)?;
    let mut position = file.metadata()?.len();
    if position == 0 {
        return Ok(Vec::new());
    }

    let mut retained = Vec::new();
    let mut newline_count = 0_usize;
    while position > 0 && newline_count <= max_lines {
        let read_len = position.min(CHUNK_SIZE);
        position -= read_len;
        file.seek(SeekFrom::Start(position))?;

        let mut chunk = vec![0_u8; read_len as usize];
        file.read_exact(&mut chunk)?;
        newline_count += chunk.iter().filter(|byte| **byte == b'\n').count();

        let mut combined = Vec::with_capacity(chunk.len() + retained.len());
        combined.extend_from_slice(&chunk);
        combined.extend_from_slice(&retained);
        retained = combined;
    }

    let text = String::from_utf8_lossy(&retained);
    let mut lines = text.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    if position > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    if lines.len() > max_lines {
        Ok(lines.split_off(lines.len() - max_lines))
    } else {
        Ok(lines)
    }
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

    #[test]
    fn returns_last_requested_lines() {
        let path = std::env::temp_dir().join(format!("union-log-test-{}", uuid::Uuid::new_v4()));
        fs::write(&path, "one\ntwo\nthree\nfour\n").unwrap();

        assert_eq!(tail_lines(&path, 2).unwrap(), vec!["three", "four"]);
        assert_eq!(
            tail_lines(&path, 10).unwrap(),
            vec!["one", "two", "three", "four"]
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn tails_large_files_without_reading_from_start() {
        let path = std::env::temp_dir().join(format!("union-log-test-{}", uuid::Uuid::new_v4()));
        let mut content = String::new();
        for index in 0..20_000 {
            content.push_str(&format!("line-{index}\n"));
        }
        fs::write(&path, content).unwrap();

        assert_eq!(
            tail_lines(&path, 3).unwrap(),
            vec!["line-19997", "line-19998", "line-19999"]
        );

        fs::remove_file(path).unwrap();
    }
}
