# 自动化检查与候选包门禁

`.github/workflows/pre-release.yml` 在 PR、push 或手动触发时执行。它不创建 Release、不上传产物、不配置签名凭据，也不将自动检查结果视为正式发布批准。

## 检查范围

1. 安全任务获取完整分支与标签历史（`fetch-depth: 0`），执行冻结安装、公开文件与历史检查、离线许可摘要检查、全部严重程度的 npm 审计，以及固定版本的 cargo-audit。`pnpm repository:check` 检查当前跟踪文件及 HEAD、本地分支、远端跟踪分支和标签可达的历史。它拒绝内部资料、生成文件、缓存和凭据文件，并扫描常见私钥与令牌格式。检查失败会阻断候选构建。Cargo 审计发现漏洞时失败，未维护、unsound 和撤回版本的警告继续显示。
2. 候选任务覆盖五个首发目标。macOS Apple Silicon 使用 `macos-15`，Intel 使用 `macos-15-intel`。Windows x64 和 x86 使用 `windows-2022`，ARM64 使用 `windows-11-arm`。各任务安装 Node 24、pnpm 11.5.1、Rust 1.96.0，执行格式、严格 Clippy、前端和 Rust 测试，再构建 Release。
3. 构建前清除旧的验证记录并写入当前源码的开始标记（提交、未提交状态和差异/新文件摘要）；构建或校验失败不会产生本次成功记录，构建期间源码变化也会拒绝。校验器拒绝旧/空安装包、错误 PE 架构、版本不一致或缺少预期产物。
4. macOS 检查真实 .app 的主程序/Helper、动态库、资源摘要和 ad hoc 完整性，并验证 DMG CRC 与品牌 ZIP。Windows 检查主程序和服务 PE 架构/版本，以及唯一 `-setup.exe` 的 x86 NSIS 格式；使用 7-Zip 解包，核对包内主程序的每份副本与资源摘要，并运行解包主程序的无副作用加载握手。Windows runner 打包后必须再次执行安装器回归，缺少 NSIS 时失败，不能以跳过代替通过。选择、UAC 时机、安装行为、系统信任与卸载仍须真实安装验收。

两端分别在 `src-tauri/target/<target>/release/` 写入 `candidate-verification.json` 和 `SHA256SUMS`。校验和路径相对源码仓库根目录。记录固定包含 `signing: not-verified`、`desktopAcceptance: pending` 和 `published: false`；绿色工作流仅代表这些自动门禁通过。

所有第三方 Actions 固定至已核对的完整提交；仓库权限仅为 `contents: read`，checkout 不保留凭据。未设置忽略错误、失败后继续发布或独立于前置检查的上传步骤。没有把编译产物放入远端缓存。

## 本地复核

在完整的 Git 工作区内，使用工作流声明的 Node、pnpm 和 Rust 版本运行以下检查。在线安全审计需要访问公开漏洞库。

```sh
pnpm install --frozen-lockfile
pnpm repository:check
pnpm licenses:check
pnpm audit --audit-level low
cargo audit --file src-tauri/Cargo.lock
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
pnpm test:ci
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --locked
```

`cargo audit` 使用工作流固定的 cargo-audit 0.22.2。工作流 YAML 可用 actionlint 校验；它不实际运行 Windows/macOS runner，也不替代打包与桌面验收。

准备当前平台候选包时，例如在 macOS Apple Silicon 上：

```sh
node scripts/release-candidate.mjs begin aarch64-apple-darwin
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg -- --locked
node scripts/release-candidate.mjs verify aarch64-apple-darwin
```

Windows 使用 `pnpm installer:windows <target>`，目标为 `aarch64-pc-windows-msvc`、`i686-pc-windows-msvc` 或 `x86_64-pc-windows-msvc`。脚本仅执行一次 Tauri NSIS 构建，成功后输出目录仅含一个 `-setup.exe`，失败时不保留半成品。验证器的输入目标必须与实际构建一致；失败时先读取具体检查原因，不通过重用旧包或修改质量门禁来消除错误。

构建脚本按目标架构准备微软 WebView2 离线运行时，验证微软的 Authenticode 签名，再交给 NSIS 嵌入。构建机无法直接访问微软下载地址时，可将 `LLREMOTE_WEBVIEW2_OFFLINE_DIR` 指向存放微软原始离线安装程序的目录。保留官方文件名 `MicrosoftEdgeWebView2RuntimeInstallerX86.exe`、`MicrosoftEdgeWebView2RuntimeInstallerX64.exe` 或 `MicrosoftEdgeWebView2RuntimeInstallerARM64.exe`。候选包校验会比对嵌入文件与构建时准备的文件。

Windows 的安装模式、目录和管理员授权行为见[安装说明](INSTALL.md)。候选构建校验覆盖安装包内容和回归夹具，实际桌面安装、系统授权和服务行为由运行环境决定。

从源码归档构建时，可为 `begin` 和 `verify` 都添加 `--source-mirror`。该模式对构建输入计算摘要，记录 `commit: null` 和 `dirty: null`。GitHub Actions 要求使用 Git checkout。

Windows x86 的 CI 在 x64 宿主上执行。x86 安装包要求系统原生架构为 32 位 Windows。

## 首发产物

`1.0.0` 的候选构建覆盖五个目标。发布页应列出实际分发的安装包、签名状态及 SHA-256。签名、公证或重新归档后，从最终下载文件重新计算校验和。
