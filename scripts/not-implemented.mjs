// Placeholder for script contracts not yet implemented by the current task.
// Per docs/10-build-release.md: report `not yet implemented`, never a fake pass.
const [script, firstTask] = process.argv.slice(2);
process.stderr.write(`not yet implemented: ${script ?? "script"} (planned ${firstTask ?? "later task"})\n`);
process.exit(1);
