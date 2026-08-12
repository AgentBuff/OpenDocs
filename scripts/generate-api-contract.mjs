import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { generatedFiles } from "./api-contract-source.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
for (const [file, value] of Object.entries(generatedFiles())) {
  const target = resolve(root, file);
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
