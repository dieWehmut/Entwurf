export function chooseFiles(accept: string, multiple = false): Promise<File[]> {
  return new Promise(resolve => {
    const input = document.createElement("input");
    input.type = "file"; input.accept = accept; input.multiple = multiple;
    input.hidden = true;
    let finished = false;
    const finish = (files: File[]) => { if (finished) return; finished = true; input.remove(); resolve(files); };
    input.addEventListener("change", () => finish(Array.from(input.files ?? [])), { once: true });
    input.addEventListener("cancel", () => finish([]), { once: true });
    document.body.append(input);
    input.click();
  });
}
export async function chooseJson(): Promise<string | null> {
  const [file] = await chooseFiles(".json,application/json");
  if (!file) return null;
  if (file.size > 16 * 1024 * 1024) throw new Error("JSON 文件超过 16 MiB。请选择较小文件。");
  return file.text();
}
export function download(bytes: Uint8Array | string, name: string, type = "application/octet-stream") {
  const body = typeof bytes === "string" ? bytes : new Uint8Array(bytes).buffer;
  const url = URL.createObjectURL(new Blob([body], { type }));
  const link = document.createElement("a"); link.href = url; link.download = name; link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
export const sourceAccept = ".pdf,.md,.markdown,.tex,.latex,.png,.jpg,.jpeg,.webp,.bmp,.tif,.tiff";
