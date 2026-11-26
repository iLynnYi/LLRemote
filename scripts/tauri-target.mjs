// Tauri's bundler must receive Cargo's inherited target explicitly as well.
export function tauriBuildArgs(args, inheritedTarget) {
  const boundary = args.includes("--") ? args.indexOf("--") : args.length;
  const options = args.slice(1, boundary);
  if (!inheritedTarget || !["build", "bundle"].includes(args[0])
    || options.some((arg) => ["--help", "-h", "--target", "-t"].includes(arg)
      || arg.startsWith("--target=") || /^-t.+/.test(arg))) return args;
  return [...args.slice(0, boundary), "--target", inheritedTarget, ...args.slice(boundary)];
}

export function tauriBuildTarget(args) {
  const boundary = args.includes("--") ? args.indexOf("--") : args.length;
  for (let index = 1; index < boundary; index += 1) {
    const arg = args[index];
    if (arg === "--target" || arg === "-t") return args[index + 1];
    if (arg.startsWith("--target=")) return arg.slice("--target=".length);
    if (/^-t.+/.test(arg)) return arg.slice(2).replace(/^=/, "");
  }
}
