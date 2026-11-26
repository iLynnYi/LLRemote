/** Resolve the native picker at the service boundary so every upload entry
 * presents localized categories rather than native exception details. */
export async function selectFileTransferSources(
  invoke: (command: "select_file_transfer_sources") => Promise<unknown>,
  t: (key: string) => string,
): Promise<string[]> {
  try {
    const paths = await invoke("select_file_transfer_sources");
    if (!Array.isArray(paths) || !paths.every((path): path is string => typeof path === "string")) throw new Error();
    return paths;
  } catch (error) {
    const invalidPath = typeof error === "object" && error !== null && "category" in error
      && error.category === "sourcePicker" && "code" in error && error.code === "invalidPath";
    throw new Error(t(invalidPath ? "sessions.fileTransfers.sourcePathInvalid" : "sessions.fileTransfers.sourcePickerUnavailable"));
  }
}
export type FileTransferSource = { filePath: string; relativePath?: string | null };

type SourceScanCommand = (
  command: "start_file_transfer_source_scan" | "next_file_transfer_source_page" | "cancel_file_transfer_source_scan",
  args: Record<string, unknown>,
) => Promise<unknown>;

/** Keep each native-to-WebView response bounded while traversing large folders. */
export async function expandFileTransferSourcesInPages(invoke: SourceScanCommand, paths: string[]): Promise<FileTransferSource[]> {
  const scanId = await invoke("start_file_transfer_source_scan", { paths });
  if (typeof scanId !== "string") throw new Error("file_transfer_scan_invalid");
  const sources: FileTransferSource[] = [];
  let done = false;
  try {
    while (!done) {
      const page = await invoke("next_file_transfer_source_page", { scanId });
      if (!page || typeof page !== "object" || !("sources" in page) || !Array.isArray(page.sources)
        || !("done" in page) || typeof page.done !== "boolean"
        || !page.sources.every(source => source && typeof source === "object" && typeof source.filePath === "string")) {
        throw new Error("file_transfer_scan_invalid");
      }
      sources.push(...page.sources as FileTransferSource[]);
      done = page.done;
    }
    return sources;
  } finally {
    if (!done) await invoke("cancel_file_transfer_source_scan", { scanId }).catch(() => undefined);
  }
}
