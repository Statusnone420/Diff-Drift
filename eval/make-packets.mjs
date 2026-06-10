#!/usr/bin/env node
import { parseCaseArgs, loadCases } from "./lib/cases.mjs";
import { writeBlindPacket } from "./lib/packets.mjs";
import { createCaseRepo } from "./lib/repo.mjs";

const args = parseCaseArgs(process.argv.slice(2));
const cases = await loadCases(args.ids);
const packets = [];

for (const caseDef of cases) {
  const fixture = createCaseRepo(caseDef);
  try {
    const packetDir = writeBlindPacket(caseDef, fixture);
    packets.push({ caseId: caseDef.id, packetDir });
    console.log(`PACKET ${caseDef.id} -> ${packetDir}`);
  } finally {
    if (!args.keep) {
      fixture.cleanup();
    } else {
      console.log(`  kept fixture: ${fixture.repoPath}`);
    }
  }
}

if (args.json) {
  console.log(JSON.stringify({ packets }, null, 2));
}
