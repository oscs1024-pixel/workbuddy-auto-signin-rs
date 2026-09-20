# workbuddy-auto-signin-rs

`workbuddy-auto-signin-rs` 是一个本地运行的 WorkBuddy 自动签到与成长中心工具。项目采用 Rust 原生实现，提供单一原生二进制、结构化模块、可测试 HTTP 层和跨平台定时运行模板。

## 工程结构

```text
workbuddy-auto-signin-rs/
├── .github/
│   └── workflows/
│       └── ci.yml
├── .gitignore
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── scripts/
│   └── install-windows.ps1
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
│   ├── instance_lock.rs
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
    ├── auth_session.rs
    ├── single_instance.rs
    ├── cli_process.rs
    ├── golden_compat.rs
    ├── reference_differential.rs
    ├── schema_drift.rs
    ├── fixtures/
    │   ├── golden_scenarios.json
    │   └── reference_differential.json
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
- 普通交互命令输出分组摘要；`status/claim/all` 和 `WORKBUDDY_OUTPUT=json` 输出纯 JSON；`silent*` 文件日志为“时间戳 + 单行 JSON payload”。
- Buddy 旅行中使用服务端 `arrive_at - server_now` 显示 `HH:MM:SS` 倒计时，例如 `旅行倒计时 02:50:56`；不依赖本机时钟。
- 进程启动时获取跨平台排他文件锁；已有实例运行时返回 `BUSY` 并跳过本轮，避免计划任务补跑与手工执行并发触发不可逆 Growth 写操作。
- 旅行配置读取失败会进入 Growth 失败统计，不再被误判为 idle；Growth 写请求会优先保留服务端 `msg` 或客户端 `error` 详情。
- Growth 主要状态机入口对最低必要 schema 做校验：HTTP 200 但关键字段消失/类型错误会记录 `schema_mismatches`，避免大部分契约漂移静默退化；具体兼容回落见“已知边界”。
- soft failure 与 hard failure 分离：4xx/schema drift 会阻止 `idle=true` 并留下日志，但只有 5xx、网络失败、预算耗尽等 hard failure 才影响整体退出码。
- silent 日志按“显式 `WORKBUDDY_SIGNIN_LOG` → 二进制目录 → 用户 cache 目录 → stderr”逐级兜底，并自动创建缺失的父目录。
- 凭据 JSON 中显式的 `auth: null` / `account: null` 按空对象处理，最终归一为 `NO_SESSION`，不会误报成 JSON 格式损坏。

## 实现范围与审计结论

当前代码按“CLI → service → api → http”分层，业务路径已覆盖签到与成长中心的全部 18 个 endpoint pattern。2026-09-20 基于业务代码提交 `da2ba77ba5c0530dd33a354bbcac487481b7558c` 再次完成全量只读审计，并与已固定的参考契约提交 `2b05ef0112319b9e9e3a8021757320371d0f88a9` 逐项核对。最新 CI run `35488744424` 已通过 Linux / Windows / macOS 的格式、Clippy、编译和测试，Rust 1.89.0 MSRV 校验、四套 release 构建以及 rolling `latest` 发布也全部成功。

当前结论是：**核心业务实现已经完整，没有发现缺失的 endpoint、主状态机断链或需要恢复的旧实现；仓库历史残留已基本清理干净。** 此前发现的旅行配置错误传播、网络错误详情、`auth/account:null`、跨进程单实例锁、最低 schema 检测、soft/hard failure 语义拆分、silent 日志兜底、MSRV 和 reference-differential 基线均已落地。剩余事项主要属于“接口继续漂移时是否足够 fail-closed”以及“自动验证覆盖强度”问题，而不是功能模块缺失。

关键行为：

- Billing 两个 POST 允许网络/5xx 重试；Growth 写操作默认不自动重试，避免超时后的重复副作用。
- 旅行状态机中的 `travel/config` 是必要读步骤；最终网络/5xx 失败会进入 hard failure 统计，不能再被 silent-poll 当成空跑吞掉。
- Growth 写接口错误优先显示响应 `msg`，网络/超时则保留客户端 `error`，最后才回退到 HTTP/伪状态码。
- Growth 固定按“旅行 → 任务 → 补登 → 兑换 → 抽奖 → Buddy → 状态汇总”执行，避免业务依赖被并发打乱。
- 兑换只在明确 unknown/unsupported/invalid tier 时从 `7d/14d/28d` 回退数字天数；403 连登天数不足按业务常态处理。
- 补登限制的是“实际消耗一张卡”，陈旧的无需补登日期不会占用该额度。
- 轮询只在“签到已完成/活动未开启 + Growth 真正 idle”时静默；hard failure、读接口 soft 4xx 和 schema mismatch 都会阻止 idle，从而不会被“无可处理项目”掩盖。
- macOS、Linux、Windows 都区分 00:05 主签到（`silent`）和 01/05/09/13/17/21 轮询（`silent-poll`），预算与日志语义保持一致。
- 所有命令进入业务逻辑前都会获取同一排他文件锁；第二个进程返回 `BUSY` 并正常退出，锁随进程退出由操作系统释放，不依赖删除 lock 文件。

CI 使用 mock API 验证关键接口契约和状态机，并在 Linux、Windows、macOS 上执行格式、Clippy、编译、测试和 release 构建；额外使用 Rust 1.89.0 执行 MSRV `cargo check --locked --all-targets`，确保 `rust-version = "1.89"` 不是仅文档声明。当前行为级回归覆盖旅行 config 硬失败、旅行领奖失败不 depart、任务 20 条分批与 results 缺失回落、补登每轮最多实际消耗一张卡、兑换 unknown-tier 数字 fallback 且使用新 client token、403 locked 优先级、抽奖一轮一次、retry 预算截断、null 会话字段、CLI 无参数兼容、跨平台单实例锁、schema drift 与日志路径回落。

`tests/fixtures/golden_scenarios.json` 固化稳定 helper/output 契约；`tests/fixtures/reference_differential.json` 绑定已验证参考提交 `2b05ef0112319b9e9e3a8021757320371d0f88a9`，通过 mock transcript 对比代表性请求顺序和关键输出子集。仓库不复制参考 Python 源文件，因此 differential harness 不会重新引入历史实现残留。其覆盖范围是“代表性 differential + 各模块独立状态机测试”，不是对全部 18 个 endpoint 的穷举差分。仓库提交 `Cargo.lock`，CI/Release 全部使用 `--locked`，避免依赖解析随时间漂移。

### 已知边界

- CI 不持有真实 WorkBuddy 登录凭据，因此不会对生产账号执行签到、补登、兑换、抽奖等写操作；服务端若改版，仍需以实际响应为准。
- Growth 活动接口属于变化较频繁的契约，因此稳定会话字段采用强类型，活动响应继续使用宽容 JSON 解析。当前最低 schema 检查已经覆盖主要状态机入口，但**不是完整 JSON Schema 校验**，仍保留少量兼容性回落。
- `tasks/accept` 的 `results[].status` 当前只有明确 `error` 才按失败处理；如果未来服务端新增未知状态字符串，现实现会把它归入成功分支。后续若进一步收紧，应只允许已验证成功状态，其余状态记录 schema mismatch。
- 旅行状态为 `idle` 且未达每日上限时，`travel/config.locations = []` 当前会被视为“没有可派地点”并继续整轮流程，而不是 schema mismatch。若服务端契约确认该场景下 locations 必须非空，可进一步改成 fail-closed。
- `buddy/quota.max_open_count` 缺失时按已验证兼容规则回落到 1；字段存在但无法解析时当前会记录 schema mismatch，同时仍以 1 继续。这能保持兼容性，但从最严格的不可逆写操作策略看，后续可考虑在“字段存在但非法”时直接跳过 open。
- Billing 签到状态接口目前没有像 Growth 一样做最低 schema mismatch 统计；HTTP 2xx 但缺少 `active/today_checked_in` 时会继续尝试幂等的 `daily-checkin`。因为领取接口自身按当天幂等，所以不会造成重复领取，但异常可观察性仍可继续加强。
- 能量和最终连签天数属于**纯展示值**：与参考行为一致，汇总阶段的 `/energy` 或二次 `/streak` 查询失败、预算不足或字段缺失不会计入 Growth failure，也不会改变主流程结果。
- `tests/fixtures/reference_differential.json` 当前固定并自动验证“已签到”和“Growth idle 请求序列”两类代表性 transcript；其它关键写状态机由独立 wiremock 测试覆盖，但还不是“18 个 endpoint 每种状态全部跑一遍”的全量 differential harness。
- Reference differential fixture 固定在已验证参考提交；参考实现若出现新提交，需要重新审计接口与 fixture，而不是自动追随最新代码。

### 历史残留检查

本轮重新检查完整 Git tree 与默认分支代码搜索，没有发现需要删除的明显历史垃圾文件或未完成标记：

- 不存在旧 `signin.py`、Python/pythonw 运行入口或 Python 依赖。
- 不存在早期平铺的重复 auth/http/growth 实现，也没有已经不用的 `model/billing.rs` / `model/growth.rs`。
- 不存在重复的 systemd `*.example`、空 fixture 占位文件或旧安装脚本副本。
- 默认分支没有 `TODO` / `FIXME` / `deprecated` / `legacy` / `temporary` 等显式未完成标记。
- `silent-growth` 是有意保留的兼容命令名，不属于废代码。
- Linux 旧凭据路径探测是迁移兼容 fallback，不属于残留实现。
- `tests/fixtures/reference_differential.json` 中的固定参考提交信息是差异回归测试的来源元数据，不是运行时代码。
- `LICENSE` 中的许可与版权归属属于法律保留内容，不应作为“旧项目标识”删除。

## 构建

要求 Rust 1.89 或更高版本。项目使用 Rust 标准库从 1.89 起提供的跨平台文件锁 API。

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

普通 `auto` / `growth` 默认输出分组后的易读摘要；`status` / `claim` / `all` 调试命令输出纯 JSON。需要脚本解析 `auto` / `growth` 时，可设置 `WORKBUDDY_OUTPUT=json`。计划任务的 `silent*` 不写 stdout，文件日志格式为 `[YYYY-MM-DD HH:MM:SS] {JSON payload}`。

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
| `WORKBUDDY_SIGNIN_LOG` | silent 模式首选日志路径；不可写时自动回落 |
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
cargo clippy --locked --all-targets -- -D warnings
cargo check --locked --all-targets
cargo test --locked --all-targets
```

GitHub Actions 会在 Linux、Windows、macOS 三个平台执行以上检查，单独用 Rust 1.89.0 验证 MSRV，并额外构建 Linux x64、Windows x64、macOS Intel、macOS Apple Silicon release 产物。

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
12. 同一用户环境同时只允许一个实例进入业务执行；并发实例返回 `BUSY`，不继续发送 API 请求。
13. `auth/account` 字段缺失或显式为 `null` 时都按空对象处理，缺少 token/uid 最终归为 `NO_SESSION`。
14. 旅行配置读取的 hard failure 必须计入 Growth failures，不能误判为 idle。
15. Growth 关键 2xx 响应缺少最低必要字段时必须记录 schema mismatch，不能静默回落为空数据。
16. 读接口 soft 4xx 会阻止 `idle=true`，但不提升为 hard failure；已知业务常态仍在具体模块中显式识别。
17. Buddy 旅行倒计时必须基于服务端时间差，并保持 `HH:MM:SS` 格式。
18. silent 日志至少尝试显式路径、二进制目录、用户 cache 三处，全部失败后写 stderr。
19. CI 必须用 Rust 1.89.0 验证 MSRV。

## 安全说明

该程序会读取本机 WorkBuddy / CodeBuddy 登录会话并持有 Bearer token，所以**二进制和源码本身属于凭据安全边界**。当前实现只向会话里的 endpoint（缺省 `https://copilot.tencent.com`）发送请求，不打印 Authorization header。

## 免责声明

这是非官方个人自动化工具。相关接口来自客户端行为分析，可能随服务端版本变化而失效。请自行评估并遵守对应服务条款。
