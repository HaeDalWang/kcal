// IPC fixture 계약 테스트(ipc.test.ts, ipc-coach.test.ts)가 함께 쓰는 도우미.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect } from "vitest";

import type { Agent, Loading } from "@/lib/ipc";

export type Fields<T> = Record<keyof T, "required" | "optional">;
export type Values<T extends string> = Record<T, true>;

export function fixture(name: string): unknown {
  // vitest는 저장소 루트에서 돈다.
  return JSON.parse(readFileSync(resolve("src/test/fixtures/ipc", `${name}.json`), "utf8"));
}

/** 키 집합이 타입과 같은가: 모르는 키 없음, 필수 키 빠짐 없음. */
export function expectFields<T>(value: unknown, fields: Fields<T>, where: string): void {
  expect(typeof value === "object" && value !== null, `${where} is an object`).toBe(true);
  const keys = Object.keys(value as object);
  const required = Object.entries(fields).flatMap(([k, v]) => (v === "required" ? [k] : []));
  expect(
    keys.filter((k) => !(k in fields)),
    `${where}: keys unknown to ipc.ts`,
  ).toEqual([]);
  expect(
    required.filter((k) => !keys.includes(k)),
    `${where}: required keys missing`,
  ).toEqual([]);
}

/** enum: fixture에 나온 값이 모두 타입의 값이고, 타입의 값이 모두 fixture에 나온다. */
export function expectValues<T extends string>(
  found: unknown[],
  allowed: Values<T>,
  where: string,
): void {
  expect([...new Set(found)].sort(), where).toEqual(Object.keys(allowed).sort());
}

export const AGENT = { claude: true, codex: true, kiro: true } satisfies Values<Agent>;
export const LOADING = {
  startup: true,
  conditional: true,
  deferred: true,
} satisfies Values<Loading>;
