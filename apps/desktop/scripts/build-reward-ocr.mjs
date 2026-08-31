import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const desktopDirectory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const resourcesDirectory = path.join(desktopDirectory, "src-tauri", "resources");

if (process.platform !== "win32") {
  console.log("Reward OCR is Windows-only; skipping its bundle on this platform.");
  process.exit(0);
}

const outputDirectory = path.join(resourcesDirectory, "reward-ocr");
const project = path.join(desktopDirectory, "..", "reward-ocr", "PlatScope.RewardOcr.csproj");
const publish = spawnSync(
  "dotnet",
  ["publish", project, "-c", "Release", "-r", "win-x64", "--self-contained", "true", "-o", outputDirectory],
  { cwd: desktopDirectory, stdio: "inherit", shell: false },
);
if (publish.status !== 0) process.exit(publish.status ?? 1);

const executable = path.join(outputDirectory, "platscope-reward-ocr.exe");
const selfTest = spawnSync(executable, ["--self-test-russian"], {
  cwd: desktopDirectory,
  stdio: "inherit",
  shell: false,
});
process.exit(selfTest.status ?? 1);
