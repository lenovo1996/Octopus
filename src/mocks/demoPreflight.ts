import type { PreflightData } from "../lib/ipc/types";

/**
 * Demo fixture for browser preview only. Values mirror a real Ubuntu 24.04
 * baseline (Git 2.43.0) but are static — never a substitute for `app_preflight`.
 */
export const demoPreflight: PreflightData = {
  gitPath: "/usr/bin/git (demo)",
  gitVersion: "2.43.0",
  gitSupported: true,
  platform: "linux (demo)",
  arch: "x86_64 (demo)",
  features: { localOperations: true }
};
