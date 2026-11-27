# 自动化检查与候选包门禁

`.github/workflows/pre-release.yml` 在 PR、push 或手动触发时执行，构建并验证五个目标的候选包。每个目标通过校验后，会把安装包上传到本次运行的 **Artifacts**。正式发布时另行核对签名状态和桌面验收结果。

## 检查范围

1. macOS Apple Silicon 使用 `macos-15`，Intel 使用 `macos-15-intel`。Windows x64 和 x86 使用 `windows-2022`，ARM64 使用 `windows-11-arm`。各任务使用 Node 24、pnpm 11.5.1 和 Rust 1.96.0，按锁文件安装依赖。打包命令执行许可证清单检查、前端类型检查和构建，以及 Rust Release 编译。
2. 构建前清除旧的验证记录并写入当前源码的开始标记（提交、未提交状态和差异/新文件摘要）。校验要求源码保持一致，安装包是本次生成的非空文件，且版本和架构符合目标。
3. macOS 检查 `.app` 的主程序、Helper、动态库、资源摘要和 ad hoc 签名完整性，并验证 DMG CRC 和品牌 ZIP。Windows 打包后执行 NSIS 安装器回归，核对主程序和服务程序的版本与架构，用 7-Zip 解包核对包内文件，并运行主程序的加载握手。选择、UAC 时机、安装行为、系统信任与卸载仍须真实安装验收。
4. 各目标通过校验后分别上传安装包。macOS 上传 `.dmg` 和 `.dmg.zip`，Windows 上传 `-setup.exe`。Artifact 名称包含目标和运行尝试编号。

两端分别在 `src-tauri/target/<target>/release/` 写入 `candidate-verification.json` 和 `SHA256SUMS`。校验和路径相对源码仓库根目录。记录固定包含 `signing: not-verified`、`desktopAcceptance: pending` 和 `published: false`；绿色工作流仅代表这些自动门禁通过。

打包工作流的权限为 `contents: read`。各 Action 固定至已核对的完整提交。

## 本地复核

仓库扫描、依赖漏洞审计、格式、Clippy 和完整测试可在本地补跑。使用工作流声明的 Node、pnpm 和 Rust 版本。在线安全审计需要访问公开漏洞库。

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

工作流 YAML 可用 actionlint 校验，打包和桌面验收仍需在相应平台执行。

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

## 准备发布草稿

在 **Actions → Pre-release checks** 选中默认分支上已成功的构建，从运行网址 `/actions/runs/<Run ID>` 复制末尾数字。随后到 **Actions → Prepare release draft → Run workflow**，在默认分支上填入该 Run ID。

工作流从所选构建的提交读取 `src-tauri/tauri.conf.json` 的版本，将 `1.0.1` 这样的版本转成 `v1.0.1` 标签。准备下一版本时，先更新项目版本并完成一次新的打包运行。同一次运行如有重试，每个平台取该次运行里最新的有效 Artifact。

五个平台的安装包会连同 `SHA256SUMS` 上传到对应版本的 Release 草稿。运行摘要会给出草稿链接。已发布的版本和其他构建的草稿会阻止创建。同一次构建的草稿如果上传中断，重新运行可继续上传缺少的附件。

安装验收后，在 **Releases** 打开草稿，核对发布说明、安装包、签名状态和校验值，点击 **Publish release**。
