//! 孤立文章收养与内容路径安全。

use super::{storage::*, *};

/// 扫描内容目录，把不在数据库中的 .md/.mdx 文件以 draft=true 导入。
/// 同时把这些文件重写一遍，确保磁盘上的 frontmatter 也是 draft: true。
///
/// 仅由显式导入接口调用；正常启动和构建始终以 PostgreSQL 为唯一管理源。
pub async fn adopt_orphan_posts(state: &AppState) -> AppResult<usize> {
    let content_dir = &state.settings.paths.blog_export_dir;
    if !content_dir.exists() {
        return Ok(0);
    }
    let canonical_base = match content_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return Ok(0),
    };

    let known: std::collections::BTreeSet<String> = database::list_blog_posts(state.db().as_ref())
        .await?
        .into_iter()
        .map(|r| r.relative_path)
        .collect();

    let files = collect_post_files(&canonical_base)?;
    let mut adopted = 0;

    for file_path in files {
        let relative = match file_path.strip_prefix(&canonical_base) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if known.contains(&relative) {
            continue;
        }

        let raw = match fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut request = parse_orphan_file(&raw, &relative);
        request.draft = true;
        normalize_orphan_request(&mut request);
        validate_post_request(&request)?;

        let input = post_input_from_request(&request);
        database::upsert_blog_post(state.db().as_ref(), &input).await?;
        if let Some(category) = input.category.as_deref() {
            database::insert_blog_taxonomy(
                state.db().as_ref(),
                TaxonomyKind::Category.db_kind(),
                category,
            )
            .await?;
        }
        for tag in &input.tags {
            database::insert_blog_taxonomy(state.db().as_ref(), TaxonomyKind::Tag.db_kind(), tag)
                .await?;
        }

        database::insert_audit(
            state.db().as_ref(),
            "blog.post.adopt",
            &relative,
            Some("adopted as draft: file existed on disk but was not in database"),
        )
        .await?;

        adopted += 1;
    }

    Ok(adopted)
}

/// 递归收集目录下所有 .md / .mdx 文件，最多递归 8 层，不跟随符号链接。
fn collect_post_files(dir: &Path) -> AppResult<Vec<PathBuf>> {
    collect_post_files_depth(dir, 0)
}

fn collect_post_files_depth(dir: &Path, depth: u32) -> AppResult<Vec<PathBuf>> {
    if depth > 8 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        // 不跟随符号链接，防止符号链接循环导致无限递归。
        if path.is_dir() && !path.is_symlink() {
            out.extend(collect_post_files_depth(&path, depth + 1)?);
        } else if is_post_path(&path) && !path.is_symlink() {
            out.push(path);
        }
    }
    Ok(out)
}

/// 尽力解析孤立文件的 frontmatter，返回可直接写入数据库的 BlogPostSaveRequest。
/// 缺失字段填安全默认值；draft 始终由调用方强制设为 true。
fn parse_orphan_file(raw: &str, relative_path: &str) -> BlogPostSaveRequest {
    let (front, body) = split_frontmatter(raw);
    let stem = relative_path
        .trim_end_matches(".mdx")
        .trim_end_matches(".md");

    let mut request = BlogPostSaveRequest {
        original_relative_path: None,
        relative_path: relative_path.to_string(),
        title: stem.to_string(),
        description: String::new(),
        pub_date: Utc::now().date_naive().to_string(),
        updated_date: None,
        author: None,
        category: None,
        series: None,
        hero_image: None,
        tags: Vec::new(),
        draft: true,
        featured: false,
        content: body.to_string(),
    };

    for line in front.lines() {
        let line = line.trim();
        if let Some(v) = frontmatter_field(line, "title") {
            if !v.is_empty() {
                request.title = v;
            }
        } else if let Some(v) = frontmatter_field(line, "description") {
            request.description = v;
        } else if let Some(v) = frontmatter_field(line, "pubDate") {
            request.pub_date = v;
        } else if let Some(v) = frontmatter_field(line, "updatedDate") {
            request.updated_date = Some(v);
        } else if let Some(v) = frontmatter_field(line, "author") {
            request.author = Some(v);
        } else if let Some(v) = frontmatter_field(line, "category") {
            request.category = Some(v);
        } else if let Some(v) = frontmatter_field(line, "series") {
            request.series = Some(v);
        } else if let Some(v) = frontmatter_field(line, "heroImage") {
            request.hero_image = Some(v);
        } else if let Some(v) = frontmatter_field(line, "tags") {
            request.tags = parse_yaml_string_array(&v);
        } else if let Some(v) = frontmatter_field(line, "featured") {
            request.featured = v == "true";
        }
        // draft 不解析——调用方始终强制设为 true
    }

    if NaiveDate::parse_from_str(&request.pub_date, "%Y-%m-%d").is_err() {
        request.pub_date = Utc::now().date_naive().to_string();
    }

    request
}

fn normalize_orphan_request(request: &mut BlogPostSaveRequest) {
    request.title = request.title.trim().to_string();
    if request.title.is_empty() {
        request.title = request
            .relative_path
            .trim_end_matches(".mdx")
            .trim_end_matches(".md")
            .to_string();
    }

    request.description = request.description.trim().to_string();
    if request.description.is_empty() {
        request.description = request.title.clone();
    }

    request.updated_date = clean_optional(&request.updated_date).and_then(|value| {
        NaiveDate::parse_from_str(&value, "%Y-%m-%d")
            .is_ok()
            .then_some(value)
    });
    request.author = clean_optional(&request.author);
    request.category = clean_optional(&request.category);
    request.series = clean_optional(&request.series);
    request.hero_image = clean_blog_asset_path(&request.hero_image);
    request.tags = normalize_list(request.tags.clone());
}

/// 把 `---\nfrontmatter\n---\nbody` 拆成 (frontmatter, body)。
/// 文件没有 YAML front matter 分隔符时返回 ("", 全文)。
fn split_frontmatter(raw: &str) -> (&str, &str) {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw); // 去 BOM
    let Some(after_open) = raw.strip_prefix("---") else {
        return ("", raw);
    };
    // 跳过开头 --- 后的换行
    let rest = after_open
        .strip_prefix('\n')
        .or_else(|| after_open.strip_prefix("\r\n"))
        .unwrap_or(after_open);
    // 查找单独占一行的 --- 作为结束分隔符
    for (marker, body_offset) in [("\n---\n", 5usize), ("\n---\r\n", 6)] {
        if let Some(pos) = rest.find(marker) {
            let front = &rest[..pos];
            let body = rest[pos + body_offset..].trim_start_matches('\n');
            return (front, body);
        }
    }
    // 文件末尾无换行时：仅在字符串末尾匹配 \n---，避免误匹配 ---subtitle 等行
    if let Some(front) = rest.strip_suffix("\n---") {
        return (front, "");
    }
    ("", raw)
}

/// 从单行 frontmatter 中解析 `key: value`，返回去引号后的字符串。
fn frontmatter_field(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?;
    let rest = rest.strip_prefix(':')?;
    let value = rest.strip_prefix(' ').unwrap_or(rest).trim();
    Some(unescape_yaml_scalar(value))
}

/// 去掉可选的外层引号并反转义 `\"` 和 `\\`。
fn unescape_yaml_scalar(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        s[1..s.len() - 1].replace("''", "'")
    } else {
        s.to_string()
    }
}

/// 把 YAML 行内序列 `["a", "b"]` 解析成 Vec<String>。
fn parse_yaml_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Vec::new();
    };
    if inner.trim().is_empty() {
        return Vec::new();
    }
    inner
        .split(',')
        .map(|item| unescape_yaml_scalar(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

pub(super) fn validate_relative_post_path(
    base: &Path,
    requested: &str,
    must_exist: bool,
) -> AppResult<PathBuf> {
    fs::create_dir_all(base)?;
    safe_content_path(base, requested, must_exist)
}

/// 把用户传入的相对路径转换成内容目录内的安全绝对路径。
///
/// 这是文件写入接口最重要的安全边界：拒绝绝对路径、`..` 等越界路径。
pub(super) fn safe_content_path(
    base: &Path,
    requested: &str,
    must_exist: bool,
) -> AppResult<PathBuf> {
    let base = base.canonicalize()?;
    if requested.contains('\\') {
        return Err(AppError::BadRequest(
            "blog path must use Linux '/' separators".to_string(),
        ));
    }
    if requested.starts_with('/') {
        return Err(AppError::BadRequest(
            "blog path must be relative to the content directory".to_string(),
        ));
    }
    let requested = requested.trim();
    if requested.trim().is_empty() {
        return Err(AppError::BadRequest(
            "blog path cannot be empty".to_string(),
        ));
    }

    let mut relative = PathBuf::new();
    // 只接受普通相对路径组件，拒绝 .. 和绝对路径，防止越界写文件。
    for component in Path::new(requested).components() {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::BadRequest(
                    "blog path escapes content directory".to_string(),
                ));
            }
        }
    }

    if !is_post_path(&relative) {
        return Err(AppError::BadRequest(
            "blog post path must end with .md or .mdx".to_string(),
        ));
    }

    let path = base.join(&relative);
    if must_exist && !path.exists() {
        return Err(AppError::BadRequest(format!(
            "blog post does not exist: {requested}"
        )));
    }
    if let Ok(canonical) = path.canonicalize() {
        // 已存在文件用 canonicalize 做最终确认，防止符号链接跳出内容目录。
        if !canonical.starts_with(&base) {
            return Err(AppError::BadRequest(
                "blog path escapes content directory".to_string(),
            ));
        }
        return Ok(canonical);
    }

    // 对每个已存在的父路径做元数据检查。仅做字符串前缀判断会允许
    // `content/link/new.md` 经由 `link -> /outside` 写出内容目录。
    let mut current = base.clone();
    for component in relative.parent().into_iter().flat_map(Path::components) {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::BadRequest(
                    "blog path must not traverse symbolic links".to_string(),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(AppError::BadRequest(
                    "blog path parent is not a directory".to_string(),
                ));
            }
            Ok(_) => {
                let canonical = current.canonicalize()?;
                if !canonical.starts_with(&base) {
                    return Err(AppError::BadRequest(
                        "blog path escapes content directory".to_string(),
                    ));
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(path)
}

#[cfg(test)]
mod path_tests {
    use super::{normalize_orphan_request, parse_orphan_file, safe_content_path};
    use std::{fs, os::unix::fs::symlink};

    #[test]
    fn new_post_cannot_traverse_parent_symlink() {
        let root = std::env::temp_dir().join(format!("union-path-test-{}", uuid::Uuid::new_v4()));
        let content = root.join("content");
        let outside = root.join("outside");
        fs::create_dir_all(&content).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, content.join("escape")).unwrap();

        let result = safe_content_path(&content, "escape/new.md", false);
        assert!(result.is_err());
        assert!(!outside.join("new.md").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn post_path_rejects_absolute_parent_and_windows_paths() {
        let root = std::env::temp_dir().join(format!("union-path-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        assert!(safe_content_path(&root, "/absolute.md", false).is_err());
        assert!(safe_content_path(&root, "../outside.md", false).is_err());
        assert!(safe_content_path(&root, r"nested\windows.md", false).is_err());
        assert!(safe_content_path(&root, "nested/valid.mdx", false).is_ok());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn orphan_request_is_normalized_before_validation() {
        let raw = r#"---
title: "  "
description: ""
pubDate: invalid
updatedDate: invalid
tags: [" rust ", "rust"]
---

Body
"#;
        let mut request = parse_orphan_file(raw, "nested/draft.md");
        normalize_orphan_request(&mut request);

        assert_eq!(request.title, "nested/draft");
        assert_eq!(request.description, "nested/draft");
        assert_eq!(request.pub_date.len(), 10);
        assert_eq!(request.updated_date, None);
        assert_eq!(request.tags, vec!["rust"]);
    }
}
