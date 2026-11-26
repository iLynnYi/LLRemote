export function isFileTransferDragEvent(event: DragEvent): boolean {
  const transfer = event.dataTransfer;
  if (!transfer) {
    return false;
  }
  return Array.from(transfer.types).some((type) => (
    type === "Files" || type === "text/uri-list"
  ));
}

export function droppedFilePaths(event: DragEvent): string[] {
  const paths = [
    ...droppedFilePathsFromFiles(event.dataTransfer?.files),
    ...droppedFilePathsFromUriList(event.dataTransfer?.getData("text/uri-list") ?? ""),
  ];
  return [...new Set(paths.filter((path) => path.length > 0))];
}

function droppedFilePathsFromFiles(files?: FileList | null): string[] {
  if (!files) {
    return [];
  }
  return Array.from(files)
    .map((file) => {
      const path = (file as File & { path?: string }).path;
      return typeof path === "string" ? path : "";
    })
    .filter(Boolean);
}

function droppedFilePathsFromUriList(uriList: string): string[] {
  return uriList
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map(filePathFromUri)
    .filter((path): path is string => Boolean(path));
}

function filePathFromUri(uri: string): string | null {
  try {
    const url = new URL(uri);
    if (url.protocol !== "file:") {
      return null;
    }
    return decodeURIComponent(url.pathname);
  } catch {
    return null;
  }
}
