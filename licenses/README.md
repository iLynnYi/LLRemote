# 发行许可材料

项目自身遵循根目录 `LICENSE` 的 MIT License；第三方组件分别遵循其各自许可，不能将项目许可套用到这些组件。

`THIRD-PARTY-NOTICES.txt` 为随包、离线可读的声明和完整许可正文；`inventory.json` 保存版本、目标依赖图、来源、文本引用及输入文件摘要。Rust 清单覆盖 macOS Intel、Apple Silicon 和 Windows ARM64、x86、x64 的锁定元数据图（包含构建依赖），前端覆盖生产依赖图，属于保守并集，并非每个二进制的链接证明。

MPL-2.0 组件未作本地源码修改，声明中给出了精确版本的 crates.io 源码归档地址。操作系统自带框架、系统 WebView 和开发工具本身不属于该应用源码归档。

部分发布包缺少正文，`overrides.json` 与 `upstream/` 保留固定上游提交/标签的正文、来源和 SHA-256。`selectors` 保留原文件 MPL 声明，并附完整 MPL；图标额外保留 Ionic 原始声明。`@vicons/ionicons5` 和 `vdirs` 没有发布完整版权通知，保留原始包标注的作者、MIT 声明和明确标注的 SPDX 标准模板补充，未捏造版权年份或持有人。这种来源记录不能冒充上游提供了不存在的版权通知。

更新依赖后，在已安装 Node/pnpm 和 Cargo 的源码工作区运行：

```sh
pnpm licenses:generate
pnpm licenses:check
```

生成器从当前锁定图和本机已下载的包收集原文；Cargo 可能补取公开元数据和依赖，pnpm 需要访问自己的包索引。缺少声明/正文、非 registry Rust 来源或上游文本摘要不符会失败，不能用空许可占位。新增缺正文依赖时，人工核对来源后补充 override，不要随意套用其他项目的版权声明。

`licenses:check` 离线核对锁文件/清单与许可材料，依赖变更后未重新生成会失败；构建前运行该门禁。不要在日志、库存文件或许可正文中加入本机绝对路径或凭据。
