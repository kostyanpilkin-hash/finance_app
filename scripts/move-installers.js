import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");

const dateStr = new Date().toISOString().split("T")[0];
const installersDir = path.join(root, "installers", dateStr);
fs.mkdirSync(installersDir, { recursive: true });

// Cargo workspace: build output lives in <root>/target.
const bundleDir = path.join(root, "target", "release", "bundle");
const { version } = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));

for (const [sub, suffix] of [
  ["msi", ".msi"],
  ["nsis", "-setup.exe"],
]) {
  const dir = path.join(bundleDir, sub);
  if (!fs.existsSync(dir)) continue;
  for (const file of fs.readdirSync(dir).filter((f) => f.endsWith(suffix) && f.includes(version))) {
    fs.copyFileSync(path.join(dir, file), path.join(installersDir, file));
    console.log(`Copied ${file}`);
  }
}

console.log(`Installers moved to: ${installersDir}`);
