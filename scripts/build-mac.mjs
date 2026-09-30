import { spawnSync } from "node:child_process";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
function run(command, args, capture = false) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: process.env,
    encoding: "utf8",
    stdio: capture ? "pipe" : "inherit",
  });
  if (result.error || result.status !== 0) {
    throw new Error(
      `${command} failed: ${result.error?.message ?? result.stderr ?? result.status}`,
    );
  }
  return `${result.stdout ?? ""}${result.stderr ?? ""}`;
}

try {
  if (process.platform !== "darwin")
    throw new Error("Mac 构建请在 macOS 上执行。");
  // An Apple-issued identity keeps the local-network permission tied to the app across updates.
  // Never silently fall back to the linker's per-build ad-hoc identity.
  const identities = [
    ...run(
      "security",
      ["find-identity", "-v", "-p", "codesigning"],
      true,
    ).matchAll(
      /\b([A-F0-9]{40}) "((?:Developer ID Application|Apple Development):[^"\n]+)"/g,
    ),
  ];
  const requested = process.env.APPLE_SIGNING_IDENTITY;
  const available = requested
    ? identities.filter(
        (identity) => identity[1] === requested || identity[2] === requested,
      )
    : identities;
  if (available.length !== 1) {
    throw new Error(
      "需要一个有效的 Apple 签名证书；有多个证书时请用 APPLE_SIGNING_IDENTITY 指定固定证书。可运行 security find-identity -v -p codesigning 查看。",
    );
  }
  process.env.APPLE_SIGNING_IDENTITY = available[0][2];
  console.log(`Signing: ${available[0][2]}`);
  run(process.execPath, [
    "node_modules/@tauri-apps/cli/tauri.js",
    "build",
    "--target",
    "aarch64-apple-darwin",
    "--bundles",
    "app",
    "--",
    "--locked",
  ]);
  const bundle = join(
    root,
    "src-tauri/target/aarch64-apple-darwin/release/bundle",
  );
  const app = join(bundle, "macos/PCMessage.app");
  run("codesign", ["--verify", "--strict", app]);
  const requirement = run("codesign", ["-d", "-r-", app], true);
  if (
    !requirement.includes('identifier "com.pcmessage.desktop"') ||
    !requirement.includes("anchor apple")
  ) {
    throw new Error("应用没有使用固定的 Apple 签名身份，停止生成安装包。");
  }
  const version = JSON.parse(
    readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
  ).version;
  const dmg = join(bundle, "dmg", `PCMessage_${version}_aarch64.dmg`);
  const staging = mkdtempSync(join(tmpdir(), "pcmessage-dmg-"));
  try {
    cpSync(app, join(staging, "PCMessage.app"), { recursive: true });
    symlinkSync("/Applications", join(staging, "Applications"));
    mkdirSync(dirname(dmg), { recursive: true });
    // Plain drag-to-Applications image; no Finder automation or GUI scripting dependency.
    run("hdiutil", [
      "create",
      "-ov",
      "-volname",
      "PCMessage",
      "-srcfolder",
      staging,
      "-format",
      "UDZO",
      dmg,
    ]);
    run("hdiutil", ["verify", dmg]);
    console.log(`Mac installer: ${dmg}`);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
