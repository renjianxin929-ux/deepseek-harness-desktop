// DeepSeek Harness Desktop — read-only Usage window.
// Displays measured usage and an estimated cost computed by the Rust backend
// from the user's ~/.dsh data (read-only). No settings are modified here.

import { invoke } from "@tauri-apps/api/core";

interface TokenTotals {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

interface UsageSession {
  sessionId: string;
  tokens: TokenTotals;
  totalTokens: number;
  apiCalls: number | null;
  provider: string | null;
  model: string | null;
  estimatedCostUsd: number | null;
  pricingUsed: boolean;
  createdAtMs: number;
}

interface UsageReport {
  available: boolean;
  note: string | null;
  currentSessionId: string | null;
  currentSession: UsageSession | null;
  today: UsageSession | null;
  pricingSnapshotVersion: number;
  pricingNote: string;
  estimation: boolean;
}

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el as T;
};

function fmtTokens(n: number | null | undefined): string {
  if (typeof n !== "number" || !Number.isFinite(n)) return "unavailable";
  return n.toLocaleString("en-US");
}

function fmtUsd(n: number | null | undefined): string {
  if (typeof n !== "number" || !Number.isFinite(n)) return "unavailable";
  return `$${n.toFixed(4)}`;
}

function tokenRow(label: string, value: number | null | undefined): HTMLElement {
  const row = document.createElement("div");
  row.className = "hd-row";
  const l = document.createElement("span");
  l.className = "hd-label";
  l.textContent = label;
  const v = document.createElement("span");
  v.className = "hd-value";
  v.textContent = fmtTokens(value);
  row.append(l, v);
  return row;
}

function renderSession(el: HTMLElement, s: UsageSession | null): void {
  el.innerHTML = "";
  if (!s) {
    const p = document.createElement("p");
    p.className = "hd-empty";
    p.textContent = "unavailable";
    el.appendChild(p);
    return;
  }

  const id = document.createElement("p");
  id.className = "hd-session-id";
  id.textContent = s.sessionId === "TODAY" ? "(aggregate)" : s.sessionId;
  el.appendChild(id);

  const grid = document.createElement("div");
  grid.className = "hd-grid";
  grid.append(
    tokenRow("Estimated cost", undefined),
    tokenRow("API calls", s.apiCalls),
    tokenRow("Total tokens", s.totalTokens),
    tokenRow("Input (uncached)", s.tokens?.inputTokens),
    tokenRow("Cache read", s.tokens?.cacheReadTokens),
    tokenRow("Cache write", s.tokens?.cacheWriteTokens),
    tokenRow("Output", s.tokens?.outputTokens),
  );
  // "Estimated cost" is monetary, not a token count: render via fmtUsd.
  const costVal = grid.querySelector(".hd-row:nth-child(1) .hd-value");
  if (costVal) {
    costVal.textContent = fmtUsd(s.estimatedCostUsd);
    costVal.classList.add("hd-cost");
  }
  el.appendChild(grid);

  const details = document.createElement("details");
  details.className = "hd-details";
  const sum = document.createElement("summary");
  sum.textContent = "Details";
  details.appendChild(sum);
  const body = document.createElement("div");
  body.className = "hd-details-body";

  const rows: [string, string][] = [
    ["Model", s.model ? `${s.provider ?? ""} / ${s.model}` : "unavailable"],
    ["Pricing applied", s.pricingUsed ? "yes (snapshot)" : "no"],
    ["Estimation flag", "true (not official billing)"],
  ];
  for (const [k, v] of rows) {
    const r = document.createElement("div");
    r.className = "hd-row";
    const l = document.createElement("span");
    l.className = "hd-label";
    l.textContent = k;
    const val = document.createElement("span");
    val.className = "hd-value";
    val.textContent = v;
    r.append(l, val);
    body.appendChild(r);
  }
  details.appendChild(body);
  el.appendChild(details);
}

async function main(): Promise<void> {
  try {
    const report = await invoke<UsageReport>("get_usage");
    renderSession($("hd-current"), report.currentSession);
    renderSession($("hd-today"), report.today);
    const note = $("hd-semantics").querySelector(".hd-note");
    if (note) {
      note.textContent =
        `${report.pricingNote} Pricing snapshot v${report.pricingSnapshotVersion}. ` +
        `"Measured Usage" is read from your local Harness data; "Estimated Cost" is ` +
        `a local calculation, never an official bill.`;
    }
  } catch (e) {
    const cur = $("hd-current");
    cur.innerHTML = "";
    const p = document.createElement("p");
    p.className = "hd-empty";
    p.textContent = `unavailable (${String(e)})`;
    cur.appendChild(p);
    const today = $("hd-today");
    today.innerHTML = "";
    today.appendChild(p.cloneNode(true));
  }
}

void main();
