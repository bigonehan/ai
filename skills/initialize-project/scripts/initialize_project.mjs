#!/usr/bin/env node

import { createHash } from "node:crypto";
import { access, mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import { constants, readFileSync } from "node:fs";
import path from "node:path";

const DEFAULT_DOC_ID = "au1EbKLtZbrW";
const DEFAULT_VAULT_ROOT = "/mnt/c/Users/tende/OneDrive/일기장";
const DEFAULT_TEMPLATE = "작업/종류/프로젝트/관리/작업관리기/작업관리기-초기문서.md";
const DEFAULT_GRIST_BASE_URL = "http://localhost:8484";

function loadEnvFile(file) {
  try {
    const content = readFileSync(file, "utf8");
    for (const line of content.split(/\r?\n/u)) {
      const match = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/u);
      if (!match || match[1] in process.env) continue;
      process.env[match[1]] = match[2].replace(/^(["'])(.*)\1$/u, "$2");
    }
  } catch (error) {
    if (error?.code !== "ENOENT") throw new Error(`env-file: .env 읽기 실패`);
  }
}

// Grist credentials come from the shared access-grist skill. A project's own
// working-directory .env is product configuration and is never read here.
const configuredEnvFile = process.env.GRIST_ENV_FILE;
const gristAccessEnvFile = path.join(process.env.HOME || "", ".codex", "skills", "access-grist", ".env");
for (const file of [
  configuredEnvFile,
  gristAccessEnvFile,
  path.resolve(new URL("..", import.meta.url).pathname, ".env"),
].filter(Boolean)) loadEnvFile(file);

function fail(boundary, error) {
  const message = error instanceof Error ? error.message : String(error);
  const result = { status: "runtime-unverified", first_failing_boundary: boundary, error: message };
  process.stderr.write(`${JSON.stringify(result)}\n`);
  process.exitCode = 2;
  return result;
}

function args(argv) {
  const values = new Map();
  for (let index = 2; index < argv.length; index += 1) {
    const value = argv[index];
    if (!value.startsWith("--")) throw new Error(`알 수 없는 인자: ${value}`);
    const key = value.slice(2);
    const next = argv[index + 1];
    if (!next || next.startsWith("--")) throw new Error(`인자 값이 없습니다: --${key}`);
    values.set(key, next);
    index += 1;
  }
  const projectRoot = values.get("project-root");
  const projectName = values.get("project-name");
  if (!projectRoot || !projectName) throw new Error("--project-root와 --project-name이 필요합니다.");
  return {
    projectRoot: path.resolve(projectRoot),
    projectName,
    operationId: values.get("operation-id") || `initialize-project-${Date.now()}`,
    docId: values.get("grist-doc-id") || process.env.GRIST_DOC_ID || DEFAULT_DOC_ID,
    table7: values.get("table7") || "Table7",
    table26: values.get("table26") || "Table26",
    gristBaseUrl: values.get("grist-base-url") || process.env.GRIST_BASE_URL || DEFAULT_GRIST_BASE_URL,
    gristApiKey: values.get("grist-api-key") || process.env.GRIST_API_KEY || "",
    vaultRoot: path.resolve(values.get("vault-root") || DEFAULT_VAULT_ROOT),
    templatePath: values.get("template-path") || DEFAULT_TEMPLATE,
    receipt: values.get("receipt") || null,
    request: values.get("request") || "완료되지 않은 사용자의 요청",
    category: values.get("category") === "오류" ? "오류" : "기능",
  };
}

function unixTimestamp() {
  return Math.floor(Date.now() / 1000);
}

// Table7 마감일(Date): 실행 시점(Asia/Seoul) 기준 내년 12월 3일 자정(UTC), Unix 초.
function nextYearDecember3Timestamp(date = new Date()) {
  const year = Number(new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Seoul", year: "numeric" }).format(date));
  return Date.UTC(year + 1, 11, 3) / 1000;
}

function datePrefix(date = new Date()) {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Seoul", year: "2-digit", month: "2-digit", day: "2-digit" }).formatToParts(date);
  const values = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${values.year}${values.month}${values.day}`;
}

function hash(value) {
  return createHash("sha256").update(value).digest("hex");
}

function relativeFile(vaultRoot, relativePath) {
  if (typeof relativePath !== "string" || !relativePath || relativePath.includes("\\") || relativePath.includes("%") || relativePath.startsWith("/")) {
    throw new Error("Obsidian 경로는 percent encoding 없는 Vault 상대경로여야 합니다.");
  }
  const segments = relativePath.split("/");
  if (segments.some((segment) => !segment || segment === "." || segment === "..")) throw new Error("Obsidian 경로 구간이 올바르지 않습니다.");
  const root = path.resolve(vaultRoot);
  const file = path.resolve(root, ...segments);
  if (file !== root && !file.startsWith(`${root}${path.sep}`)) throw new Error("Obsidian 경로가 Vault 경계를 벗어났습니다.");
  return file;
}

async function readable(file) {
  await access(file, constants.R_OK);
  const info = await stat(file);
  if (!info.isFile()) throw new Error(`파일이 아닙니다: ${path.basename(file)}`);
  return readFile(file, "utf8");
}

async function createMarkdown(vaultRoot, relativePath, source) {
  const file = relativeFile(vaultRoot, relativePath);
  await mkdir(path.dirname(file), { recursive: true });
  try {
    await writeFile(file, source, { encoding: "utf8", flag: "wx" });
  } catch (error) {
    if (!(error && typeof error === "object" && error.code === "EEXIST")) throw error;
  }
  const content = await readable(file);
  return { relativePath, bytes: Buffer.byteLength(content, "utf8"), sha256: hash(content), content };
}

async function writeTextAtomically(file, content) {
  await mkdir(path.dirname(file), { recursive: true });
  const temporary = `${file}.initialize-project-${process.pid}-${Date.now()}`;
  await writeFile(temporary, content, { encoding: "utf8", flag: "wx" });
  await rename(temporary, file);
  return readable(file);
}

async function upsertChangeRecord(projectRoot, prefix, projectName, taskDescription, taskId) {
  const changeDirectory = path.join(projectRoot, ".ai", "changes");
  const changePath = path.join(changeDirectory, `${prefix}.${projectName}.md`);
  const item = `- [ ] ${taskDescription}\n  - Table26 Raw ID: ${taskId}\n  - 완료 조건: Table26의 C가 완료이고 Grist와 Obsidian readback이 일치해야 합니다.`;
  let content = "";
  try { content = await readable(changePath); } catch (error) {
    if (error?.code !== "ENOENT" && !String(error?.message || "").includes("ENOENT")) throw error;
  }
  if (!content) content = `# jobs\n\n${item}\n`;
  else if (!content.includes(item)) {
    if (!content.includes("# jobs")) content = `# jobs\n\n${content.trimEnd()}\n\n${item}\n`;
    else content = `${content.trimEnd()}\n\n${item}\n`;
  }
  await writeTextAtomically(changePath, content);
  const readback = await readable(changePath);
  if (!readback.includes(taskDescription) || !readback.includes(`Table26 Raw ID: ${taskId}`)) {
    throw new Error("change-record-readback: dated .ai/changes record does not contain the atomic item");
  }
  return { path: changePath, bytes: Buffer.byteLength(readback, "utf8"), sha256: hash(readback) };
}

function gristUrl(baseUrl, docId, suffix) {
  const base = new URL(baseUrl);
  const [pathname, search = ""] = suffix.split("?", 2);
  base.pathname = `${base.pathname.replace(/\/+$/u, "")}/api/docs/${encodeURIComponent(docId)}${pathname}`;
  base.search = search ? `?${search}` : "";
  return base;
}

async function gristRequest(options, method, suffix, body) {
  const headers = { Accept: "application/json" };
  if (options.gristApiKey.trim()) headers.Authorization = `Bearer ${options.gristApiKey.trim()}`;
  const serialized = body === undefined ? undefined : JSON.stringify(body);
  if (serialized !== undefined) headers["Content-Type"] = "application/json";
  let response;
  try {
    response = await fetch(gristUrl(options.gristBaseUrl, options.docId, suffix), { method, headers, body: serialized });
  } catch (error) {
    throw new Error(`grist-transport: ${error instanceof Error ? error.message : String(error)}`);
  }
  const text = await response.text();
  let value = null;
  try { value = text ? JSON.parse(text) : null; } catch { throw new Error(`grist-response: JSON 응답을 읽을 수 없습니다 (HTTP ${response.status}).`); }
  if (!response.ok) {
    const safeDetail = typeof value?.error === "string" ? value.error.slice(0, 160) : "";
    throw new Error(`grist-http: ${method} ${suffix} HTTP ${response.status} 응답으로 Grist 요청이 거부됐습니다.${safeDetail ? ` ${safeDetail}` : ""}`);
  }
  return value;
}

function recordRows(value) {
  if (!Array.isArray(value?.records)) throw new Error("Grist records 응답 형식이 올바르지 않습니다.");
  return value.records.filter((record) => Number.isSafeInteger(record?.id) && record.fields && typeof record.fields === "object").map((record) => ({ id: record.id, ...record.fields }));
}

function columnId(columns, label, fallback) {
  const match = columns.find((column) => column?.id === label || column?.fields?.label === label);
  return match?.id || fallback;
}

async function loadGristSchema(options) {
  const [table7Columns, table26Columns] = await Promise.all([
    gristRequest(options, "GET", `/tables/${encodeURIComponent(options.table7)}/columns`),
    gristRequest(options, "GET", `/tables/${encodeURIComponent(options.table26)}/columns`),
  ]);
  if (!Array.isArray(table7Columns?.columns) || !Array.isArray(table26Columns?.columns)) throw new Error("grist-schema: Grist column 응답 형식이 올바르지 않습니다.");
  return {
    table7: options.table7,
    table26: options.table26,
    obsidian: columnId(table7Columns.columns, "Obsidian_Id", "Obsidian_Id"),
  };
}

async function fetchRows(options, table) {
  return recordRows(await gristRequest(options, "GET", `/tables/${encodeURIComponent(table)}/records`));
}

async function addRecord(options, table, fields) {
  return gristRequest(options, "POST", `/tables/${encodeURIComponent(table)}/records?noparse=true`, { records: [{ fields }] });
}

async function initialize(options) {
  const prefix = datePrefix();
  const timestamp = unixTimestamp();
  const projectFile = `작업/종류/프로젝트/코딩/${prefix}.${options.projectName}.md`;
  const templateFile = relativeFile(options.vaultRoot, options.templatePath);
  const template = await readable(templateFile);
  const projectRoot = options.projectRoot;
  const evidence = { operation_id: options.operationId, status: "in-progress", project_root: projectRoot, obsidian: {}, grist: {}, tasks: [] };

  evidence.grist.transport = "grist-rest-api";
  const schema = await loadGristSchema(options);
  const existing = await fetchRows(options, options.table7);
  const managementRow = existing.find((row) => row.C === "관리 프로그램");
  const managementDocument = managementRow?.[schema.obsidian];
  if (!managementRow || typeof managementDocument !== "string" || !managementDocument) throw new Error("grist-management-row: Table7에서 C가 관리 프로그램인 행의 Obsidian_Id를 찾지 못했습니다.");
  const managementFile = relativeFile(options.vaultRoot, managementDocument);
  const managementBefore = await readable(managementFile);
  const duplicate = existing.find((row) => row[schema.obsidian] === projectFile || row["C"] === options.projectName);
  let rawId = duplicate?.id;
    const table7Fields = {
      H: "코딩", P: "개인", D: "진행", M: "서류", C: options.projectName, A: timestamp,
      [schema.obsidian]: projectFile,
      I: "없음", L: "없음", K: "", E: "없음", F: nextYearDecember3Timestamp(),
  };
  if (!rawId) {
      await addRecord(options, options.table7, table7Fields);
      const after = await fetchRows(options, options.table7);
      const created = after.find((row) => row[schema.obsidian] === projectFile && row.C === options.projectName);
      if (!created) throw new Error("Table7 생성 후 동일한 Obsidian_Id·프로젝트명을 가진 행을 readback하지 못했습니다.");
      rawId = created.id;
  }
  const table7After = (await fetchRows(options, options.table7)).find((row) => row.id === rawId);
    if (!table7After || table7After[schema.obsidian] !== projectFile) throw new Error("Table7 Raw ID 또는 Obsidian_Id readback 값이 일치하지 않습니다.");
  evidence.grist.table7 = { raw_id: rawId, management_path: managementDocument, obsidian_path: projectFile };

  const existingTasks = await fetchRows(options, options.table26);
  const taskDescription = options.request;
  const taskName = taskDescription.slice(0, 80) || options.projectName;
  const task = existingTasks.find((row) => row.A === rawId && row.D === taskDescription && row.B === taskName);
  let taskId = task?.id;
  if (!taskId) {
      await addRecord(options, options.table26, { A: rawId, G: options.category, B: taskName, H: "초안", D: taskDescription, E: timestamp, F: "" });
      const afterTasks = await fetchRows(options, options.table26);
      const created = afterTasks.find((row) => row.A === rawId && row.D === taskDescription && row.B === taskName);
      if (!created) throw new Error("Table26 생성 후 atomic task를 readback하지 못했습니다.");
      taskId = created.id;
  }
  const taskAfter = (await fetchRows(options, options.table26)).find((row) => row.id === taskId);
    if (!taskAfter || taskAfter.A !== rawId || taskAfter.H !== "초안") throw new Error("Table26 부모 Raw ID·초안 단계 readback이 일치하지 않습니다.");
  evidence.grist.table26 = [{ raw_id: taskId, parent_raw_id: rawId, stage: taskAfter.H }];

  const rendered = template.replaceAll("{$C}", options.projectName).replaceAll("{$A}", prefix);
  const note = await createMarkdown(options.vaultRoot, projectFile, rendered);
    const taskLine = `- [ ] ${taskDescription}`;
    const managementContent = managementBefore.includes(taskLine)
      ? managementBefore
      : managementBefore.includes("## tasks")
        ? managementBefore.replace(/(^- \[x\] 완료된 사용자의 요청.*$)/mu, `${taskLine}\n$1`)
        : `${managementBefore.trimEnd()}\n\n## tasks\n${taskLine}\n- [x] 완료된 사용자의 요청\n`;
    if (managementContent !== managementBefore) await writeTextAtomically(managementFile, managementContent);
    const managementReadback = await readable(managementFile);
    if (!managementReadback.includes(taskLine)) throw new Error("obsidian-management-readback: 관리 프로그램 문서에서 새 task를 readback하지 못했습니다.");
  const finalNote = await readable(relativeFile(options.vaultRoot, projectFile));
  evidence.obsidian.project = { path: projectFile, bytes: Buffer.byteLength(finalNote, "utf8"), sha256: hash(finalNote) };
  const changeRecord = await upsertChangeRecord(projectRoot, prefix, options.projectName, taskDescription, taskId);

  const agents = `# AGENTS.md\n\n- Table7 Raw ID: ${rawId}\n- 관리 프로그램: ${managementDocument}\n- Obsidian 프로젝트 문서: ${projectFile}\n- Table26 task Raw ID: ${taskId}\n- 테스트에 필요한 임시 파일만 \`test/tmp/\`에 만들고 작업 후 지웁니다.\n- Table26 \`$H\`에는 \`초안\`, \`진단\`, \`교정\`, \`검증\`, \`대기\`, \`완료\` 중 하나만 씁니다.\n- Table26 \`$D\`·\`$I\`·\`$J\`에는 지금 진행 중인 최신 항목 한 줄만 작업 행 안의 순번과 함께 쓰고(예: \`3. 인증 단계 확립\`), 1번부터의 전체 목록과 세부 내용은 같은 순번으로 날짜별 \`.ai/changes\`에만 기록합니다.\n`;
  await writeTextAtomically(path.join(projectRoot, "AGENTS.md"), agents);
  const input = `## tasks\n- [ ] ${taskDescription}\n- [x] 완료된 사용자의 요청\n`;
  await writeTextAtomically(path.join(projectRoot, "Input.md"), input);
  evidence.files = { agents: path.join(projectRoot, "AGENTS.md"), input: path.join(projectRoot, "Input.md"), change: changeRecord.path };
    evidence.management = { path: managementDocument, bytes: Buffer.byteLength(managementReadback, "utf8"), sha256: hash(managementReadback), tasks: true };
  evidence.status = "complete";
  return evidence;
}

try {
  const options = args(process.argv);
  const result = await initialize(options);
  const receiptPath = options.receipt || path.join(options.projectRoot, "test", "tmp", "initialization-receipt.json");
  await writeTextAtomically(receiptPath, `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(result)}\n`);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  const boundary = message.includes(":") ? message.slice(0, message.indexOf(":")) : "initialize-project";
  const result = fail(boundary, error);
  try {
    const options = args(process.argv);
    const receiptPath = options.receipt || path.join(process.cwd(), "test", "tmp", `${options.operationId}.json`);
    await writeTextAtomically(receiptPath, `${JSON.stringify({ ...result, operation_id: options.operationId }, null, 2)}\n`);
  } catch {}
}
