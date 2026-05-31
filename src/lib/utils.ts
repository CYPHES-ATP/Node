import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function truncatePeerId(peerId: string, head = 8, tail = 6) {
  if (!peerId || peerId.length <= head + tail + 3) {
    return peerId || "0xpending";
  }

  return `${peerId.slice(0, head)}...${peerId.slice(-tail)}`;
}

export function formatRelativeTime(timestamp: number) {
  const diff = Math.max(0, Date.now() - timestamp);
  const seconds = Math.floor(diff / 1000);

  if (seconds < 8) return "now";
  if (seconds < 60) return `${seconds}s ago`;

  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;

  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function toTitle(value: string) {
  return value
    .split(/[-_\s]/)
    .filter(Boolean)
    .map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`)
    .join(" ");
}

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export function safeNumber(value: unknown, fallback: number) {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
