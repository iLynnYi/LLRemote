# 构建与打包排障

本文涵盖许可校验、Tauri 依赖、Windows 安装器和 macOS 打包排障。涉及源码的命令从仓库根目录运行。

## 1. 先确认失败阶段

| 首条具体错误 | 失败阶段 | 优先检查 |
| --- | --- | --- |
| `Distribution notices are stale or incomplete` | 项目自己的发行许可检查 | 输入文件是否与 `licenses/inventory.json` 记录一致 |
| `Upstream notice changed` | 上游许可文本完整性检查 | 报错指出的上游文本及其固定摘要 |
| `Found version mismatched Tauri packages` | Tauri 前后端版本检查 | npm API 与 Rust crate 的实际版本 |
| `!include: could not find ...install-safety.nsh` / `install-transaction.nsh` | Windows NSIS 脚本编译 | `src-tauri/windows/` 下的辅助文件及导入路径 |
| `failed to bundle project` / `bundle_dmg.sh` 失败 | 平台打包阶段 | 该错误之前的磁盘镜像、签名或安装器日志 |

`[ELIFECYCLE] Command failed with exit code 1` 是命令失败的汇总，不能据此判断根因。应向前找到第一条具体错误。前置检查通过，只表示可以进入下一阶段；后续可能暴露另一项独立问题。

构建入口关系：

```text
macOS: pnpm tauri build ...
       → scripts/tauri.mjs
       → distribution-licenses.mjs --check
       → Tauri 版本检查、前端构建、Rust 编译、平台打包

Windows: pnpm installer:windows <target>
         → scripts/windows-installers.mjs
         → scripts/tauri.mjs build ... --bundles nsis -- --locked
         → 同一套许可检查和后续构建
```

Windows 日志里连续出现 `distribution-licenses.mjs`、`tauri.mjs` 和安装器命令失败，可能是同一次许可检查失败向外传播，并不表示出现了三个独立故障。

## 2. 许可证材料过期

### 原因

[校验脚本](../../scripts/distribution-licenses.mjs)读取以下文件，规范化换行及首尾空白后计算 SHA-256，与 `licenses/inventory.json` 中的 `inputs` 比较：

- `package.json`
- `pnpm-lock.yaml`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`
- `licenses/overrides.json`

同时检查清单 schema、包列表、`licenses/THIRD-PARTY-NOTICES.txt` 正文摘要，以及 override 引用的上游许可文本摘要。

这是文件内容检查，不会先判断改动是否真正引入新依赖。因此以下情况都会使已有材料过期：

- 新增或升级依赖，更新锁文件。
- 只调整 Rust feature，没有新增依赖包。
- 修改 `package.json` 中的脚本、版本或其他内容。
- 生成后再次修改输入文件。

清单覆盖 macOS 的 ARM64、x64 和 Windows 的 ARM64、x86、x64 Rust 依赖图，以及前端生产依赖。Windows feature 改动也会影响 macOS 构建前的摘要检查。

脚本已经将 CRLF 规范化为 LF；仅 Windows/macOS 换行差异不应导致失败，不需要为此批量转换仓库换行。

### 如何确认

先单独运行，不必反复启动完整安装包构建：

```sh
pnpm licenses:check
```

校验脚本会列出具体差异，例如：

```text
Distribution notices are stale or incomplete:
  - src-tauri/Cargo.toml: changed since license materials were generated
```

有 Git 的工作区可辅助检查本地改动：

```sh
git status --short
git diff -- package.json pnpm-lock.yaml src-tauri/Cargo.toml src-tauri/Cargo.lock licenses/
```

`git diff` 用来查看未提交改动，不能替代摘要检查；已经提交的文件也可能与尚未更新的清单不一致。

### 如何修复

先确认输入文件的改动符合预期。若前端依赖尚未按锁文件安装，先执行：

```sh
pnpm install --frozen-lockfile
```

冻结安装失败时，先检查依赖声明与锁文件是否同步；不要为了消除提示直接删除锁文件。主动更新依赖时，应先完成相应安装或锁文件更新，再生成许可材料。

```sh
pnpm licenses:generate
git diff -- licenses/
pnpm licenses:check
```

生成器会重新收集当前锁定依赖图的许可原文，写入 `licenses/inventory.json` 和 `licenses/THIRD-PARTY-NOTICES.txt`。如果依赖图与正文没有变化，最终可能只有清单中的输入摘要改变，这是正常结果。

生成需要 Node、pnpm、Cargo 和相应依赖缓存，可能访问网络；`licenses:check` 本身不需要网络。生成失败时，先处理具体错误，不要把失败的生成命令当成清单已经更新。

若提示缺少许可声明或正文、非 registry Rust 来源、上游文本摘要改变，应按 [发行许可材料说明](../../licenses/README.md)核对来源和 override。不要手改摘要、填写空许可或关闭检查来制造通过结果。

## 3. Tauri 前后端版本不匹配

### 原因与确认

例如，以下报错表示 Rust 和 JavaScript 的 Tauri 次版本不同。

```text
tauri (v2.12.5) : @tauri-apps/api (v2.11.1)
```

Rust 和 JavaScript 依赖分别由 `Cargo.lock`、`pnpm-lock.yaml` 锁定。即使声明都允许版本 `2`，各自实际解析出的次版本仍可能不同。

Tauri 要求 `tauri` Rust crate 与 `@tauri-apps/api` 保持相同主、次版本；对应插件的 npm 包和 Rust crate 则应保持完整版本一致。详见 [Tauri 官方依赖同步说明](https://v2.tauri.app/develop/updating-dependencies/#sync-npm-packages-and-cargo-crates-versions)。不要据此把 CLI、tauri-build 和所有插件统一改成同一个版本号。

检查实际安装版本及环境：

```sh
pnpm list @tauri-apps/api @tauri-apps/cli @tauri-apps/plugin-dialog @tauri-apps/plugin-clipboard-manager --depth 0
pnpm exec tauri info
```

同时核对 `pnpm-lock.yaml` 的根项目依赖版本，以及 `src-tauri/Cargo.lock` 中 `name = "tauri"` 条目紧随其后的版本。依赖声明表示允许范围，不能作为实际安装版本的证明。

### 如何修复

按 `Cargo.lock` 锁定的 Rust 次版本选择前端 API。以上面的报错为例，保留 Rust `tauri 2.12.5` 时可以安装同属 `2.12` 的前端 API。

```sh
pnpm add "@tauri-apps/api@^2.12.4"
```

更新 API 会修改 `package.json` 和 `pnpm-lock.yaml`，所以必须再次生成许可材料：

```sh
pnpm licenses:generate
git diff -- package.json pnpm-lock.yaml licenses/
pnpm licenses:check
```

先遇到许可错误，修复后遇到版本错误，再更新依赖后需要重新生成许可材料，是两道独立检查依次生效的结果。

## 4. 重新构建

提交依赖变更时，一并提交锁文件和更新后的 `licenses/` 材料。在完整源码目录中执行以下检查。

```sh
pnpm install --frozen-lockfile
pnpm licenses:check
```

检查通过后，选择当前平台对应的构建命令。

Windows x64：

```powershell
pnpm installer:windows x86_64-pc-windows-msvc
```

macOS Apple Silicon：

```sh
pnpm tauri build --target aarch64-apple-darwin --bundles dmg
```

## Windows NSIS 辅助文件引用失败

若 Rust 已显示 `Finished release`，随后 `makensis` 在 `service-hooks.nsh` 第 1、2 行提示找不到 `install-safety.nsh` 或 `install-transaction.nsh`，失败发生在安装脚本编译阶段，尚未生成新安装包。先检查完整的源码目录，不应将此错误归为目标 CPU 架构、运行时 UAC 或 Rust 编译失败。

即使文件存在，混用路径分隔符也会报同样的错误。Windows NSIS 的 `get_dir_name/get_file_name` 只按原生 `\` 拆分，`${__FILEDIR__}/install-safety.nsh` 会把 `windows/install-safety.nsh` 当成文件名。引用必须使用：

```nsis
!include "${__FILEDIR__}\install-safety.nsh"
!include "${__FILEDIR__}\install-transaction.nsh"
```

Windows 路径规则回归和 NSIS 编译检查均覆盖这两条引用。

`${__FILEDIR__}` 会从 hooks 文件所在的 `src-tauri/windows/` 目录定位辅助文件。构建需要以下文件。

- `src-tauri/windows/installer.nsi`
- `src-tauri/windows/service-hooks.nsh`
- `src-tauri/windows/install-safety.nsh`
- `src-tauri/windows/install-transaction.nsh`

先检查安装脚本是否齐全。

```sh
pnpm installer:windows --check
```

打包入口也会在 Rust/Tauri 构建前执行同一检查，列出缺失、空白或无法读取的脚本。预检通过后重新执行目标架构的打包命令。


## Windows 安装包双击后无向导：System.dll 崩溃

如果安装包在 `.onInit` 预检中退出，退出状态是 `0xC0000005`，且 Windows 应用事件记录指向 NSIS 的 `System.dll`，先检查预检中的原生 API 调用。

最小 NSIS 用例将故障定位到 `GetProcAddress` 返回地址的调用：System 插件要求 `::ADDR` 表示原生函数地址，原先的 `$1(...)` 会把地址当作插件自身的过程描述读取。应使用 `System::Call '::$1(...)'`。两处动态调用均需采用该语法。

另一个预检错误是输出参数：`*i0.r2` 没有按预期读回值。初始化为零并读回 `$2` 的写法应为 `*i0r2`；`IsWow64Process2` 写入 USHORT，必须确保输出槽高位为零。否则即使不崩溃，也可能把原生 ARM64 或 x64 模拟误判为不兼容。日志追加写入同时显式 `FileSeek ... END`，保留整个检查过程。

回归 `windowsCompatibility.test.ts` 直接抽取生产预检函数，在 Windows 上编译并执行三个架构用例，验证实际系统架构；当前策略只放行原生系统与包架构完全一致的组合，模拟运行不作为安装放行条件；macOS 只完成编译，不把它算作 Windows 执行结果。

仅用已有 Rust 产物重打 NSIS 时，`makensis` 的中间产物是 `release/nsis/<arch>/nsis-output.exe`；还需放回 `release/bundle/nsis/` 的正式文件名并核对摘要，不能因为编译成功就继续测试旧包。

### Windows 解压 `legal/THIRD-PARTY-NOTICES.txt` 失败

错误生成的 NSIS 脚本可能含 `CreateDirectory "$PLUGINSDIR\payload{{this}}"`。原因是自定义 Handlebars 模板使用单个反斜杠紧邻表达式，导致表达式被当作字面量，而不是创建真实资源父目录。随后 `File /oname=legal\...` 写入不存在的目录，产生 NSIS 默认“中止/重试/忽略”，以及后续 `payload_extraction_failed`。这不能归因为 CPU 架构或管理员权限。

资源与外部程序现在统一通过 `ExtractInstallerFile` 宏传递源路径和相对目标，在 NSIS 宏内拼接路径并逐文件创建父目录，立即检查每次操作。`SetOverwrite try` 避免默认重试窗口，任何必需文件失败都进入统一错误处理并退出。完整解压发生在中断旧应用/共享之前。`windowsPayload.test.ts` 执行生产宏，覆盖嵌套资源、父路径被文件占用、目标文件被目录占用；仍需在 Windows 上运行，macOS 编译不等于 Windows 实测。

## macOS DMG 图标和 ZIP

`pnpm tauri build` 和 `pnpm tauri bundle` 会给新生成的 DMG 文件设置软件图标，并在同目录生成 `.dmg.zip`。DMG 文件图标依赖 macOS 文件元数据，直接分发 `.dmg` 可能丢失图标。若要保留文件图标，可分发 `.dmg.zip` 并通过 Finder 解压。挂载后的磁盘和其中的应用使用各自的包内图标。

以下命令为已有 DMG 补设图标并生成 ZIP。

```bash
pnpm installer:icons "src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/LL Remote_1.0.0_aarch64.dmg"
```

## macOS 共享后台签名与授权历史

在已安装应用的 Mac 上运行以下命令，查看当前共享后台的签名以及上次成功检测到授权时保存的签名哈希。

```bash
service_app="/Applications/LL Remote.app/Contents/Helpers/LLRemote Service.app"
history_file="$HOME/Library/Application Support/LLRemote/Sharing/permission-identities.json"

codesign -dv --verbose=4 "$service_app" 2>&1 | grep -E '^(Identifier|CDHash|TeamIdentifier)='
printf '上次录屏授权 CDHash: '
plutil -extract screen_recording.cdhash raw -o - "$history_file"
printf '上次辅助功能授权 CDHash: '
plutil -extract accessibility.cdhash raw -o - "$history_file"
```

将 `codesign` 输出的 `CDHash` 与两项历史值分别比较。某项不同表示该权限上次成功检测时对应另一版共享后台。可用 `codesign -d -r- "$service_app"` 查看 ad hoc 签名的版本限定要求。

`permission-identities.json` 保存上次成功检测的签名。文件不存在或某项为空时，表示没有可比较的成功记录。重新构建使用 ad hoc 签名的应用通常会改变 `CDHash`。请在“本机共享”点击“重新检测”检查当前权限。点击“授权”会清除对应旧记录并重新发起系统授权请求。

共享后台数据位于当前用户的 `~/Library/Application Support/LLRemote/Sharing/`。
