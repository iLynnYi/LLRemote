import type { AgentPermissionDiagnosticItem, AgentPermissionDiagnosticStatus } from "../protocol/protocolAdapter.ts";

export type AgentPermissionDiagnosticRow = {
  kind: AgentPermissionDiagnosticItem["kind"];
  label: string;
  purpose: string;
  status: AgentPermissionDiagnosticStatus;
  statusText: string;
  actionHint: string;
};

export type AgentPermissionDiagnosticsReport = {
  target: string;
  checkedAt: string;
  rows: AgentPermissionDiagnosticRow[];
};

const CHECKS = [
  { kind: "agent_service", label: "增强服务", purpose: "为剪贴板、文件传输等增强功能提供通信通道。", granted: "已连接", missing: "连接不可用", hint: "确认远端共享服务正在运行后重新连接。" },
  { kind: "screen_recording", label: "屏幕录制权限", purpose: "允许目标端程序采集屏幕画面。", granted: "已授权", missing: "未授权", hint: "在目标电脑的系统设置中，为共享程序开启屏幕录制权限。" },
  { kind: "accessibility", label: "辅助功能权限", purpose: "允许目标端程序控制鼠标和键盘。", granted: "已授权", missing: "未授权", hint: "在目标电脑的系统设置中，为共享程序开启辅助功能权限。" },
  { kind: "clipboard", label: "剪贴板能力", purpose: "提供增强剪贴板写入命令；实际传输结果以发送操作为准。", granted: "命令可用", missing: "命令不可用", hint: "检查目标端增强服务，确认它支持剪贴板写入。" },
  { kind: "file_system", label: "文件接收目录", purpose: "为上传文件准备接收目录；实际写入结果以文件传输为准。", granted: "目录已准备", missing: "目录不可用", hint: "检查目标电脑上的接收目录及其访问权限，然后重试。" },
] satisfies Array<{ kind: AgentPermissionDiagnosticItem["kind"]; label: string; purpose: string; granted: string; missing: string; hint: string }>;

/** Explain known checks locally, without exposing raw Agent messages or implying unreported grants. */
export function summarizeAgentPermissionDiagnostics(items: AgentPermissionDiagnosticItem[]): AgentPermissionDiagnosticRow[] {
  return CHECKS.map((check) => {
    const reported = items.find((item) => item.kind === check.kind)?.status;
    const status = reported === "granted" || reported === "missing" || reported === "unsupported" ? reported : "unknown";
    return {
      kind: check.kind,
      label: check.label,
      purpose: check.purpose,
      status,
      statusText: status === "granted" ? check.granted : status === "missing" ? check.missing : status === "unsupported" ? "暂不支持" : "未确认",
      actionHint: status === "granted" ? "" : status === "missing" ? check.hint
        : status === "unsupported" ? ""
        : "未收到明确结果，请重新检查。",
    };
  });
}
