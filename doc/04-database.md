# PostgreSQL

## 连接与所有权

建议为应用创建独立角色和同名数据库：

```sql
CREATE ROLE union LOGIN PASSWORD 'replace-me';
CREATE DATABASE union OWNER union;
```

应用角色应拥有目标数据库，不应使用 PostgreSQL 超级用户运行服务。生产环境不会自动创建缺失的数据库；部署时必须预先创建。

## 启动顺序

`union` 启动时按以下顺序工作：

1. 从环境或本地私有配置读取数据库连接串并初始化主密钥；
2. 连接 PostgreSQL；
3. 获取 advisory lock，在事务中按版本执行尚未应用的 `migrations/*.sql`；
4. 保存迁移版本与 SHA-256 校验和；
5. 读取或种子化加密运行配置；
6. 初始化 ram 账号。

Union 管理员凭据不在 PostgreSQL 中，保存在权限为 `0600` 的 `data/union-config.json`；登录会话只保存在进程内存。

初始化要求目标数据库中不存在 Union 业务表。迁移器只执行 `MIGRATIONS` 清单中登记的 SQL，不推断或转换清单之外的数据结构。

已经应用的迁移文件不可修改；启动时发现校验和不一致会拒绝继续。结构变化必须新增更高版本 SQL 文件，并在 `database/mod.rs` 的迁移清单中登记。运行配置与受管主机地址、远程 RAM 实例与其地址/账号清理均使用数据库事务提交。

## 初学者理解 migrations

数据库迁移可以理解成“数据库结构的版本历史”。Git 管理 Rust、TypeScript 等源码的版本，`schema_migrations` 表则记录数据库已经走到了哪个版本。新装环境从第 1 版开始依次执行；已有环境只执行缺少的更高版本。

当前迁移流程涉及三个位置：

| 位置 | 作用 |
| --- | --- |
| `migrations/0001_baseline.sql` | 第 1 版数据库结构的 SQL 内容。 |
| `database/mod.rs` 中的 `MIGRATIONS` | 把版本号、说明和 SQL 文件登记到程序中。 |
| PostgreSQL 的 `schema_migrations` | 记录目标数据库已经执行的版本和校验和。 |

`include_str!` 会在编译时把 SQL 内容嵌入 `union` 二进制，所以生产部署不依赖服务器上是否存在 `migrations/` 目录。反过来说，只新增一个 SQL 文件而不登记到 `MIGRATIONS` 数组，程序也不会执行它。

### `0001_baseline.sql` 分段说明

按文件从上到下阅读：

1. `union_valid_host_address`：创建 PostgreSQL 校验函数，接受合法 IP 地址或域名，拒绝空值和明显无效的主机地址。
2. `managed_host_addresses`：保存各类受管主机的规范化地址；`(kind, host_id)` 联合主键表示每类主机的每个 ID 只有一条地址记录。
3. `ram_instances`：保存远程 RAM 实例。端口限制为 `1..65535`，运行期望状态只能是 `running` 或 `stopped`。
4. `services`、`service_events`：分别保存服务的当前期望状态和历史操作事件。
5. `jobs`、`job_logs`：保存博客构建等后台任务及逐行日志。删除任务时，外键的 `ON DELETE CASCADE` 会一并删除日志。
6. `blog_posts` 及三个 taxonomy 表：保存文章正文、文章标签、分类/标签字典，以及分类允许使用的标签。
7. `settings`：按 key 保存运行配置；敏感配置由应用加密后才写入 `value`。
8. `audit_logs`：记录谁在什么请求中对什么目标执行了操作。
9. `service_accounts`、`service_account_permissions`：保存 RAM 等服务的账号和路径权限；删除账号会级联删除权限。
10. `DROP TABLE sessions/users`：确保数据库中不存在这两张表。管理员凭据保存在本地私有文件中，会话只存在进程内存。
11. `CREATE INDEX`：为常用筛选和按时间排序的字段建立索引，减少数据量增长后的全表扫描。
12. 最后的 `INSERT INTO services`：写入四个基础服务；`ON CONFLICT DO NOTHING` 使重复执行不会覆盖已有状态。

`CREATE TABLE IF NOT EXISTS` 能让 SQL 对“表已经存在”较宽容，但真正保证迁移只执行一次的是 `schema_migrations` 的版本记录，不能把 `IF NOT EXISTS` 当成完整的版本管理方案。

### 正确新增一次迁移

假设需要给文章增加阅读次数：

1. 新建 `migrations/0002_add_blog_post_views.sql`；
2. 写入 `ALTER TABLE blog_posts ADD COLUMN view_count BIGINT NOT NULL DEFAULT 0;`；
3. 在 `database/mod.rs` 的 `MIGRATIONS` 末尾登记版本 `2`，并用 `include_str!` 引入新文件；
4. 在专用空测试库执行迁移测试两次，确认首次成功、再次执行不会重复变更；
5. 同时更新数据库文档、Rust 数据模型、查询和前端类型。

不要为了“让 SQL 更好看”而格式化或给已经应用的 `0001_baseline.sql` 增加注释。空格和注释也会改变 SHA-256 校验和。确实需要修正数据库时，应新增迁移；如果只是补充解释，应修改本文档。

### 事务和并发保护

迁移开始前会获取 PostgreSQL advisory lock。同一数据库上如果两个 `union` 实例同时启动，只有一个实例执行迁移，另一个等待。所有待执行 SQL 放在同一事务中：任何一步失败都会回滚，本次版本也不会写入 `schema_migrations`。这避免数据库停在“表建了一半，但版本已标记成功”的状态。

## 基线表

| 表 | 责任 |
| --- | --- |
| `schema_migrations` | 已应用的迁移版本、说明和 SHA-256 校验和。 |
| `services` | 托管服务及期望状态。 |
| `service_events` | 服务启停和状态事件。 |
| `jobs` | blog 构建等后台任务。 |
| `job_logs` | 后台任务日志行。 |
| `blog_posts` | blog 文章元数据和正文。 |
| `blog_post_tags` | 文章与标签的多对多关系。 |
| `blog_taxonomy` | 分类、标签注册表。 |
| `blog_category_tags` | 分类与允许标签关系。 |
| `settings` | 加密运行配置和 blog 首页配置。 |
| `audit_logs` | 操作人、请求 ID、目标和审计详情。 |
| `service_accounts` | ram 等服务账号及加密密码。 |
| `service_account_permissions` | 服务账号的路径权限。 |

会话、任务日志、权限等外键使用级联删除，避免遗留孤立记录。

## 清空项目数据库

以下操作不可恢复，只适用于明确要求重置的 `union` 项目库。它不会删除系统库 `postgres`：

```bash
sudo -u postgres psql -v ON_ERROR_STOP=1 -d union <<'SQL'
BEGIN;
DROP SCHEMA public CASCADE;
CREATE SCHEMA public AUTHORIZATION pg_database_owner;
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT CREATE ON SCHEMA public TO pg_database_owner;
COMMIT;
SQL
```

验证为空：

```bash
sudo -u postgres psql -d union -Atc \
  "SELECT count(*) FROM pg_tables WHERE schemaname='public'"
```

结果应为 `0`。随后启动 `union` 会重建基线表。

## 备份

仓库脚本 `scripts/backup-production.sh` 强制使用 age 加密，不允许生成明文归档。至少设置：

```bash
export BACKUP_DIR=/srv/backup/union
export UNION_DATABASE_URL='postgresql://...'
export AGE_RECIPIENT='age1...'
./scripts/backup-production.sh
```

归档包含：

- `pg_dump` custom 格式数据库；
- `data/blog/files/`；
- `data/ram/files/`；
- 生产环境文件；
- 存在时的开发主密钥文件。

生产主密钥位于环境文件中，丢失后数据库内的外部服务凭据无法解密，所以环境文件与数据库备份必须属于同一个恢复点。

## 恢复

恢复是停机操作，目标应为空数据库。先验证归档校验和，再在隔离目录中解密；不要把解密目录放在仓库或多人可读路径。

1. 停止 `union` 和 ram；
2. 解密归档到权限为 `0700` 的临时目录；
3. 重建空数据库并设置正确所有者；
4. 使用 `pg_restore --clean --if-exists --no-owner` 恢复；
5. 恢复 `data/blog/files/`、`data/ram/files/` 和环境文件；
6. 检查文件所有权；
7. 启动服务并检查 `/api/ready`；
8. 触发一次 blog 构建并检查 ram 权限。

不要把生产 dump、解密临时目录或主密钥放入仓库。

## 恢复验收

- `/api/ready` 返回成功，且管理员可以登录；
- Sunshine、Proxmox 和 ram 的已保存凭据可正常解密；
- 文章、分类、标签及博客资源数量符合备份时记录；
- ram 文件存在且路径权限与数据库规则一致；
- 重新构建博客后，Caddy 仍提供最近一次有效站点；
- 完成验收后立即清理解密文件，并记录恢复点与耗时。
