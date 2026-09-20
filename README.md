# workbuddy-auto-signin-rs

`workbuddy-auto-signin` 的 Rust 重构版。目标是保持原 Python 脚本的业务语义，同时提供单一原生二进制、结构化模块、可测试 HTTP 层和跨平台定时运行模板。

> 上游实现基线：[`88lin/workbuddy-auto-signin@2b05ef0`](https://github.com/88lin/workbuddy-auto-signin/commit/2b05ef0112319b9e9e3a8021757320371d0f88a9)（2026-09-19）。原项目 MIT License；本仓库保留原版权与许可声明。

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
- 抽奖每轮最多一次；补登每轮最多一张卡，保持上游安全策略。

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

响应采用“稳定内部类型 + 宽容 `serde_json::Value`”策略，兼容 `data/result/resp/response` 信封、数字字符串和逆向接口的小幅结构变化。

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

复制 `workbuddy-auto-signin.plist.example` 到 `~/Library/LaunchAgents/`，将 `/PATH/TO/workbuddy-auto-signin` 替换为 release 二进制绝对路径后加载。

### Linux

`systemd/` 提供 user service/timer 示例。把 `%h/.local/bin/workbuddy-auto-signin` 改为你的实际路径即可。

## 测试

```bash
cargo test --all-targets
cargo check --all-targets
```

GitHub Actions 会在 Linux、Windows、macOS 三个平台编译并运行测试。

## 关键兼容约束

1. `silent-growth` 必须继续可用。
2. 无参数等价 `auto`。
3. 两个 Billing POST 允许网络/5xx 重试，其余 Growth 写请求不自动重试。
4. 网络和 5xx 各自维护独立退避进度。
5. 重试必须服从总时间预算。
6. `/redeem` 主参数为字符串档位 `7d/14d/28d`。
7. `403 天数不足` 必须先于通用 401/403 登录失效判断。
8. Lottery 每轮最多一次不可逆 draw。
9. Makeup 每轮最多消耗一张卡。
10. `silent*` 模式尽最大努力把结果落盘。

## 安全说明

该程序会读取本机 WorkBuddy / CodeBuddy 登录会话并持有 Bearer token，所以**二进制和源码本身属于凭据安全边界**。当前实现只向会话里的 endpoint（缺省 `https://copilot.tencent.com`）发送请求，不打印 Authorization header。

## 免责声明

这是非官方个人自动化工具。相关接口来自客户端行为分析，可能随服务端版本变化而失效。请自行评估并遵守对应服务条款。
