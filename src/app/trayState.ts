import type {
  LocalVncServerActiveConnection,
  LocalVncServerOverview,
  LocalVncServerStatus,
} from "@server/localShare";

export type TrayStatus = "idle" | "sharing" | "sharingActive" | "error";

export type TrayIconState =
  | "idle"
  | "sharing"
  | "sharing-active-1"
  | "sharing-active-2"
  | "sharing-active-3"
  | "sharing-active-9-plus"
  | "error";

export type TrayLocalShareAction =
  | "idle"
  | "starting"
  | "stopping"
  | "awaiting_permission"
  | "awaiting_password"
  | "busy";

export type TrayStateInput = {
  localVncServerOverview: LocalVncServerOverview | null;
  activeSessionCount: number;
  failedSessionCount: number;
  canStartLocalShare: boolean;
  localShareAction?: TrayLocalShareAction;
};

export type TrayLocalConnection = {
  id: string;
  label: string;
  detail: string;
};

export type TrayStateSnapshot = {
  status: TrayStatus;
  iconState: TrayIconState;
  localShareRunning: boolean;
  localShareStatus: LocalVncServerStatus;
  activeLocalConnectionCount: number;
  activeSessionCount: number;
  failedSessionCount: number;
  port?: number;
  tooltip: string;
  menuSummary: string[];
  activeLocalConnections: TrayLocalConnection[];
  canStartLocalShare: boolean;
  localShareAction: TrayLocalShareAction;
};

export type TrayNotificationKind =
  | "local-share-connection"
  | "local-share-start-failed"
  | "local-share-stopped-unexpectedly"
  | "local-share-permission-missing"
  | "local-share-port-conflict";

export type TrayNotification = {
  kind: TrayNotificationKind;
  title: string;
  body: string;
};

export type TrayNotificationOptions = {
  intentionalStop: boolean;
};

export type RefreshLocalShareTrayStateOptions = {
  refreshLocalVncServerStatus: () => Promise<void>;
  syncTrayState: () => Promise<void>;
};

const ERROR_STATUSES: LocalVncServerStatus[] = [
  "permission_missing",
  "port_conflict",
  "failed",
];

const STATUS_LABELS: Record<LocalVncServerStatus, string> = {
  stopped: "未启动",
  starting: "启动中",
  running: "已启动",
  stopping: "停止中",
  permission_missing: "权限缺失",
  port_conflict: "端口占用",
  failed: "启动失败",
  unknown: "状态未知",
};

export function createTrayStateSnapshot(input: TrayStateInput): TrayStateSnapshot {
  const overview = input.localVncServerOverview;
  const localShareStatus = overview?.status ?? "stopped";
  const localShareRunning = localShareStatus === "running";
  const activeLocalConnectionCount = overview?.activeConnections ?? 0;
  const status = resolveTrayStatus(localShareStatus, activeLocalConnectionCount);
  const iconState = resolveTrayIconState(status, activeLocalConnectionCount);
  const menuSummary = createTrayMenuSummary({
    localShareStatus,
    activeLocalConnectionCount,
    activeSessionCount: input.activeSessionCount,
    port: overview?.port,
  });
  const localShareAction = input.localShareAction && input.localShareAction !== "idle"
    ? input.localShareAction
    : localShareStatus === "starting" || localShareStatus === "stopping"
      ? localShareStatus
      : "idle";
  const actionLabels: Record<Exclude<TrayLocalShareAction, "idle">, string> = {
    starting: "正在启动本机共享…",
    stopping: "正在停止本机共享…",
    awaiting_permission: "等待权限确认",
    awaiting_password: "等待输入密码",
    busy: "正在处理本机共享…",
  };
  if (localShareAction !== "idle" && localShareAction !== localShareStatus) {
    menuSummary.push(actionLabels[localShareAction]);
  }

  return {
    status,
    iconState,
    localShareRunning,
    localShareStatus,
    activeLocalConnectionCount,
    activeSessionCount: input.activeSessionCount,
    failedSessionCount: input.failedSessionCount,
    port: overview?.port,
    tooltip: createTrayTooltip(menuSummary),
    menuSummary,
    activeLocalConnections: (overview?.activeConnectionDevices ?? []).map(createTrayLocalConnection),
    canStartLocalShare: input.canStartLocalShare,
    localShareAction,
  };
}

export function createTrayStartFailureNotification(message: string): TrayNotification {
  return {
    kind: "local-share-start-failed",
    title: "本机共享启动失败",
    body: message.trim() || "请打开本机共享查看配置和权限状态。",
  };
}

export function getTrayNotifications(
  previous: TrayStateSnapshot | null,
  next: TrayStateSnapshot,
  options: TrayNotificationOptions,
): TrayNotification[] {
  if (!previous) {
    return [];
  }

  const notifications: TrayNotification[] = [];
  if (next.activeLocalConnectionCount > previous.activeLocalConnectionCount) {
    const newestConnection = next.activeLocalConnections.find((connection) =>
      !previous.activeLocalConnections.some((previousConnection) => previousConnection.id === connection.id),
    ) ?? next.activeLocalConnections[0];

    notifications.push({
      kind: "local-share-connection",
      title: "新设备已连接",
      body: `${newestConnection?.label ?? "新设备"} 已连接本机共享。`,
    });
  }

  if (
    previous.localShareStatus === "running"
    && next.localShareStatus === "stopped"
    && !options.intentionalStop
  ) {
    notifications.push({
      kind: "local-share-stopped-unexpectedly",
      title: "本机共享已停止",
      body: "本机共享服务已停止，请打开本机共享查看状态。",
    });
  }

  if (previous.localShareStatus !== next.localShareStatus) {
    if (next.localShareStatus === "permission_missing") {
      notifications.push({
        kind: "local-share-permission-missing",
        title: "本机共享需要权限",
        body: "屏幕录制或辅助功能权限缺失，请打开本机共享处理。",
      });
    }

    if (next.localShareStatus === "port_conflict") {
      notifications.push({
        kind: "local-share-port-conflict",
        title: "本机共享端口占用",
        body: "当前监听端口被占用，请打开本机共享调整端口或停止占用进程。",
      });
    }
  }

  return notifications;
}

export async function refreshLocalShareTrayState(options: RefreshLocalShareTrayStateOptions): Promise<void> {
  await options.refreshLocalVncServerStatus();
  await options.syncTrayState();
}

function resolveTrayStatus(
  localShareStatus: LocalVncServerStatus,
  activeLocalConnectionCount: number,
): TrayStatus {
  if (ERROR_STATUSES.includes(localShareStatus)) {
    return "error";
  }

  if (localShareStatus === "running" && activeLocalConnectionCount > 0) {
    return "sharingActive";
  }

  if (localShareStatus === "running") {
    return "sharing";
  }

  return "idle";
}

function resolveTrayIconState(status: TrayStatus, activeLocalConnectionCount: number): TrayIconState {
  if (status === "error") {
    return "error";
  }

  if (status === "sharing") {
    return "sharing";
  }

  if (status !== "sharingActive") {
    return "idle";
  }

  if (activeLocalConnectionCount <= 1) {
    return "sharing-active-1";
  }

  if (activeLocalConnectionCount === 2) {
    return "sharing-active-2";
  }

  if (activeLocalConnectionCount === 3) {
    return "sharing-active-3";
  }

  return "sharing-active-9-plus";
}

function createTrayMenuSummary(input: {
  localShareStatus: LocalVncServerStatus;
  activeLocalConnectionCount: number;
  activeSessionCount: number;
  port?: number;
}): string[] {
  if (ERROR_STATUSES.includes(input.localShareStatus)) {
    return [
      `! 本机共享：${STATUS_LABELS[input.localShareStatus]}`,
      "需要处理",
    ];
  }

  if (input.localShareStatus !== "running") {
    return [
      `○ 本机共享：${STATUS_LABELS[input.localShareStatus]}`,
      ...(input.activeSessionCount > 0 ? [`远程会话：${input.activeSessionCount} 个`] : []),
    ];
  }

  return [
    "● 本机共享：已启动",
    `活动连接：${input.activeLocalConnectionCount} 台`,
    ...(input.port ? [`监听端口：${input.port}`] : []),
    ...(input.activeSessionCount > 0 ? [`远程会话：${input.activeSessionCount} 个`] : []),
  ];
}

function createTrayTooltip(summary: string[]): string {
  return ["LL Remote", ...summary].join("\n");
}

function createTrayLocalConnection(connection: LocalVncServerActiveConnection): TrayLocalConnection {
  const label = connection.deviceName?.trim()
    || connection.ipAddress?.trim()
    || connection.id;

  return {
    id: connection.id,
    label,
    detail: connection.ipAddress?.trim() || "未知地址",
  };
}
