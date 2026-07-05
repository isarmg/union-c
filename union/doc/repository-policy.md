# 仓库维护规则

## 允许提交

- `back/source`、`union/source`、`ram/source`、`blog/source` 的源代码；
- 每个独立项目的 manifest 和锁文件；
- 页面必须的源码静态资源，例如 SVG、ram 内嵌前端资源；
- Linux CI、部署配置和运维脚本；
- `.env.production.example` 等不含真实秘密的模板；
- `back/doc`、`union/doc`、`ram/doc`、`blog/doc` 下的当前文档；
- 上游许可证。

## 禁止提交

- Rust `target/`；
- `node_modules/`；
- `dist/`、`dist.next/`、`dist.previous/`、`.astro/`、`.vite/`；
- 各项目 `data/` 下除 `.gitkeep` 以外的运行数据；
- PostgreSQL dump、备份归档和校验产物；
- `.env` 实例、密钥、密码、token、证书私钥；
- IDE 配置、调试历史、代理会话目录；
- 临时日志、截图、用户上传、博客运行内容；
- 项目根目录或子项目中未纳入对应项目 `doc/` 文档体系的零散 Markdown。

## 文档规则

- 详细 Markdown 文档放在对应项目的 `doc/`，根目录只保留简要入口 `README.md`；
- 新增文档必须从对应项目的 `doc/README.md` 链接；
- 路径、环境变量、端口或部署步骤变化时同步更新相关文档；
- 许可证文件不移动到 `doc/`；
- 依赖包自带文档只存在于本地 `node_modules`，不属于仓库。

## 清理命令

在确认没有未提交运行数据需要保留后，可清理本地产物：

```bash
rm -rf \
  union/source/target \
  ram/source/target \
  back/source/node_modules back/source/dist back/source/dist.next back/source/dist.previous \
  blog/source/node_modules blog/source/dist blog/source/dist.next blog/source/dist.previous blog/source/.astro \
  back/data/* blog/data/* ram/data/* union/data/*
```

不要删除 `Cargo.lock`、`package-lock.json`、`ram/source/assets/` 或许可证。

## 提交前检查

```bash
git status --short
git ls-files | rg '(^|/)(target|node_modules|dist|data)/'
git grep -n -I -E '(BEGIN (RSA |OPENSSH )?PRIVATE KEY|postgres(ql)?://[^ ]+:[^ ]+@)'
```

再执行 [开发与验证](development.md) 中的格式化、测试、构建和审计命令。构建验证结束后删除生成产物，保持仓库只包含源代码及其直接相关文件。
