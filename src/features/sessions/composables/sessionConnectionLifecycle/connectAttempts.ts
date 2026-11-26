import type { ProtocolConnection } from "../../../../session/protocol/protocolAdapter.ts";

export type SessionConnectAttempt = {
  signal: AbortSignal;
  isLive: () => boolean;
  assertLive: () => void;
  wait: <T>(start: () => Promise<T>) => Promise<T>;
  startCore: <T>(start: () => Promise<T>) => Promise<T>;
  connect: (start: () => Promise<ProtocolConnection>) => Promise<ProtocolConnection>;
  track: <T>(task: Promise<T>) => Promise<T>;
  defer: (action: () => void, delayMs: number) => void;
};

type Request = {
  sessionId: string;
  controller: AbortController;
  settled: boolean;
  operation: Promise<boolean>;
  completion: Promise<void>;
};
type Resource = {
  owner: Request;
  coreStart: Promise<unknown> | null;
  standaloneRelease: boolean;
  coreRemoved: boolean;
  removeCore: boolean;
  connection: ProtocolConnection | null;
  tasks: Set<Promise<unknown>>;
  timers: Set<ReturnType<typeof setTimeout>>;
  closing: Promise<void> | null;
};

/** Owns startup futures and keeps a session ID unavailable until real cleanup. */
export function createSessionConnectAttempts(options: {
  canConnect: (sessionId: string) => boolean;
  releaseSession: (sessionId: string) => Promise<void>;
  stopCore: (sessionId: string) => Promise<void>;
  disconnectConnection: (connection: ProtocolConnection) => Promise<void>;
}) {
  const requests = new Map<string, Request>();
  const resources = new Map<string, Resource>();
  let disposed = false;

  function isLive(request: Request) {
    return !disposed && options.canConnect(request.sessionId) && !request.controller.signal.aborted && requests.get(request.sessionId) === request;
  }

  function closeResource(resource: Resource, removeCore = false): Promise<void> {
    resource.removeCore ||= removeCore || (!resource.standaloneRelease && resource.connection === null);
    resource.owner.controller.abort();
    resource.timers.forEach((timer) => clearTimeout(timer));
    resource.timers.clear();
    if (resource.closing) return resource.closing;
    const closing = Promise.resolve().then(async () => {
      const sessionId = resource.owner.sessionId;
      // An external Surface must detach before a full Core stop removes it.
      await options.releaseSession(sessionId);
      if (resource.connection) await options.disconnectConnection(resource.connection);
      await resource.coreStart?.catch(() => undefined);
      if (resource.removeCore && !resource.coreRemoved) {
        await options.stopCore(sessionId);
        resource.coreRemoved = true;
      }
      // Stop Core before waiting for a pending native connect to return.
      await resource.owner.operation.catch(() => false);
      if (resource.connection) await options.disconnectConnection(resource.connection);
      await Promise.allSettled([...resource.tasks]);
      // Registration services may finish allocating local state after revocation.
      await options.releaseSession(sessionId);
      // A close can escalate a concurrent plain disconnect while it awaits.
      if (resource.removeCore && !resource.coreRemoved) {
        await options.stopCore(sessionId);
        resource.coreRemoved = true;
      }
      if (resources.get(sessionId) === resource) resources.delete(sessionId);
    });
    resource.closing = closing;
    void closing.then(() => undefined, () => {
      if (resource.closing === closing) resource.closing = null;
    });
    return closing;
  }

  function run(sessionId: string, operation: (attempt: SessionConnectAttempt) => Promise<boolean>): Promise<void> {
    if (disposed || !options.canConnect(sessionId)) return Promise.resolve();
    const previous = requests.get(sessionId);
    if (previous && !previous.settled && !previous.controller.signal.aborted) return previous.completion;
    previous?.controller.abort();
    const request: Request = {
      sessionId, controller: new AbortController(), settled: false,
      operation: Promise.resolve(false), completion: Promise.resolve(),
    };
    requests.set(sessionId, request);
    let resource: Resource | null = null;
    request.operation = Promise.resolve().then(async () => {
      if (!isLive(request)) return false;
      const previousResource = resources.get(sessionId);
      if (previousResource) await closeResource(previousResource);
      if (!isLive(request)) return false;
      resource = {
        owner: request, coreStart: null, standaloneRelease: false, coreRemoved: false,
        removeCore: false, connection: null, tasks: new Set(), timers: new Set(), closing: null,
      };
      resources.set(sessionId, resource);
      const owned = resource;
      function assertLive() {
        if (!isLive(request)) throw new Error("session_connect_cancelled");
      }
      function track<T>(task: Promise<T>): Promise<T> {
        owned.tasks.add(task);
        const release = () => { owned.tasks.delete(task); };
        void task.then(release, release);
        return task;
      }
      async function wait<T>(start: () => Promise<T>): Promise<T> {
        assertLive();
        const value = await track(Promise.resolve().then(() => { assertLive(); return start(); }));
        assertLive();
        return value;
      }
      return operation({
        signal: request.controller.signal, isLive: () => isLive(request), assertLive, wait, track,
        startCore: async (start) => {
          assertLive();
          if (owned.coreStart) throw new Error("session_core_start_already_owned");
          const task = Promise.resolve().then(() => {
            assertLive();
            return start();
          });
          owned.coreStart = task;
          const value = await task;
          assertLive();
          return value;
        },
        connect: async (start) => {
          assertLive();
          const connection = await track(start());
          owned.connection = connection;
          assertLive();
          return connection;
        },
        defer: (action, delayMs) => {
          assertLive();
          const timer = setTimeout(() => {
            owned.timers.delete(timer);
            if (isLive(request)) action();
          }, delayMs);
          owned.timers.add(timer);
        },
      });
    }).finally(() => { request.settled = true; });
    request.completion = request.operation.then(async (success) => {
      if (!success && resource) await closeResource(resource);
    }, async (error) => {
      const canceled = request.controller.signal.aborted;
      if (resource) await closeResource(resource);
      if (!canceled) throw error;
    });
    return request.completion;
  }

  async function cancel(sessionId: string, removeCore = false): Promise<boolean> {
    const request = requests.get(sessionId);
    request?.controller.abort();
    let resource = resources.get(sessionId);
    if (!resource) {
      // Cancellation can arrive before run's microtask allocates a Core owner.
      const owner = request ?? {
        sessionId, controller: new AbortController(), settled: true,
        operation: Promise.resolve(false), completion: Promise.resolve(),
      };
      resource = {
        owner, coreStart: null, standaloneRelease: true, coreRemoved: false,
        removeCore: false, connection: null, tasks: new Set(), timers: new Set(), closing: null,
      };
      resources.set(sessionId, resource);
    }
    await closeResource(resource, removeCore);
    await request?.operation.catch(() => false);
    if (removeCore) {
      if (requests.get(sessionId) === request) requests.delete(sessionId);
    }
    return removeCore || requests.get(sessionId) === request;
  }

  function dispose(): Promise<void> {
    disposed = true;
    requests.forEach((request) => request.controller.abort());
    return Promise.all([...new Set([...requests.keys(), ...resources.keys()])].map((id) => cancel(id, true))).then(() => undefined);
  }

  return { run, cancel, dispose, isDisposed: () => disposed, sessionIds: () => [...requests.keys()] };
}
