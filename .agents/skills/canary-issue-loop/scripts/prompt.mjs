#!/usr/bin/env node

import { readFile } from "node:fs/promises";

const issue = process.argv[2]?.replace(/^#/, "");
if (!issue || !/^\d+$/.test(issue)) {
  console.error("Usage: prompt.mjs <issue-number>");
  process.exit(2);
}

const policy = await readFile(new URL("../../../../.agent-loop/canary-issue-pr-loop.md", import.meta.url), "utf8");
const prompt = policy.match(/```text\n([\s\S]*?)\n```/);
if (!prompt) throw new Error("Repository prompt template has no text prompt.");
process.stdout.write(`${prompt[1].replaceAll("ISSUE_NUMBER", issue)}\n`);
