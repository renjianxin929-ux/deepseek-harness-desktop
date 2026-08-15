// DeepSeek Harness Desktop V0.1 — startup/loading layer.
// Shows progress from the Rust backend, then the WebView is navigated to the
// official DeepSeek Harness Web UI (http://127.0.0.1:<dynamic-port>).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ErrorInfo {
  version: string;
  runtimePath: string;
  port: number | null;
  reason: string;
  stderrTail: string;
}

export interface StatusPayload {
  phase: string;
  message: string;
  detail: string | null;
  url: string | null;
  error: ErrorInfo | null;
}

const PHASES = ["runtime", "starting", "waiting", "ready"] as const;

function setPhase(phase: string): void {
  const idx = PHASES.indexOf(phase as (typeof PHASES)[number]);
  document.querySelectorAll<HTMLLIElement>("[data-step]").forEach((li) => {
    const step = li.dataset.step ?? "";
    const stepIdx = PHASES.indexOf(step as (typeof PHASES)[number]);
    li.classList.toggle("active", stepIdx === idx && idx >= 0);
    li.classList.toggle("done", stepIdx >= 0 && idx >= 0 && stepIdx < idx);
  });
}

function showError(err: ErrorInfo): void {
  const box = document.getElementById("error");
  const pre = document.getElementById("error-pre");
  if (!box || !pre) return;
  box.classList.remove("hidden");
  const portStr = err.port ? String(err.port) : "(OS-assigned)";
  pre.textContent = [
    `Version      : ${err.version}`,
    `Runtime path : ${err.runtimePath}`,
    `Port         : ${portStr}`,
    ``,
    `Reason: ${err.reason}`,
    err.stderrTail ? `\n--- harness output (tail) ---\n${err.stderrTail}` : "",
  ].join("\n");
}

function apply(p: StatusPayload): void {
  const detail = document.getElementById("detail");
  if (detail) detail.textContent = p.detail ?? "";
  const steps = document.getElementById("steps");
  if (!steps) return;

  if (p.phase === "error" && p.error) {
    setPhase("");
    steps.classList.add("error");
    showError(p.error);
    return;
  }
  steps.classList.remove("error");
  setPhase(p.phase);
  if (p.phase === "ready" && p.url) {
    // Rust normally navigates the WebView; this is a safety net.
    window.setTimeout(() => {
      if (window.location.hostname !== "127.0.0.1") window.location.assign(p.url!);
    }, 1500);
  }
}

async function main(): Promise<void> {
  await listen<StatusPayload>("harness-status", (e) => apply(e.payload));
  try {
    const st = await invoke<StatusPayload>("get_status");
    apply(st);
  } catch (e) {
    console.error("get_status failed", e);
  }
  const retry = document.getElementById("retry");
  if (retry) {
    retry.addEventListener("click", async () => {
      const box = document.getElementById("error");
      const steps = document.getElementById("steps");
      const detail = document.getElementById("detail");
      if (box) box.classList.add("hidden");
      if (steps) steps.classList.remove("error");
      setPhase("");
      if (detail) detail.textContent = "Restarting...";
      try {
        await invoke("restart");
      } catch (e) {
        console.error("restart failed", e);
      }
    });
  }
}

void main();
