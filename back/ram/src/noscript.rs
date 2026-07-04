//! 无 JavaScript 客户端的目录列表页面。
//!
//! ram 默认前端依赖浏览器执行 JavaScript。curl、wget、Lynx 这类客户端不适合加载前端应用，
//! 因此本文件生成一个非常朴素的 HTML 表格，保证它们也能浏览目录。

use crate::{
    server::{IndexData, PathItem, PathType, MAX_SUBPATHS_COUNT},
    utils::encode_uri,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use xml::escape::escape_str_pcdata;

/// 根据 User-Agent 判断是否应该返回无 JavaScript 页面。
pub fn detect_noscript(user_agent: &str) -> bool {
    // 根据常见命令行/文本浏览器 User-Agent 自动开启 noscript 模式。
    [
        "lynx/", "w3m/", "links ", "elinks/", "curl/", "wget/", "httpie/", "aria2/",
    ]
    .iter()
    .any(|v| user_agent.starts_with(v))
}

/// 根据目录数据生成纯 HTML 列表。
pub fn generate_noscript_html(data: &IndexData) -> Result<String> {
    // 手工拼接 HTML。所有来自文件名的文本都要 escape，避免文件名注入 HTML。
    let mut html = String::new();

    let title = format!("Index of {}", escape_str_pcdata(&data.href));

    html.push_str("<html>\n");
    html.push_str("<head>\n");
    html.push_str(&format!("<title>{title}</title>\n"));
    html.push_str(
        r#"<style>
  td {
    padding: 0.2rem;
    text-align: left;
  }
  td:nth-child(3) {
    text-align: right;
  }
</style>
"#,
    );
    html.push_str("</head>\n");
    html.push_str("<body>\n");
    html.push_str(&format!("<h1>{title}</h1>\n"));
    html.push_str("<table>\n");
    html.push_str("  <tbody>\n");
    html.push_str(&format!("    {}\n", render_parent()));

    for path in &data.paths {
        html.push_str(&format!("    {}\n", render_path_item(path)));
    }

    html.push_str("  </tbody>\n");
    html.push_str("</table>\n");
    html.push_str("</body>\n");

    Ok(html)
}

/// 渲染返回上级目录的行。
fn render_parent() -> String {
    let value = "../";
    format!("<tr><td><a href=\"{value}?noscript\">{value}</a></td><td></td><td></td></tr>")
}

/// 渲染单个文件或目录行。
fn render_path_item(path: &PathItem) -> String {
    // 目录链接继续带 ?noscript，确保后续页面仍然保持纯 HTML 模式。
    let mut href = encode_uri(&path.name);
    let mut name = escape_str_pcdata(&path.name).to_string();
    if path.path_type.is_dir() {
        href.push_str("/?noscript");
        name.push('/');
    };
    let mtime = format_mtime(path.mtime).unwrap_or_default();
    let size = format_size(path.size, path.path_type);

    format!("<tr><td><a href=\"{href}\">{name}</a></td><td>{mtime}</td><td>{size}</td></tr>")
}

/// 把毫秒时间戳格式化成可读时间。
fn format_mtime(mtime: u64) -> Option<String> {
    let datetime = DateTime::<Utc>::from_timestamp_millis(mtime as _)?;
    Some(datetime.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string())
}

/// 把文件大小格式化成 B/KiB/MiB/GiB，目录不显示大小。
fn format_size(size: u64, path_type: PathType) -> String {
    // 对目录来说 size 表示子项数量；对文件来说 size 才表示字节大小。
    if path_type.is_dir() {
        let unit = if size == 1 { "item" } else { "items" };
        let num = match size >= MAX_SUBPATHS_COUNT {
            true => format!(">{}", MAX_SUBPATHS_COUNT - 1),
            false => size.to_string(),
        };
        format!("{num} {unit}")
    } else {
        if size == 0 {
            return "0 B".to_string();
        }
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let i = (size as f64).log2() / 10.0;
        let i = i.floor() as usize;

        if i >= UNITS.len() {
            // 超过 TB 的极大文件用 PB 兜底展示。
            return format!("{:.2} PB", size as f64 / 1024.0f64.powi(5));
        }

        let size = size as f64 / 1024.0f64.powi(i as i32);
        format!("{:.2} {}", size, UNITS[i])
    }
}
