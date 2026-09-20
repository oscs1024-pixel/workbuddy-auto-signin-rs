# workbuddy-auto-signin-rs

`workbuddy-auto-signin-rs` 是一个本地运行的 WorkBuddy 自动签到与成长中心工具。项目采用 Rust 原生实现，提供单一原生二进制、结构化模块、可测试 HTTP 层和跨平台定时运行模板。

## 工程结构

```text
workbuddy-auto-signin-rs/
├── Cargo.toml
├── workbuddy-auto-signin.plist.example
├── workbuddy-growth-poll.plist.example
├── systemd/
│   ├── workbuddy-auto-signin.service
│   ├── workbuddy-auto-signin.timer
│   ├── workbuddy-growth-poll.service
│   └── workbuddy-growth-poll.timer
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── app.rs
│   ├── config.rs
│   ├── error.rs
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── discovery.rs
│   │   ├── session.rs
│   │   └── headers.rs
│   ├── http/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   ├── retry.rs
│   │   └── response.rs
│   ├── budget/mod.rs
│   ├── api/
│   │   ├── mod.rs
│   │   ├── billing.rs
│   │   └── growth.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── common.rs
│   │   └── auth.rs
│   ├── service/
│   │   ├── mod.rs
│   │   ├── signin.rs
│   │   ├── daily.rs
│   │   └── growth/
│   │       ├── mod.rs
│   │       ├── context.rs
│   │       ├── travel.rs
│   │       ├── tasks.rs
│   │       ├── makeup.rs
│   │       ├── redeem.rs
│   │       ├── lottery.rs
│   │       ├── buddy.rs
│   │       └── summary.rs
│   ├── output/
│   │   ├── mod.rs
│   │   ├── reporter.rs
│   │   └── json.rs
│   └── util/
│       ├── mod.rs
│       ├── json.rs
│       ├── number.rs
│       ├── env.rs
│       └── time.rs
└── tests/
    ├── auth_discovery.rs
    ├── http_retry.rs
    ├── signin_flow.rs
    ├── daily_flow.rs
    ├── growth_travel.rs
    ├── growth_tasks.rs
    ├── growth_makeup.rs
    ├── growth_redeem.rs
    ├── growth_lottery.rs
    ├── growth_buddy.rs
    ├── growth_flow.rs
    └── cli_compat.rs
```


## 已实现能力

- 自动探测 Windows / macOS WorkBuddy 桌面端凭据，以及 Linux CodeBuddy CLI 凭据。
- `auto`：签到 + 成长中心完整日常。
- `silent`：同 `auto`，结果写 `signin.log`。
- `growth`：只运行成长中心。
- `silent-poll`：补签 + 成长中心；空跑默认不写日志。
- `silent-growth`：兼容旧命令名，行为等同 `silent-poll`。
- `status` / `claim` / `all`：输出原始签到接口结果用于排错。
- 网络故障按 `5/15/30/60/90s` 退避；5xx 按 `3/10s` 退避。
- 普通写操作不自动重试；两个幂等签到 POST 例外。
- 总运行时间预算：普通命令默认 420s、上限 540s；轮询默认 180s、上限 240s。
- 成长中心：旅行领奖/派出、任务接取/领奖、断登补登、连登兑换、抽奖、Buddy 盲盒、能量/连签汇总。
- `/redeem` 使用最新已验证契约：`tier = "7d" | "14d" | "28d"`；只有明确 unknown-tier 参数错误时才回退到数字天数。
- `403 连登天数不足` 作为业务常态处理，不误判为登录失效。
- 抽奖每轮最多一次；补登每轮最多消耗一张卡，降低不可逆写操作的风险。
- 补登候选列表若包含服务端陈旧的“无需补登”日期，会跳过该日期继续检查下一项，不会长期阻塞真正待补日期。
- 普通交互命令输出分组摘要；silent/调试模式保持 JSON，机器解析可用 `WORKBUDDY_OUTPUT=json`。

## 实现范围与审计结论

当前代码按“CLI → service → api → http”分层，业务路径已覆盖签到与成长中心的全部 18 个 endpoint pattern。2026-09-20 的代码审计重点核对了凭据发现、预算与重试、签到幂等、旅行、任务、补登、连登兑换、抽奖、Buddy、输出和三平台调度模板，并清理了未参与运行逻辑的早期模型骨架与空 fixture 占位文件。

关键行为：

- Billing 两个 POST 允许网络/5xx 重试；Growth 写操作默认不自动重试，避免超时后的重复副作用。
- Growth 固定按“旅行 → 任务 → 补登 → 兑换 → 抽奖 → Buddy → 状态汇总”执行，避免业务依赖被并发打乱。
- 兑换只在明确 unknown/unsupported/invalid tier 时从 `7d/14d/28d` 回退数字天数；403 连登天数不足按业务常态处理。
- 补登限制的是“实际消耗一张卡”，陈旧的无需补登日期不会占用该额度。
- 轮询只在“签到已完成/活动未开启 + Growth 真正 idle”时静默；网络、登录失效和实际失败不会被“无可处理项目”掩盖。
- macOS、Linux、Windows 都区分 00:05 主签到（`silent`）和 01/05/09/13/17/21 轮询（`silent-poll`），预算与日志语义保持一致。

CI 使用 mock API 验证契约与状态机，并在 Linux、Windows、macOS 上执行格式、Clippy、编译、测试和 release 构建。CI 不持有真实 WorkBuddy 登录凭据，因此真实线上接口若发生服务端改版，仍需以实际响应为准。

## 构建

```bash
cargo build --release
```

二进制：

```text
target/release/workbuddy-auto-signin      # macOS / Linux
target/release/workbuddy-auto-signin.exe  # Windows
```

## 使用

```bash
workbuddy-auto-signin auto
workbuddy-auto-signin silent
workbuddy-auto-signin growth
workbuddy-auto-signin silent-poll
workbuddy-auto-signin silent-growth
workbuddy-auto-signin status
workbuddy-auto-signin claim
workbuddy-auto-signin all
```

不传参数时默认执行 `auto`。

普通 `auto` / `growth` 默认输出分组后的易读摘要；`silent*` 日志以及 `status` / `claim` / `all` 调试命令仍保持 JSON。需要脚本解析 `auto` / `growth` 时，可设置 `WORKBUDDY_OUTPUT=json`。

## 凭据探测

按以下顺序查找：

1. `WORKBUDDY_AUTH_FILE`
2. Windows `%LOCALAPPDATA%/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`
3. macOS `~/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info`
4. Linux `$XDG_DATA_HOME/CodeBuddyExtension/Data/Public/auth/Tencent-Cloud.coding-copilot.info`
5. Linux `~/.config/.../workbuddy-desktop.info` 兼容路径
6. `~/.workbuddy/auth/workbuddy-desktop.info`

请求头复用本机会话中的 `accessToken`、`uid`、可选 `enterpriseId` / `domain`。程序不会主动输出 Bearer token。

## 环境变量

| 环境变量 | 作用 |
| --- | --- |
| `WORKBUDDY_AUTH_FILE` | 手动指定凭据文件 |
| `WORKBUDDY_SIGNIN_LOG` | silent 模式日志路径 |
| `WORKBUDDY_BUDGET_SECONDS` | 覆盖单轮网络预算；非法值会回落并输出 `config_warning` |
| `WORKBUDDY_GROWTH_LOG_EMPTY` | `1/true/yes/on` 时轮询空跑也写日志 |
| `WORKBUDDY_OUTPUT` | 设为 `json` 时，普通交互命令也输出 JSON |

## API 契约

当前实现覆盖 18 个 endpoint pattern：

### Billing

```text
POST /v2/billing/meter/checkin-activity-status
POST /v2/billing/meter/daily-checkin
```

### Growth Center

```text
GET  /v2/activity/growth/buddy/travel/status
POST /v2/activity/growth/buddy/travel/claim
GET  /v2/activity/growth/buddy/travel/config
POST /v2/activity/growth/buddy/travel/depart
GET  /v2/activity/growth/tasks
POST /v2/activity/growth/tasks/accept
POST /v2/activity/growth/tasks/{task_code}/claim
GET  /v2/activity/growth/streak
POST /v2/activity/growth/makeup-cards/use
GET  /v2/activity/growth/redeem/summary
POST /v2/activity/growth/redeem
GET  /v2/activity/growth/lottery/chances
POST /v2/activity/growth/lottery/draw
GET  /v2/activity/growth/buddy/quota
POST /v2/activity/growth/buddy/open
GET  /v2/activity/growth/energy
```

本机会话等稳定字段使用强类型模型；变化较频繁的活动 API 响应保持宽容 `serde_json::Value` 解析，兼容 `data/result/resp/response` 信封、数字字符串和小幅结构变化。

## 系统定时

### Windows

先把 release 二进制放在固定目录，然后在仓库目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1
```

创建：

- `WorkBuddyAutoSignin`：每天 00:05，执行 `silent`
- `WorkBuddyGrowthPoll`：每天 01/05/09/13/17/21 点，执行 `silent-poll`

脚本直接运行 Rust `.exe`，不再依赖 Python/pythonw。

### macOS

提供两份 LaunchAgent 模板：

- `workbuddy-auto-signin.plist.example`：每天 00:05 执行 `silent`。
- `workbuddy-growth-poll.plist.example`：登录后补跑，并在 01/05/09/13/17/21 点执行 `silent-poll`。

把两份文件里的 `/PATH/TO/workbuddy-auto-signin` 改为 release 二进制绝对路径，再复制到 `~/Library/LaunchAgents/` 后按系统方式加载。两类任务分开后，主签到使用普通预算，轮询使用较短预算并保留空跑静默语义。

### Linux

`systemd/` 提供两组 user service/timer：

- `workbuddy-auto-signin.*`：00:05 执行 `silent`。
- `workbuddy-growth-poll.*`：01/05/09/13/17/21 点执行 `silent-poll`。

默认二进制路径为 `%h/.local/bin/workbuddy-auto-signin`；安装位置不同则修改两个 service 的 `ExecStart`。

```bash
mkdir -p ~/.config/systemd/user
cp systemd/workbuddy-auto-signin.{service,timer} ~/.config/systemd/user/
cp systemd/workbuddy-growth-poll.{service,timer} ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now workbuddy-auto-signin.timer workbuddy-growth-poll.timer
```

## Release

CI 通过后会自动生成可直接运行的原生二进制：

- 推送到 `main`：更新滚动的 `latest` 预发布，适合直接获取最新构建。
- 推送 `v*` 标签（例如 `v0.1.0`）：创建对应正式 GitHub Release。
- 自动发布 Linux x64、Windows x64、macOS Intel、macOS Apple Silicon 四套产物。
- 每个压缩包同时附带 `.sha256` 校验文件。

Release 资产命名：

```text
workbuddy-auto-signin-x86_64-unknown-linux-gnu.tar.gz
workbuddy-auto-signin-x86_64-pc-windows-msvc.zip
workbuddy-auto-signin-x86_64-apple-darwin.tar.gz
workbuddy-auto-signin-aarch64-apple-darwin.tar.gz
```

## 测试与质量门禁

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo check --all-targets
cargo test --all-targets
```

GitHub Actions 会在 Linux、Windows、macOS 三个平台执行以上检查，并额外构建 Linux x64、Windows x64、macOS Intel、macOS Apple Silicon release 产物。

## 关键兼容约束

1. `silent-growth` 必须继续可用。
2. 无参数等价 `auto`。
3. 两个 Billing POST 允许网络/5xx 重试，其余 Growth 写请求不自动重试。
4. 网络和 5xx 各自维护独立退避进度。
5. 重试必须服从总时间预算。
6. `/redeem` 主参数为字符串档位 `7d/14d/28d`。
7. `403 天数不足` 必须先于通用 401/403 登录失效判断。
8. Lottery 每轮最多一次不可逆 draw。
9. Makeup 每轮最多实际消耗一张卡；“无需补登”的陈旧日期不占用该额度。
10. `growth` 单独运行时即使提前失败，也必须使用“成长中心”语境输出。
11. `silent*` 模式尽最大努力把结果落盘。

## 安全说明

该程序会读取本机 WorkBuddy / CodeBuddy 登录会话并持有 Bearer token，所以**二进制和源码本身属于凭据安全边界**。当前实现只向会话里的 endpoint（缺省 `https://copilot.tencent.com`）发送请求，不打印 Authorization header。

## 免责声明

这是非官方个人自动化工具。相关接口来自客户端行为分析，可能随服务端版本变化而失效。请自行评估并遵守对应服务条款。
