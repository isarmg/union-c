# UnionC Agent

`unionc-agent` 是一个只读、零入站端口的跨平台主机遥测程序。它采集 CPU、内存、
网络、磁盘、温度和可用的 GPU 指标，将完整 capability/快照上报给 UnionC，并可选
同步发送标准 OTLP/HTTP Protobuf 指标。

## 安全边界

- 不监听任何端口，不实现远程命令、脚本、配置下发或自更新。
- NVIDIA 只调用 NVML query，Linux AMD/Intel 只读取 sysfs/hwmon。
- 缺少驱动、权限或平台 API 时上报 capability 和 N/A，不提升整个进程权限。
- 本地只写稳定 `host-id`、私有注册 proof、每主机 token 和有大小上限的断线 spool。

## 构建和验证

```bash
cargo test --manifest-path unionc/agent/Cargo.toml
cargo build --release --manifest-path unionc/agent/Cargo.toml
```

默认启用 `nvidia` 和 `otlp`。不需要 NVIDIA 时可以进一步缩小依赖：

```bash
cargo build --release --manifest-path unionc/agent/Cargo.toml \
  --no-default-features --features otlp
```

## 运行

复制 `config.example.json`。首次启动填写部署级 `enrollment_token`；Agent 只用它调用一次
`/api/agent/v1/register`，随后把服务端签发的主机独立 token 以 `agent-token` 保存到状态目录。
也可以通过 `token` 直接预配主机 token：

```bash
unionc-agent probe --config /etc/unionc-agent/config.json
unionc-agent once --config /etc/unionc-agent/config.json
unionc-agent run --config /etc/unionc-agent/config.json
```

`probe` 只在标准输出打印本机快照和 capability，不连接服务端。环境变量可以覆盖常用
字段：`UNIONC_AGENT_ENDPOINT`、`UNIONC_AGENT_REGISTRATION_ENDPOINT`、
`UNIONC_AGENT_TOKEN`、`UNIONC_AGENT_ENROLLMENT_TOKEN`、
`UNIONC_AGENT_OTLP_ENDPOINT`、`UNIONC_AGENT_OTLP_TOKEN`、
`UNIONC_AGENT_HOST_ID`、`UNIONC_AGENT_STATE_DIR`、
`UNIONC_AGENT_INTERVAL_SECONDS`、`UNIONC_AGENT_SLOW_INTERVAL_SECONDS`。
未指定状态目录时使用 Linux `/var/lib/unionc-agent`、Windows
`%ProgramData%\UnionC Agent` 或 macOS `/Library/Application Support/UnionC Agent`。

如入口采用 mTLS，Linux 可把客户端证书和私钥合并成一个 PEM，并设置
`tls_identity_pem`；Windows/macOS 原生证书栈使用 `tls_identity_pkcs12` 和对应密码。
私有 CA 使用 `tls_ca_pem`。配置、主机 token 和证书文件必须只允许服务账户读取；注册
完成后应从配置/环境中移除部署级 enrollment token。Agent 会在第一次网络请求前生成
`enrollment-secret`；它使注册响应丢失后的重试保持幂等，同时阻止仅持共享部署 token 的
其他主机接管已有 `host-id`。不要复制或重用另一台机器的整个状态目录。
除回环地址外，Agent 默认拒绝明文 HTTP；生产入口应使用 HTTPS，确有隔离内网需求时
才显式设置 `allow_insecure_http`。

## 平台能力

| 平台 | 基线 | GPU/温度 |
|---|---|---|
| Linux | CPU、内存、网络、磁盘 | hwmon；NVML；AMD/Intel DRM sysfs |
| Windows | CPU、内存、网络、磁盘、可用 ACPI thermal zone | NVIDIA NVML；其他 GPU 当前明确显示 capability gap |
| macOS | CPU、内存、网络、磁盘、sysinfo 可读传感器 | 公共稳定 API 不提供整机 Apple/AMD/Intel GPU 利用率，明确显示 N/A |

Windows/macOS 的私有传感器接口以及需要管理员权限的查询不会作为正式能力启用。
主机卡的网络和磁盘概要取当前最忙单接口/单设备的速率，避免 veth/bridge、bind mount
重复计算；详情页仍展示每个接口和挂载项。

## 打包

- Linux：先用 `cargo build --release --target <RUST_TARGET>` 构建，再设置 `VERSION`、
  `RUST_TARGET`、`NFPM_ARCH` 并执行 `nfpm package -f packaging/nfpm.yaml -p deb`（或
  `rpm`）。包内含专用用户和加固后的 systemd unit。
- Windows：管理员 PowerShell 执行
  `packaging/windows/install.ps1 -Binary .\unionc-agent.exe -Config .\config.json`；安装器
  使用 LOCAL SERVICE、私有 ACL、无限执行时长并在升级前停止旧任务。只有明确需要覆盖
  现有配置时才加 `-ReplaceConfig`。
- macOS：在 `unionc/agent` 目录设置 `BINARY`、`VERSION` 后执行
  `packaging/macos/build-pkg.sh`，生成带专用隐藏账户和 launchd plist 的 pkg；正式发布
  二进制建议用 `--no-default-features --features otlp` 构建；发布前必须完成 Developer ID
  签名、Hardened Runtime 和 notarization。

Linux 基础 unit 使用 `PrivateDevices=yes`，适合不采集 GPU 的主机。需要 GPU 时，确认
本机存在 `render`、`video` 组后，再安装 `packaging/linux/unionc-agent-gpu.conf` 作为
systemd drop-in；不要授予 `CAP_SYS_ADMIN`、`CAP_SYS_RAWIO` 或 root。

## 设计参考

采样生命周期和跨平台基线参考 [sysinfo](https://docs.rs/sysinfo/latest/sysinfo/)；Linux
设备/挂载过滤与 hwmon 语义参考 [Prometheus node_exporter](https://github.com/prometheus/node_exporter)；
NVIDIA 查询使用 [nvml-wrapper](https://docs.rs/nvml-wrapper/latest/nvml_wrapper/device/struct.Device.html)，
不调用其 setter；指标命名、资源属性和网关链路参考
[OpenTelemetry hostmetrics](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/receiver/hostmetricsreceiver/README.md)
及 [VictoriaMetrics OTLP 集成](https://docs.victoriametrics.com/victoriametrics/data-ingestion/opentelemetry-collector/)。
