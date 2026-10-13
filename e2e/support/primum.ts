// A board for each test: two temporary projects with Turkish fixture items, a
// temporary XDG_CONFIG_HOME listing them, and `primum serve` on a free port.
import { once } from "node:events";
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { get } from "node:http";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn, type ChildProcess } from "node:child_process";
import { test as base } from "@playwright/test";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const READY_TIMEOUT_MS = 15_000;

export type Board = {
  baseURL: string;
  /** The first project's folder, `işler`. */
  root: string;
  /** An item file's text, by id. */
  item: (id: string) => Promise<string>;
};

async function binaryPath(): Promise<string> {
  const binary = process.env.PRIMUM_E2E_BIN ?? join(REPO_ROOT, "target/debug/primum");
  try {
    await access(binary);
  } catch {
    throw new Error(`primum binary not found at ${binary}; run make build-dev first`);
  }
  return binary;
}

function output(stream: NodeJS.ReadableStream): () => string {
  let text = "";
  stream.setEncoding("utf8");
  stream.on("data", (chunk: string) => { text = `${text}${chunk}`.slice(-16_000); });
  return () => text;
}

async function run(binary: string, args: string[], cwd: string, env: NodeJS.ProcessEnv): Promise<void> {
  const child = spawn(binary, args, { cwd, env, stdio: ["ignore", "pipe", "pipe"] });
  const stderr = output(child.stderr);
  const [code] = await once(child, "exit") as [number | null];
  if (code !== 0) throw new Error(`primum ${args.join(" ")} exited ${code}: ${stderr()}`);
}

async function freePort(): Promise<number> {
  return new Promise((resolvePort, reject) => {
    const listener = createServer();
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => {
      const address = listener.address();
      if (!address || typeof address === "string") return reject(new Error("no free port"));
      listener.close((error) => error ? reject(error) : resolvePort(address.port));
    });
  });
}

function respondsOk(url: string): Promise<boolean> {
  return new Promise((done) => {
    const req = get(url, (res) => { res.resume(); done((res.statusCode ?? 500) < 400); });
    req.setTimeout(1_000, () => req.destroy());
    req.once("error", () => done(false));
  });
}

async function makeBoard(dir: string, binary: string): Promise<{ root: string; env: NodeJS.ProcessEnv }> {
  const env = { ...process.env, XDG_CONFIG_HOME: join(dir, "config"), NO_COLOR: "1" };
  const root = join(dir, "işler");
  const other = join(dir, "ödev");
  await mkdir(root);
  await mkdir(other);
  const p = (...args: string[]) => run(binary, args, root, env);
  await p("init");
  await writeFile(join(dir, "body.md"), "## Neden\n\nGökçe galeriyi açınca **boş** sayfa görüyor.\n");
  await p("add", "BUG", "Gökçe'nin galerisi boş açılıyor", "--impact", "8", "--urgency", "7", "--body-file", join(dir, "body.md"));
  await p("add", "TODO", "Özgür için CSV dışa aktarımı", "--needs", "BUG:1");
  await p("add", "KILLER", "Işıl'ın harita görünümü", "--impact", "9", "--effort", "7");
  await p("add", "DEBT", "Eski ayar dosyasını kaldır");
  await writeFile(join(dir, "spec.md"), "# Galeri spec\n\n| alan | tür |\n|---|---|\n| ad | metin |\n");
  await p("doc", "BUG:1", "spec", "--file", join(dir, "spec.md"));
  await writeFile(join(dir, "plan.md"), "# Galeri planı\n\n1. önce\n2. sonra\n");
  await p("doc", "BUG:1", "plan", "--file", join(dir, "plan.md"));
  await run(binary, ["init"], other, env);
  await run(binary, ["add", "URGENT", "Ödev teslimi"], other, env);
  await p("set", "BUG:1", "--effort", "4");
  await makeConflict(dir, binary, root, env);
  return { root, env };
}

/// A second machine (its own config) clones işler through a bare remote;
/// both change KILLER:1's urgency and sync, which leaves a conflict on
/// this machine. Sync is manual, so nothing else syncs during the tests.
async function makeConflict(dir: string, binary: string, root: string, env: NodeJS.ProcessEnv): Promise<void> {
  const remote = join(dir, "remote.git");
  await run("git", ["init", "--quiet", "--bare", "-b", "main", remote], dir, env);
  const p = (...args: string[]) => run(binary, args, root, env);
  await p("remote", "set", remote, "--manual");
  await p("sync");
  const env2 = { ...env, XDG_CONFIG_HOME: join(dir, "config2") };
  await run(binary, ["clone", remote, "kopya"], dir, env2);
  const q = (...args: string[]) => run(binary, args, join(dir, "kopya"), env2);
  await q("remote", "set", remote, "--manual");
  await q("set", "KILLER:1", "--urgency", "2");
  await q("sync");
  await p("set", "KILLER:1", "--urgency", "8");
  await p("sync");
}

export const test = base.extend<{ board: Board }>({
  board: async ({}, use) => {
    const binary = await binaryPath();
    const dir = await mkdtemp(join(tmpdir(), "primum-e2e-"));
    let child: ChildProcess | undefined;
    try {
      const { root, env } = await makeBoard(dir, binary);
      const port = await freePort();
      child = spawn(binary, ["serve", "--port", String(port), "--no-open"], { cwd: root, env, stdio: ["ignore", "pipe", "pipe"] });
      const stderr = output(child.stderr!);
      output(child.stdout!);
      const baseURL = `http://127.0.0.1:${port}`;
      const deadline = Date.now() + READY_TIMEOUT_MS;
      while (!(await respondsOk(`${baseURL}/static/md.js`))) {
        if (child.exitCode !== null) throw new Error(`primum serve exited ${child.exitCode}: ${stderr()}`);
        if (Date.now() > deadline) throw new Error(`primum serve did not start: ${stderr()}`);
        await new Promise((r) => setTimeout(r, 100));
      }
      const item = (id: string) => readFile(join(root, "items", `${id.replace(":", "-")}.md`), "utf8");
      await use({ baseURL, root, item });
    } finally {
      if (child && child.exitCode === null) {
        child.kill("SIGTERM");
        await once(child, "exit");
      }
      await rm(dir, { recursive: true, force: true });
    }
  }
});

export { expect } from "@playwright/test";
