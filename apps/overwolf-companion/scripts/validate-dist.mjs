import { readFile, readdir, stat } from "node:fs/promises";
import { writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";

const dist = new URL("../dist/", import.meta.url);

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const manifest = JSON.parse(await readFile(new URL("manifest.json", dist), "utf8"));
assert(manifest.manifest_version === 1, "manifest_version must be 1");
assert(manifest.type === "WebApp", "manifest type must be WebApp");
assert(manifest.meta?.name === "PlatScope Companion", "unexpected app name");
assert(/^\d+(?:\.\d+){0,3}$/u.test(manifest.meta?.version ?? ""), "invalid app version");
assert((manifest.meta?.description?.length ?? 181) <= 180, "description exceeds 180 characters");
assert(
  JSON.stringify([...manifest.permissions].sort()) === JSON.stringify(["FileSystem", "GameInfo"]),
  "permissions must contain only FileSystem and GameInfo",
);
assert(manifest.data?.game_targeting?.type === "dedicated", "game targeting must be dedicated");
assert(
  JSON.stringify(manifest.data?.game_targeting?.game_ids) === JSON.stringify([8954]),
  "only Warframe game ID 8954 is allowed",
);
const windowConfig = manifest.data?.windows?.[manifest.data?.start_window];
assert(windowConfig?.file === "index.html", "start window must load index.html");
assert(windowConfig?.desktop_only === true, "companion window must be desktop-only");
assert(windowConfig?.native_window === true, "companion must use a visible native window");
assert(windowConfig?.show_in_taskbar === true, "companion must remain identifiable in the taskbar");

const iconUrl = new URL(manifest.meta.icon, dist);
const icon = await readFile(iconUrl);
assert(icon.length <= 30 * 1024, "dock icon exceeds 30 KiB");
assert(icon.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])), "dock icon is not PNG");
assert(icon.readUInt32BE(16) === 256 && icon.readUInt32BE(20) === 256, "dock icon must be 256x256");

const html = await readFile(new URL("index.html", dist), "utf8");
assert(html.includes("connect-src 'none'"), "CSP must block outbound connections");
assert(html.includes("<main id=\"main-content\""), "main landmark is missing");
assert(html.includes("class=\"skip-link\""), "skip link is missing");

const assetsUrl = new URL("assets/", dist);
const assetNames = await readdir(assetsUrl);
const scripts = assetNames.filter((name) => name.endsWith(".js"));
assert(scripts.length === 1, "expected one bundled application script");
const script = await readFile(new URL(scripts[0], assetsUrl), "utf8");
for (const forbidden of ["fetch(", "XMLHttpRequest", "WebSocket", "sendBeacon"]) {
  assert(!script.includes(forbidden), `network primitive found in bundle: ${forbidden}`);
}

async function packageFiles(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (relative === "SHA256SUMS.txt") continue;
    const url = new URL(`${relative}`, dist);
    if (entry.isDirectory()) files.push(...await packageFiles(url, relative));
    else if (entry.isFile()) files.push(relative);
  }
  return files;
}

const packagedFiles = (await packageFiles(dist)).sort((left, right) => left.localeCompare(right));
const checksumLines = [];
for (const relative of packagedFiles) {
  const bytes = await readFile(new URL(relative, dist));
  const hash = createHash("sha256").update(bytes).digest("hex");
  checksumLines.push(`${hash} *${relative}`);
}
await writeFile(new URL("SHA256SUMS.txt", dist), `${checksumLines.join("\n")}\n`, "ascii");

const manifestSize = (await stat(new URL("manifest.json", dist))).size;
console.log(
  `Companion package verified: files=${packagedFiles.length} manifest=${manifestSize}B icon=${icon.length}B permissions=GameInfo,FileSystem network=blocked checksums=written`,
);
