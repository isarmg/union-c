# Blog 发布流程

博客内容的管理源是 PostgreSQL，不是 `blog/data/content/`。

## 单机或同机部署

Union 可以在本机导出内容并直接执行博客构建：

1. 从 PostgreSQL 导出文章、分类、标签和站点配置到 `blog/data/content/`。
2. 使用 `blog/data/files/` 作为图片和附件源目录。
3. 在 `blog/source` 内执行 `npm run build`。
4. 构建到 `dist.next/`，复制公开资源到 `dist.next/blog-assets/`。
5. 验证成功后把 `dist.next/` 原子切换为 `dist/`。

构建失败时保留最近一次成功的 `dist/`。排障先看 `blog/data/logs/`。

## 四机独立部署

Blog 主机不直接访问 Union 的本地文件系统。需要一个显式同步步骤把以下目录传到 Blog 主机：

- `blog/data/content/`
- `blog/data/files/`

同步方式可以是 `rsync`、备份恢复、对象存储拉取或后续专用发布脚本。同步必须保证同一批内容和资源来自同一版本，避免文章引用了尚未同步的图片。

同步后在 Blog 主机执行：

```bash
cd blog/source
npm ci
npm run build
```

然后由 Caddy 托管 `blog/source/dist/`。

## 回滚

`dist.previous/` 只覆盖最近一次本机构建切换，不能替代备份。跨主机回滚应使用同一恢复点恢复：

- PostgreSQL；
- `blog/data/files/`；
- `blog/data/content/`；
- 当前发布的 `blog/source/dist/`。
