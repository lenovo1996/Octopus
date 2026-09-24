# T15 — Performance report (engine-level git spawn timings)

Date: 2026-09-22. Method: `/tmp/bench15.sh`, 20 warm runs each, same argv
the engine uses (`STATUS_ARGV`, `rev-list --topo-order --parents
--boundary`, `cat-file --batch`, worktree `diff` flags). Rust parse, IPC
and render costs excluded (small vs spawn; stated, not hidden).

Dataset `/tmp/perf-repo`: 10 000 commits (fast-import), 5 000 tracked
files, 101 untracked files (status case), one 200 KiB tracked text file
with a 1-line change (diff case). Cold = first run after generation (OS
caches not dropped — per doc, no sudo cache dropping).

Machine: Intel i5-10400 @ 2.90GHz, 12 CPUs, 31 GiB RAM, Ubuntu 24.04.4
LTS, Git 2.43.0, app 0.1.0 release bundle (numbers are git-spawn only).

| Budget (§5) | Dataset | Target | Measured (n=20 warm) | Pass |
|---|---|---|---|---|
| Status refresh | 5k files, 101 changes | warm p95 ≤ 1 s | median 7 ms, p95 8–10 ms | yes |
| Time to usable history (first 200) | 10k commits | warm p95 ≤ 2 s | rev-list p95 41 ms + cat-file-200 p95 2 ms ≈ 43 ms + parse | yes |
| Small diff | 200 KiB, 1-line change | warm p95 ≤ 300 ms | median 4 ms, p95 4 ms | yes |
| Selection/scroll UI | loaded rows, no network | ≤ 100 ms / frame p95 ≤ 33 ms | NOT MEASURED (no UI frame harness) | open |
| Memory RSS ≤ 350 MiB | idle + 1k rows | target | NOT MEASURED (no native profiler run) | open |

Do not compare these numbers across machines as regressions; they are a
single-machine sanity gate, not a benchmark baseline.
