// Demo fixture for T02 shell (browser mock + native shell preview).
// Static data shaped like real IPC DTOs; real wiring arrives T04–T08.
// Never used as Git truth: production paths must call the typed IPC client.

export interface DemoCommit {
  oid: string;
  shortOid: string;
  subject: string;
  authorName: string;
  committedAt: string;
  refs: string[];
  parents: string[];
  /** Static lane for the T02 placeholder graph; T05 computes lanes properly. */
  lane: number;
  merge: boolean;
}

export interface DemoFile {
  path: string;
  indexStatus: string;
  worktreeStatus: string;
  staged: boolean;
  conflicted: boolean;
}

export interface DemoConflict {
  path: string;
  kind: "content" | "modifyDelete";
  currentBranch: string;
  incomingBranch: string;
}

export const demoBranches = ["main", "feature/ui", "fix/login-crash"];

export const demoCommits: DemoCommit[] = [
  {
    oid: "c9f1a2b3c9f1a2b3c9f1a2b3c9f1a2b3c9f1a2b3",
    shortOid: "c9f1a2b",
    subject: "Merge branch 'feature/ui' into main",
    authorName: "A. Nguyen",
    committedAt: "2026-09-21 14:02 +07:00",
    refs: ["main"],
    parents: ["a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4", "e5f60718e5f60718e5f60718e5f60718e5f60718"],
    lane: 0,
    merge: true
  },
  {
    oid: "e5f60718e5f60718e5f60718e5f60718e5f60718",
    shortOid: "e5f6071",
    subject: "Add inspector conflict panel",
    authorName: "B. Tran",
    committedAt: "2026-09-21 11:47 +07:00",
    refs: ["feature/ui"],
    parents: ["77aa991077aa991077aa991077aa991077aa9910"],
    lane: 1,
    merge: false
  },
  {
    oid: "a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4a1b2c3d4",
    shortOid: "a1b2c3d",
    subject: "Fix login crash on empty token",
    authorName: "A. Nguyen",
    committedAt: "2026-09-20 17:22 +07:00",
    refs: ["fix/login-crash"],
    parents: ["77aa991077aa991077aa991077aa991077aa9910"],
    lane: 0,
    merge: false
  },
  {
    oid: "77aa991077aa991077aa991077aa991077aa9910",
    shortOid: "77aa991",
    subject: "Update commit graph lanes",
    authorName: "C. Le",
    committedAt: "2026-09-20 09:05 +07:00",
    refs: [],
    parents: [],
    lane: 0,
    merge: false
  }
];

export const demoUnstaged: DemoFile[] = [
  { path: "src/app/App.svelte", indexStatus: " ", worktreeStatus: "M", staged: false, conflicted: false },
  { path: "src/lib/ipc/client.ts", indexStatus: "M", worktreeStatus: "M", staged: false, conflicted: false },
  { path: "docs/development.md", indexStatus: " ", worktreeStatus: "?", staged: false, conflicted: false }
];

export const demoStaged: DemoFile[] = [
  { path: "src/lib/ipc/types.ts", indexStatus: "M", worktreeStatus: " ", staged: true, conflicted: false },
  { path: "src-tauri/src/domain/error.rs", indexStatus: "M", worktreeStatus: " ", staged: true, conflicted: false }
];

export const demoConflicts: DemoFile[] = [
  { path: "src/app/HistoryPane.svelte", indexStatus: "U", worktreeStatus: "U", staged: false, conflicted: true }
];

export const demoConflictDetails: DemoConflict[] = [
  {
    path: "src/app/HistoryPane.svelte",
    kind: "content",
    currentBranch: "main",
    incomingBranch: "feature/ui"
  }
];

export const demoDiffPreview = `@@ -12,7 +12,9 @@
   rows.map((row) => paint(row));
 -  lane = nextFreeLane();
 +  // keep lane across page boundary
 +  lane = carryLane(row);
   select(row);`;

export const DEMO_COMMIT_MESSAGE_DRAFT = "";
